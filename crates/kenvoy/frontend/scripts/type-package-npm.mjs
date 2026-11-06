import { spawnSync } from 'node:child_process';
import { existsSync, realpathSync } from 'node:fs';
import { basename, delimiter, dirname, join } from 'node:path';

function npmCli() {
  const configured = process.env.npm_execpath;
  if (configured && basename(configured) === 'npm-cli.js' && existsSync(configured)) return configured;

  // 直接执行 npm 的 Node 入口，避免 Windows .cmd 与含空格路径依赖 shell 转义。
  const directories = [dirname(process.execPath), ...(process.env.PATH ?? '').split(delimiter)].filter(Boolean);
  for (const directory of directories) {
    const installed = join(directory, 'node_modules/npm/bin/npm-cli.js');
    if (existsSync(installed)) return installed;
    const executable = join(directory, process.platform === 'win32' ? 'npm.cmd' : 'npm');
    if (existsSync(executable)) {
      const resolved = realpathSync(executable);
      if (basename(resolved) === 'npm-cli.js') return resolved;
    }
  }
  throw new Error('找不到 npm 的 npm-cli.js；请安装包含 npm 的 Node.js 后重试');
}

export function runNpm(args, cwd) {
  const result = spawnSync(process.execPath, [npmCli(), ...args], {
    cwd, encoding: 'utf8', timeout: 120_000, windowsHide: true,
  });
  if (result.error) throw new Error(`npm ${args[0]} 无法执行：${result.error.message}`, { cause: result.error });
  if (result.status !== 0) {
    throw new Error(`npm ${args[0]} 失败（${result.signal ?? result.status}）：\n${result.stdout}${result.stderr}`);
  }
  return result.stdout;
}
