'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { createHash } = require('node:crypto');
const { createRequire } = require('node:module');
const runtime = path.resolve(__dirname, '../npm_runtime');
const runtimeRequire = createRequire(path.join(runtime, 'package.json'));
const { zipSync, Zip, ZipPassThrough } = runtimeRequire('fflate');
const { selectTarget } = require('../npm_runtime/lib/config.cjs');
const { extractBinary } = require('../npm_runtime/lib/archive.cjs');
const { reuseBinary, writeBinary } = require('../npm_runtime/lib/files.cjs');

const combinations = [
  ['win32', 'x64', 'kenvoy.exe', 'x86_64-pc-windows-msvc'],
  ['win32', 'arm64', 'kenvoy.exe', 'aarch64-pc-windows-msvc'],
  ['linux', 'x64', 'kenvoy', 'x86_64-unknown-linux-musl'],
  ['linux', 'arm64', 'kenvoy', 'aarch64-unknown-linux-musl'],
  ['darwin', 'x64', 'kenvoy', 'x86_64-apple-darwin'],
  ['darwin', 'arm64', 'kenvoy', 'aarch64-apple-darwin'],
];
const payload = Buffer.from('经过双重摘要校验的二进制内容\n'.repeat(100));
const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');

function archiveFixture(entries = { 'bundle/kenvoy': payload, LICENSE: Buffer.from('MIT') }) {
  const bytes = Buffer.from(zipSync(entries));
  return {
    bytes,
    target: {
      url: 'https://github.com/zerobiubiu/kenvoy/releases/download/v9.8.7/kenvoy-9.8.7-x86_64-unknown-linux-musl.zip',
      archiveSize: bytes.length,
      archiveSha256: sha256(bytes),
      member: 'bundle/kenvoy',
      executable: 'kenvoy',
      binarySize: payload.length,
      binarySha256: sha256(payload),
    },
  };
}

function packageFixture(t) {
  const root = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'kenvoy-runtime-')));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const { target } = archiveFixture();
  const manifest = { formatVersion: 1, version: '9.8.7', repository: 'zerobiubiu/kenvoy', targets: {} };
  for (const [platform, arch, executable, triple] of combinations) {
    manifest.targets[`${platform}-${arch}`] = {
      ...target,
      member: `bundle/${executable}`,
      executable,
      url: `https://github.com/zerobiubiu/kenvoy/releases/download/v9.8.7/kenvoy-9.8.7-${triple}.zip`,
    };
  }
  fs.writeFileSync(path.join(root, 'package.json'), JSON.stringify({ name: 'kenvoy', version: manifest.version }));
  const save = () => fs.writeFileSync(path.join(root, 'download-manifest.json'), JSON.stringify(manifest));
  save();
  return { root, manifest, save };
}

test('平台映射使用实际 Release 文件名与内容寻址安装路径', (t) => {
  const { root, manifest } = packageFixture(t);
  for (const [platform, arch, executable, triple] of combinations) {
    const selected = selectTarget(root, platform, arch);
    assert.equal(selected.key, `${platform}-${arch}`);
    assert.equal(selected.binaryPath, path.join(root, 'vendor', sha256(payload), executable));
    assert.equal(selected.target.url, manifest.targets[selected.key].url);
    assert.ok(selected.target.url.endsWith(`-${triple}.zip`));
  }
  for (const [platform, arch] of [['freebsd', 'x64'], ['linux', 'ia32'], ['win32', 'riscv64']]) {
    assert.throws(() => selectTarget(root, platform, arch), /不支持当前平台/);
  }
  assert.equal(fs.existsSync(path.join(root, 'vendor')), false);
});

for (const [name, mutate, message] of [
  ['版本不匹配', (m) => { m.version = '9.8.8'; }, /不一致/],
  ['仓库与下载地址不匹配', (m) => { m.repository = 'someone/kenvoy'; }, /初始下载地址/],
  ['缺少目标', (m) => { delete m.targets['darwin-arm64']; }, /完整包含/],
  ['逃逸输出', (m) => { m.targets['linux-x64'].executable = '../kenvoy'; }, /名称无效/],
  ['逃逸成员', (m) => { m.targets['linux-x64'].member = '../kenvoy'; }, /非法成员路径/],
  ['无效摘要', (m) => { m.targets['linux-x64'].binarySha256 = 'invalid'; }, /binarySha256/],
  ['零长度', (m) => { m.targets['linux-x64'].binarySize = 0; }, /binarySize/],
  ['非整数长度', (m) => { m.targets['linux-x64'].archiveSize = 1.5; }, /archiveSize/],
]) {
  test(`下载清单拒绝${name}且不创建输出`, (t) => {
    const fixture = packageFixture(t);
    mutate(fixture.manifest);
    fixture.save();
    assert.throws(() => selectTarget(fixture.root, 'linux', 'x64'), message);
    assert.equal(fs.existsSync(path.join(fixture.root, 'vendor')), false);
  });
}

