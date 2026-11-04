'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const https = require('node:https');
const http = require('node:http');
const net = require('node:net');
const { execFile } = require('node:child_process');
const { createHash } = require('node:crypto');
const { createRequire } = require('node:module');
const runtime = path.resolve(__dirname, '../npm_runtime');
const runtimeRequire = createRequire(path.join(runtime, 'package.json'));
const { zipSync } = runtimeRequire('fflate');
const { install } = require('../npm_runtime/lib/install.cjs');

// 仅供回环 HTTPS 测试的自签名证书；不是发布凭据，不禁用 TLS 校验。
const KEY = `-----BEGIN PRIVATE KEY-----
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgMOQO/l9Jxn9ASXE+
Rsim1BzqpgZ2hQgdfEjANgMUVPehRANCAAT9sTTX0imVMXJ2YuMKV/XDIUGD2CMa
d6CUcKNoTMbHBEq61/6JPYCdBon6RtcPghrEdMLJMYsx9JjanYR1viPT
-----END PRIVATE KEY-----`;
const CERT = `-----BEGIN CERTIFICATE-----
MIIB6TCCAY+gAwIBAgIBATAKBggqhkjOPQQDAjAmMSQwIgYDVQQDDBtLZW52b3kg
b2ZmbGluZSB0ZXN0IGZpeHR1cmUwIBcNMjAwMTAxMDAwMDAwWhgPMjEyMDAxMDEw
MDAwMDBaMCYxJDAiBgNVBAMMG0tlbnZveSBvZmZsaW5lIHRlc3QgZml4dHVyZTBZ
MBMGByqGSM49AgEGCCqGSM49AwEHA0IABP2xNNfSKZUxcnZi4wpX9cMhQYPYIxp3
oJRwo2hMxscESrrX/ok9gJ0GifpG1w+CGsR0wskxizH0mNqdhHW+I9Ojgaswgagw
gZQGA1UdEQSBjDCBiYIKZ2l0aHViLmNvbYIdb2JqZWN0cy5naXRodWJ1c2VyY29u
dGVudC5jb22CJHJlbGVhc2UtYXNzZXRzLmdpdGh1YnVzZXJjb250ZW50LmNvbYIl
Z2l0aHViLXJlbGVhc2VzLmdpdGh1YnVzZXJjb250ZW50LmNvbYIJbG9jYWxob3N0
hwR/AAABMA8GA1UdEwEB/wQFMAMBAf8wCgYIKoZIzj0EAwIDSAAwRQIgcQw7QNSz
pdSJG0v7NZu0E821d6bucNCKbjjICszarVECIQDV6oisC0EoV8Zu9jU98inLrSHs
OqWTCwv9DzgKrYEtPg==
-----END CERTIFICATE-----`;
const payload = Buffer.from('真实 HTTPS 下载后才安装的二进制内容\n'.repeat(30));
const digest = (bytes) => createHash('sha256').update(bytes).digest('hex');
const oldBinary = Buffer.from('previous binary must survive');

function cleanProxyEnvironment(t) {
  const names = ['https_proxy', 'http_proxy', 'all_proxy', 'no_proxy', 'npm_config_proxy',
    'npm_config_https_proxy', 'npm_config_http_proxy', 'npm_config_no_proxy', 'npm_config_noproxy'];
  const saved = new Map();
  for (const name of names.flatMap((item) => [item, item.toUpperCase()])) {
    saved.set(name, process.env[name]);
  }
  for (const name of saved.keys()) delete process.env[name];
  t.after(() => {
    for (const [name, value] of saved) {
      if (value === undefined) delete process.env[name];
      else process.env[name] = value;
    }
  });
}

