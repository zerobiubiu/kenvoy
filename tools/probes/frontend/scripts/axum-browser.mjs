import assert from 'node:assert/strict';
import { chromium, expect } from '@playwright/test';

// 由显式运行的 Rust 跨工具探针启动 Axum；本脚本不另起替代 HTTP 服务。
let browser;
try {
  assert.equal(process.argv.length, 3, '用法：node scripts/axum-browser.mjs <loopback-baseURL>');
  const base = new URL(process.argv[2]);
  assert.equal(base.protocol, 'http:', '探针只接受本机 HTTP loopback 地址');
  assert(['127.0.0.1', 'localhost', '[::1]'].includes(base.hostname), '拒绝非 loopback 目标');
  assert(!base.username && !base.password && !base.search && !base.hash, '地址不得含凭据、查询或片段');
  assert(base.pathname === '/', 'baseURL 必须指向服务根目录');

  browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  const pageErrors = [];
  page.on('pageerror', (error) => pageErrors.push(error.message));
  const navigation = await page.goto(new URL('/customers', base).href);
  assert.equal(navigation.status(), 200, 'Axum 页面必须返回 HTTP 200');
  await expect(page.getByRole('heading', { name: '客户登记技术探针' })).toBeVisible();
  await expect(page.getByLabel('客户姓名')).toBeVisible();
  await expect(page.getByLabel('联系电话')).toBeVisible();
  await expect(page.getByLabel('年龄')).toBeVisible();
  await page.getByLabel('客户姓名').fill('页面字段交互');
  await page.getByLabel('联系电话').fill('13800000000');
  await page.getByLabel('年龄').fill('25');
  await page.getByTestId('counter').click({ clickCount: 2 });
  await expect(page.getByTestId('counter')).toHaveText('点击次数：2');
  await page.getByRole('button', { name: '验证交互' }).click();
  await expect(page.getByText('浏览器表单交互完成；未写入数据库。')).toBeVisible();

  // API 请求独立于当前探针表单；没有把本地表单反馈冒充数据库提交。
  const api = await page.evaluate(async () => {
    const summarize = async (response) => ({
      status: response.status,
      contentType: response.headers.get('content-type') ?? '',
      body: await response.text(),
    });
    const created = await summarize(await fetch('/api/customers', {
      method: 'POST',
      credentials: 'omit',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name: 'Axum浏览器', age: 25 }),
    }));
    const anonymousList = await summarize(await fetch('/api/customers', { credentials: 'omit' }));
    const unknownResource = await summarize(await fetch('/api/not_registered', { credentials: 'omit' }));
    return { origin: location.origin, created, anonymousList, unknownResource };
  });
  assert.equal(api.origin, base.origin, '页面与 API 必须同源');
  assert.equal(api.created.status, 201, '真实 Axum POST 应返回 HTTP 201');
  assert.match(api.created.contentType, /application\/json/i, '创建响应应为 JSON');
  const record = JSON.parse(api.created.body);
  assert(Number.isSafeInteger(record.id) && record.id > 0, '创建响应须含正整数记录 ID');
  assert.equal(record.name, 'Axum浏览器');
  assert.equal(record.age, 25);
  assert.equal(api.anonymousList.status, 401, '匿名列表读取必须拒绝');
  assert.equal(api.unknownResource.status, 404, '未注册资源必须返回 404');
  for (const response of [api.anonymousList, api.unknownResource]) {
    assert(!/text\/html/i.test(response.contentType), 'API 错误不能被 CSR HTML fallback 覆盖');
    assert(!/<(?:!doctype|html|body)\b/i.test(response.body), 'API 错误不能返回 HTML 文档');
  }
  assert.deepEqual(pageErrors, [], '真实浏览器不应产生 pageerror');
  process.stdout.write(`${JSON.stringify({
    probe: 'P03-P04-axum-browser',
    status: 'passed',
    browser: browser.version(),
    page: '/customers',
    muiFields: 3,
    hookCounter: 2,
    created: { status: api.created.status, record },
    anonymousList: api.anonymousList.status,
    unknownResource: api.unknownResource.status,
    pageErrors: pageErrors.length,
    boundary: '当前表单仅验证本地交互；POST 由浏览器独立同源 fetch 执行，SQLite 文件读回由 Rust 测试复查。',
  }, null, 2)}\n`);
} catch (error) {
  process.exitCode = 1;
  process.stderr.write(`${error.stack ?? String(error)}\n`);
} finally {
  if (browser) await browser.close();
}
