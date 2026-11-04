'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawn, spawnSync } = require('node:child_process');
const { createHash } = require('node:crypto');
const runtime = path.resolve(__dirname, '../npm_runtime');

let executableIdentity;
function nodeIdentity() {
  if (executableIdentity) return executableIdentity;
  const fd = fs.openSync(process.execPath, 'r');
  try {
    const hash = createHash('sha256');
    const buffer = Buffer.allocUnsafe(64 * 1024);
    let length;
    while ((length = fs.readSync(fd, buffer, 0, buffer.length, null)) !== 0) {
      hash.update(buffer.subarray(0, length));
    }
    executableIdentity = { sha256: hash.digest('hex'), size: fs.fstatSync(fd).size };
    return executableIdentity;
  } finally {
    fs.closeSync(fd);
  }
}

function placeNode(binary) {
  fs.mkdirSync(path.dirname(binary), { recursive: true });
  try {
    fs.linkSync(process.execPath, binary);
  } catch (error) {
    if (!['EXDEV', 'EPERM', 'EACCES', 'ENOTSUP'].includes(error.code)) throw error;
    fs.copyFileSync(process.execPath, binary);
  }
}

function runnerFixture(t, withBinary = true) {
  const root = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'kenvoy-runner-')));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  fs.mkdirSync(path.join(root, 'bin'));
  const runner = path.join(root, 'bin', 'kenvoy.cjs');
  fs.copyFileSync(path.join(runtime, 'run.cjs'), runner);
  fs.cpSync(path.join(runtime, 'lib'), path.join(root, 'lib'), { recursive: true });
  const manifest = { formatVersion: 1, version: '9.8.7', repository: 'zerobiubiu/kenvoy', targets: {} };
  const identity = nodeIdentity();
  for (const platform of ['win32', 'linux', 'darwin']) {
    for (const arch of ['x64', 'arm64']) {
      const executable = platform === 'win32' ? 'kenvoy.exe' : 'kenvoy';
      manifest.targets[`${platform}-${arch}`] = {
        url: `https://github.com/zerobiubiu/kenvoy/releases/download/v9.8.7/kenvoy-${platform}-${arch}.zip`,
        archiveSha256: 'a'.repeat(64), archiveSize: 1,
        binarySha256: identity.sha256, binarySize: identity.size, member: executable, executable,
      };
    }
  }
  const save = () => {
    fs.writeFileSync(path.join(root, 'package.json'), JSON.stringify({ name: 'kenvoy', version: manifest.version }));
    fs.writeFileSync(path.join(root, 'download-manifest.json'), JSON.stringify(manifest));
  };
  save();
  const binary = path.join(root, 'vendor', identity.sha256, process.platform === 'win32' ? 'kenvoy.exe' : 'kenvoy');
  if (withBinary) placeNode(binary);
  const environment = { ...process.env, NODE_PATH: '', NODE_OPTIONS: '' };
  return { root, runner, binary, environment, manifest, save };
}

test('正确摘要缓存中的 runner 真实透传参数、stdio 与退出码', (t) => {
  const fixture = runnerFixture(t);
  const args = ['参数含空格', '"quotes"; $(not-a-command)', '--literal'];
  const script = String.raw`
    const fs = require('node:fs');
    process.stdout.write(JSON.stringify({ args: process.argv.slice(1), input: fs.readFileSync(0, 'utf8') }));
    process.stderr.write('真实子进程 stderr\n');
    process.exitCode = 23;
  `;
  const result = spawnSync(process.execPath, [fixture.runner, '-e', script, '--', ...args], {
    env: fixture.environment, input: '输入内容\n', encoding: 'utf8', timeout: 5_000,
  });
  assert.equal(result.error, undefined);
  assert.equal(result.status, 23);
  assert.equal(result.signal, null);
  assert.deepEqual(JSON.parse(result.stdout), { args, input: '输入内容\n' });
  assert.equal(result.stderr, '真实子进程 stderr\n');
});

test('跳过安装脚本后 runner 离线失败并分别说明局部与全局 rebuild 命令', (t) => {
  const fixture = runnerFixture(t, false);
  const result = spawnSync(process.execPath, [fixture.runner, '--version'], {
    env: { ...fixture.environment, HTTPS_PROXY: 'http://127.0.0.1:9' }, encoding: 'utf8', timeout: 5_000,
  });
  assert.equal(result.error, undefined);
  assert.notEqual(result.status, 0);
  assert.equal(result.stdout, '');
  assert.match(result.stderr, /npm rebuild kenvoy/);
  assert.match(result.stderr, /npm rebuild -g kenvoy/);
  assert.equal(fs.existsSync(path.join(fixture.root, 'vendor')), false);
});

test('runner 不把输出位置的目录当作已安装的二进制', (t) => {
  const fixture = runnerFixture(t, false);
  fs.mkdirSync(fixture.binary, { recursive: true });
  const result = spawnSync(process.execPath, [fixture.runner], {
    env: fixture.environment, encoding: 'utf8', timeout: 5_000,
  });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /普通文件/);
  assert.equal(fs.statSync(fixture.binary).isDirectory(), true);
});