function packageFixture(t, archive) {
  cleanProxyEnvironment(t);
  const root = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'kenvoy-download-')));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const bytes = archive ?? Buffer.from(zipSync({ 'bin/kenvoy': payload, 'bin/kenvoy.exe': payload }));
  const manifest = { formatVersion: 1, version: '9.8.7', repository: 'zerobiubiu/kenvoy', targets: {} };
  for (const platform of ['win32', 'linux', 'darwin']) {
    for (const arch of ['x64', 'arm64']) {
      const executable = platform === 'win32' ? 'kenvoy.exe' : 'kenvoy';
      manifest.targets[`${platform}-${arch}`] = {
        url: `https://github.com/zerobiubiu/kenvoy/releases/download/v9.8.7/kenvoy-${platform}-${arch}.zip`,
        archiveSha256: digest(bytes), archiveSize: bytes.length,
        member: `bin/${executable}`, executable,
        binarySha256: digest(payload), binarySize: payload.length,
      };
    }
  }
  const save = () => fs.writeFileSync(path.join(root, 'download-manifest.json'), JSON.stringify(manifest));
  save();
  fs.writeFileSync(path.join(root, 'package.json'), JSON.stringify({ name: 'kenvoy', version: '9.8.7' }));
  return {
    root, bytes, manifest, save,
    filename: path.join(root, 'vendor', digest(payload), 'kenvoy'),
    oldFilename: path.join(root, 'vendor', digest(oldBinary), 'kenvoy'),
  };
}

async function listen(t, server) {
  const sockets = new Set();
  server.on('connection', (socket) => {
    sockets.add(socket);
    socket.once('close', () => sockets.delete(socket));
    socket.on('error', () => {});
  });
  await new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolve);
  });
  t.after(() => new Promise((resolve) => {
    for (const socket of sockets) socket.destroy();
    server.close(resolve);
  }));
  return server;
}

async function origin(t, handler) {
  return listen(t, https.createServer({ key: KEY, cert: CERT }, handler));
}

function localRequest(server, trusted = true) {
  return (url, options, callback) => https.get(url, {
    ...options,
    port: server.address().port,
    ca: trusted ? CERT : undefined,
    lookup(hostname, config, done) {
      if (config.all) done(null, [{ address: '127.0.0.1', family: 4 }]);
      else done(null, '127.0.0.1', 4);
    },
  }, callback);
}

function options(server) {
  return { platform: 'linux', arch: 'arm64', request: localRequest(server), timeoutMs: 2_000 };
}

function preserveOld(fixture) {
  fs.mkdirSync(path.dirname(fixture.oldFilename), { recursive: true });
  fs.writeFileSync(fixture.oldFilename, oldBinary);
}

function assertUnchanged(fixture, existing) {
  if (existing) {
    assert.deepEqual(fs.readFileSync(fixture.oldFilename), oldBinary);
    assert.deepEqual(fs.readdirSync(path.dirname(fixture.oldFilename)), ['kenvoy']);
    assert.deepEqual(fs.readdirSync(path.join(fixture.root, 'vendor')), [digest(oldBinary)]);
    const expectedHash = fixture.manifest.targets['linux-arm64'].binarySha256;
    assert.equal(fs.existsSync(path.join(fixture.root, 'vendor', expectedHash, 'kenvoy')), false);
  } else {
    assert.equal(fs.existsSync(path.join(fixture.root, 'vendor')), false);
  }
}

test('真实 HTTPS 重定向只下载选中的目标并安装经过校验的文件', async (t) => {
  const fixture = packageFixture(t);
  const requests = [];
  const server = await origin(t, (req, res) => {
    requests.push({ host: req.headers.host.split(':')[0], path: req.url });
    assert.equal(req.headers.authorization, undefined);
    assert.equal(req.headers['proxy-authorization'], undefined);
    if (req.url.endsWith('.zip')) {
      res.writeHead(302, { location: 'https://release-assets.githubusercontent.com/payload?signed=fixture' });
      res.end();
    } else {
      res.writeHead(200, { 'content-length': fixture.bytes.length });
      res.end(fixture.bytes);
    }
  });
  const installed = await install(fixture.root, options(server));
  assert.equal(installed, fixture.filename);
  assert.deepEqual(fs.readFileSync(installed), payload);
  assert.deepEqual(requests, [
    { host: 'github.com', path: new URL(fixture.manifest.targets['linux-arm64'].url).pathname },
    { host: 'release-assets.githubusercontent.com', path: '/payload?signed=fixture' },
  ]);
  assert.deepEqual(fs.readdirSync(path.dirname(installed)), ['kenvoy']);
  assert.deepEqual(fs.readdirSync(path.join(fixture.root, 'vendor')), [digest(payload)]);
  if (process.platform !== 'win32') assert.equal(fs.statSync(installed).mode & 0o777, 0o755);
  const metadata = fs.statSync(installed);
  assert.equal(await install(fixture.root, {
    platform: 'linux', arch: 'arm64', request() { throw new Error('正确文件不应再次联网'); },
  }), installed);
  assert.equal(fs.statSync(installed).mtimeMs, metadata.mtimeMs);
});