for (const repository of [
  'Example-Owner/kenvoy_clone.v2',
  `${'A'.repeat(39)}/${'r'.repeat(100)}`,
]) {
  test(`合法非默认仓库使用自身对应版本的 Release：${repository}`, (t) => {
    const fixture = packageFixture(t);
    fixture.manifest.repository = repository;
    for (const target of Object.values(fixture.manifest.targets)) {
      target.url = target.url.replace('/zerobiubiu/kenvoy/', `/${repository}/`);
    }
    fixture.save();
    const selected = selectTarget(fixture.root, 'linux', 'x64');
    assert.equal(selected.manifest.repository, repository);
    assert.ok(selected.target.url.startsWith(`https://github.com/${repository}/releases/download/v9.8.7/`));
    fixture.manifest.targets['linux-x64'].url = fixture.manifest.targets['linux-x64'].url
      .replace(`/${repository}/`, '/different-owner/different-repo/');
    fixture.save();
    assert.throws(() => selectTarget(fixture.root, 'linux', 'x64'), /初始下载地址/);
  });
}

for (const repository of [
  '', 'owner', '/repo', 'owner/repo/extra', '-owner/repo', 'owner_/repo',
  `${'a'.repeat(40)}/repo`, `owner/${'r'.repeat(101)}`, 'owner/repo\n',
  'https://github.com/owner/repo', 'owner/repo?query', null,
]) {
  test(`仓库名称拒绝非法 owner/name：${JSON.stringify(repository)}`, (t) => {
    const fixture = packageFixture(t);
    fixture.manifest.repository = repository;
    fixture.save();
    assert.throws(() => selectTarget(fixture.root, 'linux', 'x64'), /不一致/);
    assert.equal(fs.existsSync(path.join(fixture.root, 'vendor')), false);
  });
}

for (const url of [
  'http://github.com/zerobiubiu/kenvoy/releases/download/v9.8.7/file.zip',
  'https://github.com:8443/zerobiubiu/kenvoy/releases/download/v9.8.7/file.zip',
  'https://example.com/zerobiubiu/kenvoy/releases/download/v9.8.7/file.zip',
  'https://github.com/other/kenvoy/releases/download/v9.8.7/file.zip',
  'https://github.com/zerobiubiu/kenvoy/releases/download/v9.8.6/file.zip',
  'https://secret:token@github.com/zerobiubiu/kenvoy/releases/download/v9.8.7/file.zip',
  'https://github.com/zerobiubiu/kenvoy/releases/download/v9.8.7/folder%2ffile.zip',
]) {
  test(`初始下载地址拒绝非官方当前版本地址 ${new URL(url).hostname}`, (t) => {
    const fixture = packageFixture(t);
    fixture.manifest.targets['linux-x64'].url = url;
    fixture.save();
    assert.throws(() => selectTarget(fixture.root, 'linux', 'x64'), /官方/);
  });
}

test('ZIP 只返回唯一可执行成员并校验真实解压内容', () => {
  const fixture = archiveFixture();
  assert.deepEqual(Buffer.from(extractBinary(fixture.bytes, fixture.target)), payload);
});

for (const [name, mutate, message] of [
  ['下载长度', (f) => { f.target.archiveSize += 1; }, /下载长度/],
  ['归档摘要', (f) => { f.target.archiveSha256 = '0'.repeat(64); }, /ZIP SHA-256/],
  ['二进制长度', (f) => { f.target.binarySize += 1; }, /成员长度/],
  ['二进制摘要', (f) => { f.target.binarySha256 = '0'.repeat(64); }, /二进制 SHA-256/],
]) {
  test(`归档拒绝错误的${name}`, () => {
    const fixture = archiveFixture();
    mutate(fixture);
    assert.throws(() => extractBinary(fixture.bytes, fixture.target), message);
  });
}

for (const member of ['../escape', '/absolute', 'C:/escape', 'dir\\escape', './escape', 'dir//escape', 'bad\0name']) {
  test(`归档拒绝非目标成员中的非法路径 ${JSON.stringify(member)}`, () => {
    const fixture = archiveFixture({ 'bundle/kenvoy': payload, [member]: Buffer.from('bad') });
    assert.throws(() => extractBinary(fixture.bytes, fixture.target), /非法成员路径/);
  });
}