test('无法执行时真实返回失败并提示恢复命令', { skip: process.platform === 'win32' }, (t) => {
  const fixture = runnerFixture(t, false);
  fs.mkdirSync(path.dirname(fixture.binary), { recursive: true });
  fs.writeFileSync(fixture.binary, '#!/bin/sh\nexit 0\n', { mode: 0o600 });
  const result = spawnSync(process.execPath, [fixture.runner], {
    env: fixture.environment, encoding: 'utf8', timeout: 5_000,
  });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /EACCES/);
  assert.match(result.stderr, /npm rebuild -g kenvoy/);
});

test('子进程信号终止真实透传给 runner 调用者', { skip: process.platform === 'win32' }, (t) => {
  const fixture = runnerFixture(t);
  const result = spawnSync(process.execPath, [fixture.runner, '-e', 'process.kill(process.pid, "SIGTERM")'], {
    env: fixture.environment, encoding: 'utf8', timeout: 5_000,
  });
  assert.equal(result.error, undefined);
  assert.equal(result.status, null);
  assert.equal(result.signal, 'SIGTERM');
});

test('发给 runner 的信号会抵达真实子进程', { skip: process.platform === 'win32', timeout: 7_000 }, async (t) => {
  const fixture = runnerFixture(t);
  const script = String.raw`
    process.on('SIGTERM', () => { process.stderr.write('received SIGTERM\n'); process.exit(37); });
    process.stdout.write('ready\n');
    setInterval(() => {}, 1000);
  `;
  const child = spawn(process.execPath, [fixture.runner, '-e', script], {
    env: fixture.environment, stdio: ['ignore', 'pipe', 'pipe'],
  });
  t.after(() => { if (child.exitCode === null && child.signalCode === null) child.kill('SIGKILL'); });
  let stderr = '';
  child.stderr.on('data', (chunk) => { stderr += chunk; });
  const exit = new Promise((resolve, reject) => {
    child.once('error', reject);
    child.once('close', (code, signal) => resolve({ code, signal }));
  });
  await new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('真实子进程未进入可接收信号状态')), 4_000);
    let stdout = '';
    child.stdout.on('data', (chunk) => {
      stdout += chunk;
      if (stdout.includes('ready\n')) { clearTimeout(timer); resolve(); }
    });
    child.once('error', (error) => { clearTimeout(timer); reject(error); });
  });
  child.kill('SIGTERM');
  assert.deepEqual(await exit, { code: 37, signal: null });
  assert.equal(stderr, 'received SIGTERM\n');
});

test('更新清单后 runner 拒绝仍在旧摘要目录中的真实程序', (t) => {
  const fixture = runnerFixture(t);
  const nextHash = createHash('sha256').update('next release binary').digest('hex');
  fixture.manifest.version = '9.8.8';
  for (const target of Object.values(fixture.manifest.targets)) {
    target.binarySha256 = nextHash;
    target.url = target.url.replace('/v9.8.7/', '/v9.8.8/');
  }
  fixture.save();
  const result = spawnSync(process.execPath, [fixture.runner, '-e', 'process.stdout.write("stale program executed")'], {
    env: fixture.environment, encoding: 'utf8', timeout: 5_000,
  });
  assert.equal(result.error, undefined);
  assert.notEqual(result.status, 0);
  assert.equal(result.stdout, '');
  assert.match(result.stderr, /npm rebuild kenvoy/);
  assert.equal(fs.statSync(fixture.binary).size, nodeIdentity().size);
  assert.equal(fs.existsSync(path.join(fixture.root, 'vendor', nextHash)), false);
});

test('runner 不回退到 legacy vendor 根目录中的真实程序', (t) => {
  const fixture = runnerFixture(t, false);
  const legacy = path.join(fixture.root, 'vendor', path.basename(fixture.binary));
  placeNode(legacy);
  const result = spawnSync(process.execPath, [fixture.runner, '-e', 'process.stdout.write("legacy program executed")'], {
    env: fixture.environment, encoding: 'utf8', timeout: 5_000,
  });
  assert.equal(result.error, undefined);
  assert.notEqual(result.status, 0);
  assert.equal(result.stdout, '');
  assert.match(result.stderr, /npm rebuild -g kenvoy/);
  assert.equal(fs.statSync(legacy).size, nodeIdentity().size);
  assert.equal(fs.existsSync(fixture.binary), false);
});

test('Windows 控制台 Ctrl+C 允许真实子进程完成清理而不被强杀', {
  skip: process.platform !== 'win32', timeout: 20_000,
}, (t) => {
  const fixture = runnerFixture(t);
  const childScript = path.join(fixture.root, 'console-child.cjs');
  fs.writeFileSync(childScript, `
    process.on('SIGINT', () => {
      process.stdout.write('child-sigint\\n');
      setTimeout(() => { process.stdout.write('graceful-complete\\n'); process.exit(37); }, 300);
    });
    process.stdout.write('ready\\n');
    setInterval(() => {}, 1000);
  `);
  const result = spawnSync(process.env.KENVOY_TEST_PYTHON || 'python', [
    '-B', path.join(__dirname, 'npm_runtime_console_helper.py'),
    process.execPath, fixture.runner, childScript,
  ], { env: fixture.environment, encoding: 'utf8', timeout: 15_000 });
  assert.equal(result.error, undefined);
  assert.equal(result.status, 0, result.stderr);
  const observed = JSON.parse(result.stdout);
  assert.equal(observed.code, 37);
  assert.equal(observed.stdout, 'child-sigint\ngraceful-complete\n');
  assert.equal(observed.stderr, '');
});
