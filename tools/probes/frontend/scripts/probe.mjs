import assert from 'node:assert/strict';
import { createServer as createHttpServer } from 'node:http';
import { readFile, writeFile, mkdir, readdir } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { chromium, expect } from '@playwright/test';
import { build, createServer } from 'vite';

// 独立 P04 探针：浏览器与临时服务均在 finally 中回收，源码编辑也恢复。
const root = fileURLToPath(new URL('../', import.meta.url));
process.chdir(root);
const packageManifest = JSON.parse(await readFile(path.join(root, 'package.json'), 'utf8'));
const cardPath = path.join(root, 'src/CustomerCard.tsx');
const cssPath = path.join(root, 'src/styles.css');
const mainPath = path.join(root, 'src/main.tsx');
const [originalCard, originalCss, originalMain] = await Promise.all([
  readFile(cardPath, 'utf8'), readFile(cssPath, 'utf8'), readFile(mainPath, 'utf8'),
]);
const results = {
  probe: 'P04-frontend',
  time: new Date().toISOString(),
  node: process.version,
  platform: process.platform,
  versions: { ...packageManifest.dependencies, ...packageManifest.devDependencies },
  checks: [],
  limitations: [
    '独立技术探针，不是正式 Kenvoy Runtime；JSON 描述不是 KEngine 输出。',
    '表单仅执行浏览器约束校验与交互，不连接 API 或 SQLite。',
    'server 边界为本探针显式插件规则，未覆盖完整框架模块分类。',
    'Fast Refresh 只证明本次函数组件和编辑序列，不承诺所有导出、组件和错误场景保留状态。',
    '生产静态服务使用 Node 探针；Axum 同端口集成由 P03 另行验证。',
  ],
};
const passed = (name, details = {}) => {
  results.checks.push({ name, status: 'passed', ...details });
  process.stdout.write(`通过：${name}\n`);
};
const filesUnder = async (directory) => {
  const files = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const entryPath = path.join(directory, entry.name);
    if (entry.isDirectory()) files.push(...await filesUnder(entryPath));
    else files.push(entryPath);
  }
  return files;
};
const configFile = path.join(root, 'vite.config.mjs');
const initialNodeEnv = process.env.NODE_ENV;
let browser;
let dev;
let production;
try {
  await build({ root, configFile });
  passed('生产构建');
  const assets = await filesUnder(path.join(root, 'dist'));
  const scanned = [];
  for (const file of assets) {
    if (!/\.(?:js|css|html|json|map)$/.test(file)) continue;
    const content = await readFile(file, 'utf8');
    assert(!content.includes('KENVoy_SERVER_ONLY_SECRET_20261101'), `服务端标记泄漏：${file}`);
    assert(!content.includes('private.server.ts'), `服务端路径泄漏：${file}`);
    assert(!content.includes('privateCalculation'), `服务端函数名泄漏：${file}`);
    scanned.push(path.relative(root, file));
  }
  assert(scanned.some((file) => file.endsWith('.map')), '未生成用于检查的 source map');
  passed('客户端构建资产与 source map 不含服务端标记/路径', { files: scanned });

  // 实际尝试直接导入服务端模块，确认构建失败而非依赖 tree shaking 隐藏泄漏。
  await writeFile(mainPath, `${originalMain}\nimport { serverOnlyMarker } from '../server/private.server';\nconsole.log(serverOnlyMarker);\n`);
  let rejected = false;
  try {
    await build({ root, configFile, build: { write: false } });
  } catch (error) {
    rejected = String(error).includes('P04_SERVER_BOUNDARY');
  } finally {
    await writeFile(mainPath, originalMain);
  }
  assert(rejected, '客户端 server 导入未按边界规则拒绝');
  passed('客户端直接导入 server 模块被拒绝');

  browser = await chromium.launch({ headless: true });
  results.browser = browser.version();
  const page = await browser.newPage();
  const browserErrors = [];
  page.on('pageerror', (error) => browserErrors.push(error.message));
  production = createHttpServer(async (request, response) => {
    try {
      const requested = new URL(request.url, 'http://127.0.0.1').pathname;
      const relative = requested === '/customers' || requested === '/' ? 'index.html' : requested.slice(1);
      const file = path.resolve(root, 'dist', relative);
      const distRoot = path.resolve(root, 'dist') + path.sep;
      if (!file.startsWith(distRoot)) { response.writeHead(403); response.end(); return; }
      const data = await readFile(file);
      const contentTypes = { '.html': 'text/html; charset=utf-8', '.js': 'text/javascript', '.css': 'text/css' };
      response.writeHead(200, { 'Content-Type': contentTypes[path.extname(file)] ?? 'application/octet-stream' });
      response.end(data);
    } catch { response.writeHead(404); response.end(); }
  });
  await new Promise((resolve) => production.listen(0, '127.0.0.1', resolve));
  const productionUrl = `http://127.0.0.1:${production.address().port}`;
  await page.goto(`${productionUrl}/customers`);
  await expect(page.getByRole('heading', { name: '客户登记技术探针' })).toBeVisible();
  await expect(page.getByLabel('客户姓名')).toBeVisible();
  await expect(page.getByLabel('联系电话')).toBeVisible();
  await expect(page.getByLabel('年龄')).toBeVisible();
  await page.getByLabel('客户姓名').fill('张三');
  await page.getByLabel('联系电话').fill('13800000000');
  await page.getByLabel('年龄').fill('25');
  await page.getByRole('button', { name: '验证交互' }).click();
  await expect(page.getByText('浏览器表单交互完成；未写入数据库。')).toBeVisible();
  assert.equal(browserErrors.length, 0, `生产页面错误：${browserErrors.join(', ')}`);
  passed('真实浏览器：生产资产的 JSON→React/MUI 表单与交互、Router 深层地址');

  // Vite build 在同一 Node 进程设置 production；dev 必须显式切换回开发环境。
  process.env.NODE_ENV = 'development';
  dev = await createServer({ root, configFile, mode: 'development', server: { host: '127.0.0.1', port: 0 } });
  await dev.listen();
  const devUrl = dev.resolvedUrls.local[0];
  await page.goto(new URL('/customers', devUrl).href);
  await expect(page.getByText('跨文件组件与 Props')).toBeVisible();
  const counter = page.getByTestId('counter');
  await counter.click({ clickCount: 3 });
  await expect(counter).toHaveText('点击次数：3');
  const changedCard = originalCard.replace('组件修改前', '组件修改后');
  await writeFile(cardPath, changedCard);
  await expect(page.getByTestId('refresh-label')).toHaveText('组件修改后', { timeout: 15000 });
  await expect(counter).toHaveText('点击次数：3');
  passed('跨文件 Props/Hooks 与 Fast Refresh：UI 编辑后计数器 3 保留');

  await writeFile(cssPath, originalCss.replace('rgb(23, 45, 67)', 'rgb(111, 22, 33)'));
  await expect(page.getByTestId('css-probe')).toHaveCSS('color', 'rgb(111, 22, 33)', { timeout: 15000 });
  await expect(counter).toHaveText('点击次数：3');
  passed('CSS 热更新且计数器 3 保留');

  await writeFile(cardPath, `${changedCard}\nexport function Broken( {\n`);
  await expect.poll(() => page.locator('vite-error-overlay').count(), { timeout: 15000 }).toBeGreaterThan(0);
  const overlayText = await page.locator('vite-error-overlay').evaluate((element) => element.shadowRoot?.textContent ?? '');
  assert(overlayText.length > 0, '错误遮罩缺少诊断内容');
  await writeFile(cardPath, changedCard);
  await expect.poll(() => page.locator('vite-error-overlay').count(), { timeout: 15000 }).toBe(0);
  await expect(page.getByTestId('refresh-label')).toHaveText('组件修改后');
  passed('真实语法错误显示与修复恢复', { countAfterRecovery: await counter.textContent() });
  await mkdir(path.join(root, '.probe'), { recursive: true });
  await page.screenshot({ path: path.join(root, '.probe/browser.png'), fullPage: true });
  results.status = 'passed';
} catch (error) {
  results.status = 'failed';
  results.error = error.stack ?? String(error);
  process.exitCode = 1;
  process.stderr.write(`${results.error}\n`);
} finally {
  await Promise.all([writeFile(cardPath, originalCard), writeFile(cssPath, originalCss), writeFile(mainPath, originalMain)]);
  if (browser) await browser.close();
  if (dev) await dev.close();
  if (production) await new Promise((resolve) => production.close(resolve));
  if (initialNodeEnv === undefined) delete process.env.NODE_ENV;
  else process.env.NODE_ENV = initialNodeEnv;
  await mkdir(path.join(root, '.probe'), { recursive: true });
  await writeFile(path.join(root, '.probe/results.json'), JSON.stringify(results, null, 2));
  process.stdout.write(`结果：${results.status}；已回收浏览器/服务并恢复探针源码。\n`);
}
