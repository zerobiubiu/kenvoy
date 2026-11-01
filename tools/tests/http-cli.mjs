// 真实 CLI 子进程与本机 HTTP；仅操作本脚本独有的临时项目。
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdtemp, readFile, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve, join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const repository = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const binary = process.env.KENVOY_BINARY || join(repository, 'target/debug', process.platform === 'win32' ? 'kenvoy.exe' : 'kenvoy');
const root = await mkdtemp(join(tmpdir(), 'kenvoy-http-cli-'));
const children = new Set();
const password = 'isolated-http-test-password-472';
let checks = 0;

async function command(args, input = '') {
  const child = spawn(binary, args, { cwd: root, windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] });
  children.add(child);
  let stdout = '', stderr = '';
  child.stdout.setEncoding('utf8').on('data', data => { stdout += data; });
  child.stderr.setEncoding('utf8').on('data', data => { stderr += data; });
  child.stdin.end(input);
  return await new Promise((resolve, reject) => {
    const timer = setTimeout(() => { child.kill(); reject(new Error('CLI 命令超过 30 秒')); }, 30000);
    child.once('error', error => { clearTimeout(timer); reject(error); });
    child.once('exit', code => {
      clearTimeout(timer); children.delete(child);
      if (code === 0) resolve(stdout);
      else reject(new Error(`CLI 退出 ${code}：${stderr}`));
    });
  });
}

async function start() {
  const child = spawn(binary, ['--project', 'app', 'dev'], { cwd: root, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'] });
  children.add(child);
  let stdout = '', stderr = '';
  const origin = await new Promise((resolve, reject) => {
    const timer = setTimeout(() => { child.kill(); reject(new Error(`服务启动超过 30 秒：${stderr}`)); }, 30000);
    child.stdout.setEncoding('utf8').on('data', data => {
      stdout += data;
      const matched = stdout.match(/http:\/\/127\.0\.0\.1:\d+/);
      if (matched) { clearTimeout(timer); resolve(matched[0]); }
    });
    child.stderr.setEncoding('utf8').on('data', data => { stderr += data; });
    child.once('error', error => { clearTimeout(timer); reject(error); });
    child.once('exit', code => { clearTimeout(timer); children.delete(child); reject(new Error(`服务提前退出 ${code}：${stderr}`)); });
  });
  return { child, origin };
}

async function stop(child) {
  if (child.exitCode !== null) return;
  const completed = new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('测试子进程未退出')), 10000);
    child.once('exit', () => { clearTimeout(timer); children.delete(child); resolve(); });
  });
  // Windows 下这是进程终止；本测试只证明重启持久化，不据此声称 Ctrl+C 优雅退出。
  child.kill('SIGTERM');
  await completed;
}

let origin, cookie, csrf, generation;
async function request(path, { method = 'GET', data, form, authenticated = true, expected = 200 } = {}) {
  const headers = {};
  if (authenticated && cookie) headers.cookie = cookie;
  if (!['GET', 'HEAD'].includes(method)) {
    headers.origin = origin;
    headers['x-kenvoy-csrf'] = csrf;
    headers['x-kenvoy-generation'] = String(generation);
    if (data !== undefined) headers['content-type'] = 'application/json';
  }
  if (form) headers['x-kenvoy-form'] = form;
  const response = await fetch(origin + path, { method, headers, body: data === undefined ? undefined : JSON.stringify(data), signal: AbortSignal.timeout(10000) });
  const text = await response.text();
  assert.equal(response.status, expected, `${method} ${path}：${text}`);
  checks++;
  const value = text ? JSON.parse(text) : null;
  const set = response.headers.get('set-cookie');
  if (set) {
    assert.ok(set.includes('HttpOnly') && set.includes('SameSite=Lax'));
    cookie = set.split(';')[0];
  }
  if (value?.csrf_token) { csrf = value.csrf_token; generation = value.generation; }
  return value;
}

try {
  await command(['create', 'app']);
  const configPath = join(root, 'app/kenvoy.toml');
  await writeFile(configPath, (await readFile(configPath, 'utf8')).replace('port = 3000', 'port = 0'));
  await command(['--project', 'app', 'admin', 'init', '--username', 'administrator', '--password-stdin'], password + '\n');
  await command(['--project', 'app', 'admin', 'add-user', '--username', 'reader', '--password-stdin'], password + '\n');
  let server = await start(); origin = server.origin;
  await request('/_kenvoy/session');
  const model = await request('/_kenvoy/application');
  assert.ok(!JSON.stringify(model).includes(root));
  const form = model.pages[0].components.find(node => node.type === 'form').id;
  await request('/api/customers', { data: { name: '', age: -1 }, method: 'POST', form, expected: 422 });
  const created = await request('/api/customers', { data: { name: '张三', phone: '13800000000', age: 25 }, method: 'POST', form, expected: 201 });
  const id = created.data.id;
  assert.equal(created.data.name, '张三');
  await request('/api/customers', { expected: 401 });
  await request(`/api/customers/${id}`, { expected: 401 });
  await request(`/api/customers/${id}`, { method: 'PATCH', data: { age: 26 }, expected: 401 });
  await request('/_kenvoy/login', { method: 'POST', data: { username: 'reader', password } });
  await request('/api/customers', { expected: 403 });
  await request('/_kenvoy/logout', { method: 'POST', data: {} });
  await request('/_kenvoy/login', { method: 'POST', data: { username: 'administrator', password } });
  const list = await request('/api/customers?search=%E5%BC%A0&page=1&page_size=20');
  assert.equal(list.total, 1); assert.equal(list.data[0].id, id);
  await request(`/api/customers/${id}`, { method: 'PATCH', data: { age: 26 } });
  await request('/api/customers?page=1&page=2', { expected: 400 });
  await request('/api/_kenvoy_users', { expected: 404 });
  await stop(server.child);
  server = await start(); origin = server.origin;
  const restored = await request(`/api/customers/${id}`);
  assert.equal(restored.data.age, 26);
  await request(`/api/customers/${id}`, { method: 'DELETE', expected: 204 });
  await request(`/api/customers/${id}`, { expected: 404 });
  await request('/_kenvoy/logout', { method: 'POST', data: {} });
  await request('/api/customers', { expected: 401 });
  await stop(server.child);
  console.log(JSON.stringify({ status: '通过', http_assertions: checks, real_cli: true, sqlite_restart_persistence: true, administrator_and_user_roles: true, graceful_shutdown: '本脚本未验证；由 Rust HTTP 测试单独验证' }, null, 2));
} finally {
  await Promise.allSettled([...children].map(stop));
  if (dirname(root) === tmpdir() && root.startsWith(join(tmpdir(), 'kenvoy-http-cli-'))) await rm(root, { recursive: true });
}
