// Playwright 仅发送夹具请求；临时项目、CLI 和服务进程由 Rust 测试拥有。
import { createInterface } from 'node:readline';
import { writeFile } from 'node:fs/promises';
import { join } from 'node:path';

const prefix = 'KENVOY_FIXTURE_RPC:';

export async function projectFixture() {
  if (process.env.KENVOY_TEST_RPC !== '1') {
    throw new Error('请通过 cargo test -p kenvoy-tests --test browser -- --ignored 运行浏览器验收');
  }
  const reader = createInterface({ input: process.stdin });
  const pending = new Map();
  let nextId = 1;
  let closed = false;
  reader.on('line', line => {
    try {
      const response = JSON.parse(line);
      const request = pending.get(response.id);
      if (!request) throw new Error('Rust 夹具返回了未知请求编号');
      pending.delete(response.id);
      if (response.error) request.reject(new Error(response.error));
      else request.resolve(response.result);
    } catch (error) {
      for (const request of pending.values()) request.reject(error);
      pending.clear();
      reader.close();
    }
  });
  reader.on('close', () => {
    closed = true;
    for (const request of pending.values()) request.reject(new Error('Rust 夹具通信提前关闭'));
    pending.clear();
  });
  function request(method, parameters = {}) {
    if (closed) return Promise.reject(new Error('Rust 夹具已关闭'));
    const id = nextId++;
    return new Promise((resolve, reject) => {
      pending.set(id, { resolve, reject });
      process.stdout.write(prefix + JSON.stringify({ id, method, ...parameters }) + '\n', error => {
        if (error) { pending.delete(id); reject(error); }
      });
    });
  }
  try {
    const paths = await request('fixture');
    return {
      ...paths,
      command: (args, input = '', timeout = 45000) => request('command', { args, input, timeout }),
      async sealMigration(description, rules) {
        const invoke = async args => JSON.parse(await request('command', { args: ['--project', 'app', 'migrate', ...args], input: '', timeout: 45000 }));
        const args = ['generate', '--description', description];
        if (rules) {
          const path = join(paths.project, 'migration-rules.json');
          await writeFile(path, JSON.stringify(rules));
          args.push('--rules', path);
        }
        const draft = await invoke(args);
        await invoke(['seal', '--confirm', draft.confirmation_checksum]);
        return invoke(['status']);
      },
      async start(mode = 'dev', args = [], timeout = 45000, env = {}) {
        const server = await request('start', { mode, args, timeout, env });
        return {
          origin: server.origin,
          stop: () => request('stop', { server: server.id }),
          logs: () => request('logs', { server: server.id }),
        };
      },
      async close() {
        if (closed) return;
        try { await request('close'); }
        finally { reader.close(); process.stdin.pause(); }
      },
    };
  } catch (error) {
    reader.close();
    process.stdin.pause();
    throw error;
  }
}
