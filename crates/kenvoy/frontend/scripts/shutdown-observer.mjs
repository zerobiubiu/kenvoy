// 配合独立终端中的真实 Ctrl+C；不把 Windows kill 当作优雅退出。
import assert from 'node:assert/strict';
import { chromium, expect } from '@playwright/test';
const [mode, origin] = process.argv.slice(2);
assert.ok(['observe', 'verify'].includes(mode));
assert.match(origin, /^http:\/\/127\.0\.0\.1:\d+$/);
const browser = await chromium.launch({ headless: true });
try {
  const page = await browser.newPage();
  await page.addInitScript(() => {
    const NativeWebSocket=window.WebSocket;
    window.__kenvoyObservedSockets=[];
    window.WebSocket=class extends NativeWebSocket {
      constructor(...arguments_) {super(...arguments_);window.__kenvoyObservedSockets.push(this);}
    };
  });
  const sockets = new Set();
  page.on('websocket', socket => { sockets.add(socket); socket.on('close', () => sockets.delete(socket)); });
  await page.goto(origin + '/customers');
  if (mode === 'observe') {
    await page.getByLabel('客户姓名', { exact: false }).fill('优雅退出保留客户');
    await page.getByRole('button', { name: '提交信息', exact: true }).click();
    await expect(page.getByText('提交成功', { exact: true })).toBeVisible();
    const activeSockets=()=>page.evaluate(()=>window.__kenvoyObservedSockets.filter(socket=>socket.readyState===WebSocket.OPEN||socket.readyState===WebSocket.CONNECTING).length);
    // 导航销毁的旧 Playwright 句柄可能没有 close 事件；以当前文档的原生状态为准。
    console.log(JSON.stringify({ status: '等待终端 Ctrl+C', saved: true, active_websockets: await activeSockets(), observed_handles:sockets.size }));
    await expect.poll(async () => {
      try { await fetch(origin + '/_kenvoy/session', { signal: AbortSignal.timeout(1000) }); return false; }
      catch { return true; }
    }, { timeout: 90000, intervals: [200, 500] }).toBe(true);
    await expect.poll(activeSockets, { timeout: 10000 }).toBe(0);
    console.log(JSON.stringify({ status: '服务端口与开发连接已关闭', mode }));
  } else {
    await page.goto(origin + '/admin/login');
    await page.getByLabel('用户名', { exact: false }).fill('admin');
    await page.getByLabel('密码', { exact: false }).fill('shutdown-test-password-891');
    await page.getByRole('button', { name: '登录', exact: true }).click();
    await expect(page.getByRole('cell', { name: '优雅退出保留客户', exact: true }).first()).toBeVisible();
    console.log(JSON.stringify({ status: '重启后真实记录保留', mode }));
  }
} finally { await browser.close(); }