const networkFailures = [
  ['服务端失败', (req, res) => { res.writeHead(503); res.end('private upstream detail'); }, /HTTP 503/],
  ['错误响应长度', (req, res, bytes) => { res.writeHead(200, { 'content-length': bytes.length + 1 }); res.end(bytes); }, /响应长度/],
  ['截短响应', (req, res, bytes) => { res.writeHead(200); res.write(bytes.subarray(0, 5)); res.end(); }, /下载长度/],
  ['超长响应', (req, res, bytes) => { res.writeHead(200); res.write(bytes); res.end('extra'); }, /超过/],
  ['中断连接', (req, res, bytes) => { res.writeHead(200); res.write(bytes.subarray(0, 5)); setImmediate(() => res.destroy()); }, /中断|失败/],
  ['超时', () => {}, /超时/],
  ['降级重定向', (req, res) => { res.writeHead(302, { location: 'http://github.com/asset' }); res.end(); }, /HTTPS/],
  ['外部重定向', (req, res) => { res.writeHead(307, { location: 'https://example.com/asset' }); res.end(); }, /官方/],
  ['认证重定向', (req, res) => { res.writeHead(301, { location: 'https://user:secret@github.com/asset' }); res.end(); }, /认证信息/],
  ['缺失重定向地址', (req, res) => { res.writeHead(302); res.end(); }, /Location/],
  ['空重定向地址', (req, res) => { res.writeHead(302, { location: '' }); res.end(); }, /Location/],
  ['循环重定向', (req, res) => { res.writeHead(308, { location: req.url }); res.end(); }, /次数超过/],
];
for (const [label, handler, expected] of networkFailures) {
  for (const existing of [false, true]) {
    test(`${label}不留下半成品，已有文件=${existing}`, async (t) => {
      const fixture = packageFixture(t);
      if (existing) preserveOld(fixture);
      const server = await origin(t, (req, res) => handler(req, res, fixture.bytes));
      await assert.rejects(install(fixture.root, {
        ...options(server), timeoutMs: label === '超时' ? 100 : 2_000, idleTimeoutMs: 1_000,
      }), expected);
      assertUnchanged(fixture, existing);
    });
  }
}

for (const [label, archive, mutate, expected] of [
  ['归档摘要', undefined, (target) => { target.archiveSha256 = '0'.repeat(64); }, /ZIP SHA-256/],
  ['二进制摘要', undefined, (target) => { target.binarySha256 = '0'.repeat(64); }, /二进制 SHA-256/],
  ['二进制大小', undefined, (target) => { target.binarySize += 1; }, /成员长度/],
  ['损坏归档', Buffer.from('broken ZIP'), () => {}, /归档损坏/],
  ['缺少成员', Buffer.from(zipSync({ LICENSE: Buffer.from('MIT') })), () => {}, /缺少/],
  ['非法路径', Buffer.from(zipSync({ 'bin/kenvoy': payload, '../outside': payload })), () => {}, /非法成员路径/],
  ['不唯一成员', Buffer.from(zipSync({ 'bin/kenvoy': payload, 'other/kenvoy': payload })), () => {}, /不唯一/],
]) {
  for (const existing of [false, true]) {
    test(`真实下载后的${label}错误不破坏安装状态，已有文件=${existing}`, async (t) => {
      const fixture = packageFixture(t, archive);
      mutate(fixture.manifest.targets['linux-arm64']);
      fixture.save();
      if (existing) preserveOld(fixture);
      const server = await origin(t, (req, res) => res.end(fixture.bytes));
      await assert.rejects(install(fixture.root, options(server)), expected);
      assertUnchanged(fixture, existing);
    });
  }
}

test('自签名证书未显式信任时不能安装，保持 TLS 校验', async (t) => {
  const fixture = packageFixture(t);
  const server = await origin(t, (req, res) => res.end(fixture.bytes));
  await assert.rejects(install(fixture.root, {
    ...options(server), request: localRequest(server, false),
  }), /SELF_SIGNED|CERT_|UNABLE_TO_VERIFY/);
  assertUnchanged(fixture, false);
});

