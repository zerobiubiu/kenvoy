#!/usr/bin/env node
'use strict';

const path = require('node:path');
const { spawn } = require('node:child_process');
const { constants } = require('node:os');
const { selectTarget, rebuildHelp } = require('../lib/config.cjs');
const { installedPath } = require('../lib/files.cjs');

function run() {
  const selected = selectTarget(path.resolve(__dirname, '..'));
  const { filename, stat } = installedPath(selected.root, selected.target);
  if (!stat) {
    throw new Error(`尚未安装 Kenvoy CLI 二进制；安装时可能使用了 --ignore-scripts。${rebuildHelp}`);
  }
  const child = spawn(filename, process.argv.slice(2), { stdio: 'inherit', shell: false });
  const handlers = new Map();
  for (const signal of ['SIGINT', 'SIGTERM', 'SIGHUP']) {
    const handler = () => {
      // 前台控制台已向子进程广播 Ctrl+C；Windows 再 kill(SIGINT) 会直接强杀。
      if (signal === 'SIGINT' && (process.platform === 'win32' || process.stdin.isTTY)) return;
      if (child.exitCode === null && child.signalCode === null) child.kill(signal);
    };
    handlers.set(signal, handler);
    process.on(signal, handler);
  }
  const cleanup = () => {
    for (const [signal, handler] of handlers) process.removeListener(signal, handler);
  };
  child.once('error', (error) => {
    cleanup();
    console.error(`无法启动 Kenvoy CLI（${error.code || 'UNKNOWN'}）。${rebuildHelp}`);
    process.exitCode = 1;
  });
  child.once('exit', (code, signal) => {
    cleanup();
    if (signal) {
      process.exitCode = 128 + (constants.signals[signal] || 1);
      try {
        process.kill(process.pid, signal);
      } catch {
        // 不支持向自身发送该信号的平台保留约定的非零退出码。
      }
    } else {
      process.exitCode = code === null ? 1 : code;
    }
  });
}

try {
  run();
} catch (error) {
  console.error(`kenvoy：${error.message}`);
  process.exitCode = 1;
}