test('归档拒绝缺失成员、多个同名文件与损坏的 ZIP', () => {
  const missing = archiveFixture({ LICENSE: Buffer.from('MIT') });
  assert.throws(() => extractBinary(missing.bytes, missing.target), /缺少/);
  const ambiguous = archiveFixture({ 'bundle/kenvoy': payload, 'another/kenvoy': payload });
  assert.throws(() => extractBinary(ambiguous.bytes, ambiguous.target), /不唯一/);
  const broken = archiveFixture();
  broken.bytes = Buffer.from('not a ZIP');
  broken.target.archiveSize = broken.bytes.length;
  broken.target.archiveSha256 = sha256(broken.bytes);
  assert.throws(() => extractBinary(broken.bytes, broken.target), /归档损坏/);
});

test('归档拒绝中央目录中的完全重复成员而不是让解压器覆盖它', () => {
  const chunks = [];
  const zip = new Zip((error, chunk) => {
    if (error) throw error;
    chunks.push(Buffer.from(chunk));
  });
  for (const bytes of [payload, Buffer.from('second')]) {
    const member = new ZipPassThrough('bundle/kenvoy');
    zip.add(member);
    member.push(bytes, true);
  }
  zip.end();
  const fixture = archiveFixture();
  fixture.bytes = Buffer.concat(chunks);
  fixture.target.archiveSize = fixture.bytes.length;
  fixture.target.archiveSha256 = sha256(fixture.bytes);
  assert.throws(() => extractBinary(fixture.bytes, fixture.target), /重复成员/);
});

test('内容寻址缓存可复用，不同摘要不会复用或删除旧文件', (t) => {
  const { root } = packageFixture(t);
  const { target } = archiveFixture();
  const filename = writeBinary(root, target, payload, true);
  assert.deepEqual(fs.readFileSync(filename), payload);
  assert.equal(reuseBinary(root, target, true), true);
  const wrong = { ...target, binarySha256: '0'.repeat(64) };
  assert.equal(reuseBinary(root, wrong, true), false);
  assert.deepEqual(fs.readFileSync(filename), payload);
  if (process.platform !== 'win32') {
    fs.chmodSync(filename, 0o600);
    assert.equal(reuseBinary(root, target, true), true);
    assert.equal(fs.statSync(filename).mode & 0o777, 0o755);
  }
});

for (const failure of ['write', 'rename']) {
  test(`原子${failure}失败保留旧文件且清理临时文件`, (t) => {
    const { root } = packageFixture(t);
    const { target } = archiveFixture();
    const old = Buffer.from('previous executable');
    const filename = writeBinary(root, target, old, false);
    const error = Object.assign(new Error('模拟磁盘写入故障'), { code: 'EACCES' });
    if (failure === 'write') {
      t.mock.method(fs, 'writeFileSync', (fd) => {
        fs.writeSync(fd, Buffer.from('partial'));
        throw error;
      });
    } else {
      t.mock.method(fs, 'renameSync', () => { throw error; });
    }
    assert.throws(() => writeBinary(root, target, payload, false), /模拟磁盘写入故障/);
    assert.deepEqual(fs.readFileSync(filename), old);
    assert.deepEqual(fs.readdirSync(path.dirname(filename)), ['kenvoy']);
    assert.deepEqual(fs.readdirSync(path.join(root, 'vendor')), [target.binarySha256]);
  });
}

for (const level of ['vendor', 'digest']) {
  test(`${level} 目录链接不能向包外写入或复用外部文件`, (t) => {
    const { root } = packageFixture(t);
    const { target } = archiveFixture();
    const outside = fs.mkdtempSync(path.join(os.tmpdir(), 'kenvoy-outside-'));
    t.after(() => fs.rmSync(outside, { recursive: true, force: true }));
    const vendor = path.join(root, 'vendor');
    if (level === 'digest') fs.mkdirSync(vendor);
    const link = level === 'vendor' ? vendor : path.join(vendor, target.binarySha256);
    fs.symlinkSync(outside, link, process.platform === 'win32' ? 'junction' : 'dir');
    assert.throws(() => writeBinary(root, target, payload, true), /真实目录/);
    assert.throws(() => reuseBinary(root, target, true), /真实目录/);
    assert.deepEqual(fs.readdirSync(outside), []);
  });
}

test('已有可执行文件链接不能改写其指向的文件', { skip: process.platform === 'win32' }, (t) => {
  const { root } = packageFixture(t);
  const outside = path.join(root, 'outside');
  fs.writeFileSync(outside, 'do not replace');
  const { target } = archiveFixture();
  const directory = path.join(root, 'vendor', target.binarySha256);
  fs.mkdirSync(directory, { recursive: true });
  fs.symlinkSync(outside, path.join(directory, 'kenvoy'));
  assert.throws(() => writeBinary(root, target, payload, true), /普通文件/);
  assert.equal(fs.readFileSync(outside, 'utf8'), 'do not replace');
});