async function tunnelProxy(t, server, seen, secure = false) {
  const proxy = secure ? https.createServer({ key: KEY, cert: CERT }) : http.createServer();
  proxy.on('connect', (req, client, head) => {
    seen.push({ url: req.url, authorization: req.headers['proxy-authorization'] });
    const upstream = net.connect(server.address().port, '127.0.0.1', () => {
      client.write('HTTP/1.1 200 Connection Established\r\n\r\n');
      if (head.length) upstream.write(head);
      client.pipe(upstream);
      upstream.pipe(client);
    });
    upstream.on('error', () => client.destroy());
    client.once('close', () => upstream.destroy());
  });
  return listen(t, proxy);
}

for (const variable of ['HTTPS_PROXY', 'https_proxy', 'npm_config_https_proxy', 'npm_config_proxy', 'ALL_PROXY']) {
  test(`${variable}通过真实 CONNECT 代理下载且不向源站泄露代理认证`, async (t) => {
    const fixture = packageFixture(t);
    const server = await origin(t, (req, res) => {
      assert.equal(req.headers.authorization, undefined);
      assert.equal(req.headers['proxy-authorization'], undefined);
      res.end(fixture.bytes);
    });
    const seen = [];
    const proxy = await tunnelProxy(t, server, seen);
    process.env[variable] = `http://fixture-user:fixture-password@127.0.0.1:${proxy.address().port}`;
    await install(fixture.root, {
      ...options(server), request: (url, config, cb) => https.get(url, { ...config, ca: CERT }, cb),
    });
    assert.deepEqual(fs.readFileSync(fixture.filename), payload);
    assert.deepEqual(seen, [{
      url: 'github.com:443',
      authorization: `Basic ${Buffer.from('fixture-user:fixture-password').toString('base64')}`,
    }]);
  });
}

for (const [variable, value] of [
  ['NO_PROXY', 'github.com:443'], ['no_proxy', '*'],
  ['npm_config_noproxy', 'github.com'], ['npm_config_no_proxy', '*.com'],
]) {
  test(`${variable}匹配时绕过不可用代理`, async (t) => {
    const fixture = packageFixture(t);
    const server = await origin(t, (req, res) => res.end(fixture.bytes));
    process.env.HTTPS_PROXY = 'http://127.0.0.1:9';
    process.env[variable] = value;
    await install(fixture.root, options(server));
    assert.deepEqual(fs.readFileSync(fixture.filename), payload);
  });
}

test('每次官方重定向重新选择代理与 NO_PROXY', async (t) => {
  const fixture = packageFixture(t);
  const server = await origin(t, (req, res) => {
    if (req.url.endsWith('.zip')) {
      res.writeHead(302, { location: 'https://objects.githubusercontent.com/asset' });
      res.end();
    } else res.end(fixture.bytes);
  });
  const seen = [];
  const proxy = await tunnelProxy(t, server, seen);
  process.env.HTTPS_PROXY = `http://127.0.0.1:${proxy.address().port}`;
  process.env.NO_PROXY = 'github.com';
  const direct = localRequest(server);
  await install(fixture.root, {
    ...options(server),
    request(url, config, cb) {
      return config.agent === false ? direct(url, config, cb) : https.get(url, { ...config, ca: CERT }, cb);
    },
  });
  assert.deepEqual(seen, [{ url: 'objects.githubusercontent.com:443', authorization: undefined }]);
  assert.deepEqual(fs.readFileSync(fixture.filename), payload);
});

test('代理错误不会在诊断中暴露用户名或密码', async (t) => {
  const fixture = packageFixture(t);
  process.env.HTTPS_PROXY = 'socks://fixture-user:fixture-password@127.0.0.1:9';
  await assert.rejects(install(fixture.root, { platform: 'linux', arch: 'arm64' }), (error) => {
    assert.match(error.message, /代理地址无效/);
    assert.doesNotMatch(error.message, /fixture-user|fixture-password/);
    return true;
  });
  assertUnchanged(fixture, false);
});

