'use strict';

const fs = require('node:fs');
const path = require('node:path');
const { constants: bufferConstants } = require('node:buffer');

const TARGETS = Object.freeze({
  'win32-x64': 'kenvoy.exe',
  'win32-arm64': 'kenvoy.exe',
  'linux-x64': 'kenvoy',
  'linux-arm64': 'kenvoy',
  'darwin-x64': 'kenvoy',
  'darwin-arm64': 'kenvoy',
});
const OFFICIAL_HOSTS = new Set([
  'github.com',
  'objects.githubusercontent.com',
  'release-assets.githubusercontent.com',
  'github-releases.githubusercontent.com',
]);
const rebuildHelp = '局部安装请执行 npm rebuild kenvoy；全局安装请执行 npm rebuild -g kenvoy。若被 npm 12 拦截：局部先 npm install-scripts approve kenvoy，全局为 rebuild 添加 --allow-scripts=kenvoy。';

function assertOfficialUrl(value) {
  let url;
  try {
    url = new URL(value);
  } catch {
    throw new Error('下载地址不是合法 URL。');
  }
  if (url.protocol !== 'https:' || !OFFICIAL_HOSTS.has(url.hostname)
      || url.port || url.username || url.password || url.hash) {
    throw new Error('下载只允许无认证信息的官方 GitHub HTTPS 地址。');
  }
  return url;
}

function assertMemberPath(name, directoryAllowed = false) {
  if (typeof name !== 'string' || !name || /[\\:\u0000-\u001f\u007f]/.test(name)) {
    throw new Error('ZIP 含非法成员路径。');
  }
  const normalized = directoryAllowed && name.endsWith('/') ? name.slice(0, -1) : name;
  if (normalized.split('/').some((part) => !part || part === '.' || part === '..')) {
    throw new Error('ZIP 含非法成员路径。');
  }
}

function readJson(root, name) {
  try {
    return JSON.parse(fs.readFileSync(path.join(root, name), 'utf8'));
  } catch {
    throw new Error(`无法读取 ${name}；请重新安装 kenvoy。`);
  }
}

function isObject(value) {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function assertTarget(target, executable, version, repository) {
  if (!isObject(target) || target.executable !== executable) {
    throw new Error('下载清单的可执行文件名称无效。');
  }
  for (const field of ['archiveSha256', 'binarySha256']) {
    if (typeof target[field] !== 'string' || !/^[a-f0-9]{64}$/.test(target[field])) {
      throw new Error(`下载清单缺少有效的 ${field}。`);
    }
  }
  for (const field of ['archiveSize', 'binarySize']) {
    if (!Number.isSafeInteger(target[field]) || target[field] <= 0
        || target[field] > bufferConstants.MAX_LENGTH) {
      throw new Error(`下载清单的 ${field} 无效。`);
    }
  }
  assertMemberPath(target.member);
  if (target.member.split('/').at(-1) !== executable) {
    throw new Error('下载清单的 ZIP 成员与可执行文件不一致。');
  }
  const url = assertOfficialUrl(target.url);
  const prefix = `/${repository}/releases/download/v${version}/`;
  let pathname;
  try {
    pathname = decodeURIComponent(url.pathname);
  } catch {
    throw new Error('下载清单的 Release 路径无效。');
  }
  const filename = pathname.slice(prefix.length);
  if (url.hostname !== 'github.com' || url.search || !pathname.startsWith(prefix)
      || !filename.endsWith('.zip') || filename.includes('/') || filename.includes('\\')) {
    throw new Error('初始下载地址必须属于对应版本的官方 Kenvoy Release。');
  }
}

function selectTarget(packageRoot, platform = process.platform, arch = process.arch) {
  const key = `${platform}-${arch}`;
  if (!Object.hasOwn(TARGETS, key)) {
    throw new Error(`Kenvoy CLI 不支持当前平台 ${key}；支持 Windows、Linux、macOS 的 x64/arm64。`);
  }
  const root = fs.realpathSync(packageRoot);
  const metadata = readJson(root, 'package.json');
  const manifest = readJson(root, 'download-manifest.json');
  if (!isObject(metadata) || metadata.name !== 'kenvoy' || !isObject(manifest)
      || manifest.formatVersion !== 1 || typeof manifest.repository !== 'string'
      || /^[A-Za-z0-9][A-Za-z0-9-]{0,38}\/[A-Za-z0-9._-]{1,100}$/.exec(manifest.repository)?.[0] !== manifest.repository
      || typeof manifest.version !== 'string'
      || !/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(manifest.version)
      || metadata.version !== manifest.version || !isObject(manifest.targets)) {
    throw new Error('kenvoy 包信息与下载清单不一致；请重新安装。');
  }
  const keys = Object.keys(manifest.targets);
  if (keys.length !== Object.keys(TARGETS).length || keys.some((item) => !Object.hasOwn(TARGETS, item))) {
    throw new Error('下载清单必须完整包含六个受支持的平台。');
  }
  for (const [targetKey, executable] of Object.entries(TARGETS)) {
    assertTarget(manifest.targets[targetKey], executable, manifest.version, manifest.repository);
  }
  const target = manifest.targets[key];
  return {
    root, manifest, target, key,
    binaryPath: path.join(root, 'vendor', target.binarySha256, target.executable),
  };
}

module.exports = { selectTarget, assertOfficialUrl, assertMemberPath, rebuildHelp };
