// 只拥有通过 mkdtemp 创建的项目和由本模块启动的进程。
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdtemp, readFile, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve, join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
const repository = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const binary = process.env.KENVOY_BINARY || join(repository, 'target/debug', process.platform === 'win32' ? 'kenvoy.exe' : 'kenvoy');

export async function projectFixture() {
  const root = await mkdtemp(join(tmpdir(), 'kenvoy-browser-'));
  const children = new Set();
  async function command(args, input = '', timeout = 45000) {
    const child = spawn(binary, args, { cwd: root, windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] });
    children.add(child); let stdout = '', stderr = '';
    child.stdout.setEncoding('utf8').on('data', chunk => { stdout += chunk; });
    child.stderr.setEncoding('utf8').on('data', chunk => { stderr += chunk; });
    child.stdin.end(input);
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => { child.kill(); reject(new Error(`CLI 超过 ${timeout / 1000} 秒`)); }, timeout);
      child.once('error', error => { clearTimeout(timer); reject(error); });
      child.once('exit', code => { clearTimeout(timer); children.delete(child); if (code === 0) resolve(stdout); else reject(new Error(`CLI ${args[0]} 退出 ${code}：${stderr}`)); });
    });
  }
  async function stop(child) {
    if (child.exitCode !== null) return;
    const ended = new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error('测试进程未退出')), 10000);
      child.once('exit', () => { clearTimeout(timer); children.delete(child); resolve(); });
    });
    child.kill('SIGTERM'); await ended;
  }
  async function start(mode = 'dev', options = [], timeout = 45000, env = {}) {
    const child = spawn(binary, ['--project', 'app', mode, ...options], { cwd: root, env: { ...process.env, ...env }, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'] });
    children.add(child); let output = '', errors = '';
    const origin = await new Promise((resolve, reject) => {
      const timer = setTimeout(() => { child.kill(); reject(new Error(`服务启动超时：${errors}`)); }, timeout);
      child.stdout.setEncoding('utf8').on('data', chunk => { output += chunk; const match = output.match(/http:\/\/127\.0\.0\.1:\d+/); if (match) { clearTimeout(timer); resolve(match[0]); } });
      child.stderr.setEncoding('utf8').on('data', chunk => { errors += chunk; });
      child.once('error', error => { clearTimeout(timer); reject(error); });
      child.once('exit', code => { clearTimeout(timer); children.delete(child); reject(new Error(`服务提前退出 ${code}：${errors}`)); });
    });
    return { child, origin, stop: () => stop(child), logs: () => errors };
  }
  await command(['create', 'app']);
  const config = join(root, 'app/kenvoy.toml');
  await writeFile(config, (await readFile(config, 'utf8')).replace('port = 3000', 'port = 0'));
  return { root, project: join(root, 'app'), command, start, async close() {
    const results = await Promise.allSettled([...children].map(stop));
    const failures = results.filter(result => result.status === 'rejected');
    if (failures.length) throw new AggregateError(failures.map(result => result.reason), '夹具进程未全部退出，保留临时目录');
    assert.equal(dirname(root), tmpdir());
    assert.ok(root.startsWith(join(tmpdir(), 'kenvoy-browser-')));
    // Windows 强制终止父进程后，开发子进程会在管道 EOF 后关闭；等待它释放文件句柄。
    await rm(root, { recursive: true, maxRetries: 10, retryDelay: 100 });
  } };
}