test('损坏但长度正确的已有文件只能在完整验证新文件后替换', async (t) => {
  const fixture = packageFixture(t);
  fs.mkdirSync(path.dirname(fixture.filename), { recursive: true });
  fs.writeFileSync(fixture.filename, Buffer.alloc(payload.length));
  const server = await origin(t, (req, res) => res.end(fixture.bytes));
  await install(fixture.root, options(server));
  assert.deepEqual(fs.readFileSync(fixture.filename), payload);
  assert.deepEqual(fs.readdirSync(path.dirname(fixture.filename)), ['kenvoy']);
  assert.deepEqual(fs.readdirSync(path.join(fixture.root, 'vendor')), [digest(payload)]);
});

test('CONNECT 一直无响应时总超时会关闭代理套接字', async (t) => {
  const fixture = packageFixture(t);
  let closed;
  const disconnected = new Promise((resolve) => { closed = resolve; });
  const proxy = await listen(t, net.createServer((socket) => {
    socket.on('data', () => {});
    socket.once('close', closed);
  }));
  process.env.HTTPS_PROXY = `http://127.0.0.1:${proxy.address().port}`;
  await assert.rejects(install(fixture.root, {
    platform: 'linux', arch: 'arm64', timeoutMs: 250,
  }), /超时/);
  let timer;
  try {
    await Promise.race([
      disconnected,
      new Promise((resolve, reject) => {
        timer = setTimeout(() => reject(new Error('超时后代理连接仍未关闭')), 2_000);
      }),
    ]);
  } finally {
    clearTimeout(timer);
  }
  assertUnchanged(fixture, false);
});

test('生成目录的真实 postinstall 经 HTTPS 代理安装，调试日志也不含代理凭据', async (t) => {
  const fixture = packageFixture(t);
  const server = await origin(t, (req, res) => {
    assert.equal(req.headers.authorization, undefined);
    assert.equal(req.headers['proxy-authorization'], undefined);
    res.end(fixture.bytes);
  });
  const seen = [];
  const proxy = await tunnelProxy(t, server, seen, true);
  fs.copyFileSync(path.join(runtime, 'install.cjs'), path.join(fixture.root, 'install.cjs'));
  fs.cpSync(path.join(runtime, 'lib'), path.join(fixture.root, 'lib'), { recursive: true });
  const ca = path.join(fixture.root, 'test-ca.pem');
  fs.writeFileSync(ca, CERT);
  const result = await new Promise((resolve, reject) => {
    execFile(process.execPath, [path.join(fixture.root, 'install.cjs')], {
      timeout: 5_000,
      env: {
        ...process.env, NODE_OPTIONS: '', NODE_PATH: path.join(runtime, 'node_modules'),
        NODE_EXTRA_CA_CERTS: ca, DEBUG: 'https-proxy-agent',
        HTTPS_PROXY: `https://fixture-user:fixture-password@127.0.0.1:${proxy.address().port}`,
      },
    }, (error, stdout, stderr) => {
      if (error) reject(new Error(`真实 postinstall 失败：${stderr}`));
      else resolve({ stdout, stderr });
    });
  });
  const binary = path.join(fixture.root, 'vendor', digest(payload), process.platform === 'win32' ? 'kenvoy.exe' : 'kenvoy');
  assert.deepEqual(fs.readFileSync(binary), payload);
  assert.doesNotMatch(result.stderr, /fixture-user|fixture-password/);
  assert.equal(result.stderr.includes(Buffer.from('fixture-user:fixture-password').toString('base64')), false);
  assert.deepEqual(seen, [{
    url: 'github.com:443',
    authorization: `Basic ${Buffer.from('fixture-user:fixture-password').toString('base64')}`,
  }]);
});

test('新摘要安装成功后保留旧缓存但只返回新摘要路径', async (t) => {
  const fixture = packageFixture(t);
  preserveOld(fixture);
  const server = await origin(t, (req, res) => res.end(fixture.bytes));
  const installed = await install(fixture.root, options(server));
  assert.equal(installed, fixture.filename);
  assert.notEqual(installed, fixture.oldFilename);
  assert.deepEqual(fs.readFileSync(installed), payload);
  assert.deepEqual(fs.readFileSync(fixture.oldFilename), oldBinary);
  assert.deepEqual(fs.readdirSync(path.join(fixture.root, 'vendor')).sort(),
    [digest(oldBinary), digest(payload)].sort());
});
