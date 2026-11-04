'use strict';

const https = require('node:https');
const { HttpsProxyAgent } = require('https-proxy-agent');
const { getProxyForUrl } = require('proxy-from-env');
const { assertOfficialUrl, rebuildHelp } = require('./config.cjs');

const REDIRECTS = new Set([301, 302, 303, 307, 308]);
const MAX_REDIRECTS = 5;

function proxyForUrl(url) {
  // npm 的 noproxy 配置名称与 proxy-from-env 的 no_proxy 别名不同。
  const alias = process.env.npm_config_noproxy || process.env.NPM_CONFIG_NOPROXY;
  if (!alias || process.env.npm_config_no_proxy || process.env.NPM_CONFIG_NO_PROXY) {
    return getProxyForUrl(url);
  }
  const previous = process.env.npm_config_no_proxy;
  try {
    process.env.npm_config_no_proxy = alias;
    return getProxyForUrl(url);
  } finally {
    if (previous === undefined) delete process.env.npm_config_no_proxy;
    else process.env.npm_config_no_proxy = previous;
  }
}

function proxyAgent(url, signal) {
  const proxy = proxyForUrl(url.href);
  if (!proxy) return false;
  try {
    const address = new URL(proxy);
    if (!['http:', 'https:'].includes(address.protocol)) throw new Error();
    const headers = {};
    if (address.username || address.password) {
      const auth = `${decodeURIComponent(address.username)}:${decodeURIComponent(address.password)}`;
      headers['Proxy-Authorization'] = `Basic ${Buffer.from(auth).toString('base64')}`;
      address.username = '';
      address.password = '';
    }
    // 凭据只进入 CONNECT 头，不让代理库的 URL 调试日志记录认证信息。
    return new HttpsProxyAgent(address, { signal, headers, rejectUnauthorized: true });
  } catch {
    throw new Error('代理地址无效；请检查 npm proxy/https-proxy 或 HTTPS_PROXY 配置。');
  }
}

function networkError(error) {
  const code = typeof error.code === 'string' && /^[A-Z0-9_]+$/.test(error.code)
    ? error.code : 'NETWORK_ERROR';
  return new Error(`下载 Kenvoy CLI 失败（${code}）；请检查网络与代理。${rebuildHelp}`);
}

function requestArchive(url, size, deadline, idleTimeoutMs, request) {
  return new Promise((resolve, reject) => {
    const controller = new AbortController();
    let agent;
    let req;
    let response;
    let timer;
    let settled = false;
    const finish = (error, result) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      if (error || result.redirect) {
        controller.abort();
        if (response) response.destroy();
        if (req) req.destroy();
      }
      if (agent) agent.destroy();
      if (error) reject(error);
      else resolve(result);
    };
    const remaining = deadline - Date.now();
    if (remaining <= 0) {
      reject(new Error(`下载 Kenvoy CLI 超时；请检查网络。${rebuildHelp}`));
      return;
    }
    timer = setTimeout(() => finish(new Error('下载 Kenvoy CLI 超时。')), remaining);
    try {
      agent = proxyAgent(url, controller.signal);
      req = request(url, {
        agent,
        signal: controller.signal,
        rejectUnauthorized: true,
        headers: {
          Accept: 'application/octet-stream',
          'Accept-Encoding': 'identity',
          'User-Agent': 'kenvoy-npm',
        },
      }, (incoming) => {
        if (settled) { incoming.destroy(); return; }
        response = incoming;
        response.once('error', (error) => finish(networkError(error)));
        response.once('aborted', () => finish(new Error('ZIP 下载被中断。')));
        const status = response.statusCode;
        if (REDIRECTS.has(status)) {
          if (typeof response.headers.location !== 'string' || !response.headers.location.trim()) {
            finish(new Error('下载重定向缺少 Location。'));
            return;
          }
          finish(null, { redirect: response.headers.location });
          return;
        }
        if (status !== 200) {
          finish(new Error(`下载 Kenvoy CLI 失败（HTTP ${status}）。`));
          return;
        }
        const length = response.headers['content-length'];
        if (length !== undefined && (!/^\d+$/.test(length) || Number(length) !== size)) {
          finish(new Error('ZIP 响应长度与清单不一致。'));
          return;
        }
        if (response.headers['content-encoding'] && response.headers['content-encoding'] !== 'identity') {
          finish(new Error('ZIP 响应不能使用额外的内容编码。'));
          return;
        }
        let bytes;
        try {
          bytes = Buffer.allocUnsafe(size);
        } catch {
          finish(new Error('无法为 ZIP 下载分配所需内存。'));
          return;
        }
        let received = 0;
        response.on('data', (chunk) => {
          if (settled) return;
          if (received + chunk.length > size) {
            finish(new Error('ZIP 下载长度超过清单声明。'));
            return;
          }
          chunk.copy(bytes, received);
          received += chunk.length;
        });
        response.once('end', () => {
          if (received !== size) finish(new Error('ZIP 下载长度与清单不一致。'));
          else finish(null, { bytes });
        });
      });
      req.once('error', (error) => finish(networkError(error)));
      req.setTimeout(Math.min(idleTimeoutMs, remaining), () => finish(new Error('ZIP 下载连接超时。')));
    } catch (error) {
      finish(error.message.startsWith('代理地址') ? error : networkError(error));
    }
  });
}

async function download(target, options = {}) {
  const timeoutMs = options.timeoutMs ?? 120_000;
  const idleTimeoutMs = options.idleTimeoutMs ?? 30_000;
  if (![timeoutMs, idleTimeoutMs].every((value) => Number.isSafeInteger(value) && value > 0 && value <= 2_147_483_647)) {
    throw new Error('下载超时必须是有效的正整数毫秒数。');
  }
  const deadline = Date.now() + timeoutMs;
  let url = assertOfficialUrl(target.url);
  for (let redirects = 0; ; redirects += 1) {
    const result = await requestArchive(url, target.archiveSize, deadline, idleTimeoutMs, options.request ?? https.get);
    if (!result.redirect) return result.bytes;
    if (redirects >= MAX_REDIRECTS) throw new Error('下载重定向次数超过限制。');
    let next;
    try {
      next = new URL(result.redirect, url);
    } catch {
      throw new Error('下载重定向地址无效。');
    }
    url = assertOfficialUrl(next.href);
  }
}

module.exports = { download };
