// 真实 SQLite、授权 API、第三方 React 字段重载与原生控件使用同一声明。
import assert from 'node:assert/strict';
import { cp, mkdir, readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium, expect } from '@playwright/test';
import { projectFixture } from '../../../kenvoy-tests/browser/fixture.mjs';

const fixture = await projectFixture();
const example = fileURLToPath(new URL('../../../../examples/options-app/', import.meta.url));
const output = fileURLToPath(new URL('../../../../artifacts/component-bindings/', import.meta.url));
const password = 'isolated-options-test-4582';
let browser;
try {
  await cp(join(example, 'src'), join(fixture.project, 'src'), { recursive: true });
  const entryPath = join(fixture.project, 'src/app.tsx');
  await writeFile(entryPath, (await readFile(entryPath, 'utf8'))
    .replace('CustomSelect, CustomForm', 'CustomNumber, CustomText, CustomSelect, CustomForm')
    .replace('<override name="select"', '<override name="number" component={CustomNumber} /><override name="text" component={CustomText} /><override name="select"')
    .replace('<resource name="entries">', '<resource name="entries"><text name="note" label="可空备注" /><number name="test_quantity" label="测试数量" />'));
  const componentsPath = join(fixture.project, 'src/components.tsx');
  await writeFile(componentsPath, await readFile(componentsPath, 'utf8') + `
    export function CustomText(){const b=useField();return <label>{b.field.label}<input name={b.field.name} value={b.draft} disabled={b.disabled} onChange={event=>b.onDraftChange(event.target.value)} />
      {b.field.name==='note'&&<><button type="button" disabled={b.disabled} onClick={()=>b.onChange(null)}>清空为null</button><button type="button" disabled={b.disabled} onClick={()=>b.onChange({bad:true} as never)}>测试对象拦截</button></>}
      {b.error&&<span role="alert">{b.error}</span>}</label>;}
    export function CustomNumber(){const b=useField();return <div><label>{b.field.label}<input name={b.field.name} value={b.draft} disabled={b.disabled} aria-invalid={Boolean(b.error)} onChange={event=>b.onDraftChange(event.target.value)} /></label>
      <output data-testid="number-binding-state" data-value-type={b.value===null?'null':typeof b.value} data-valid={String(b.valid)} />
      {b.error&&<span role="alert">{b.error}</span>}</div>;}
  `);
  for (const name of ['package.json', 'package-lock.json']) await cp(join(example, name), join(fixture.project, name));
  const migration = await fixture.sealMigration('选项与字段重载夹具的显式Schema迁移', { format: 1, resolved_risks: ['drop_table:customers'] });
  await fixture.command(['--project', 'app', 'migrate', 'apply', ...(migration.confirmation_checksum ? ['--confirm', migration.confirmation_checksum] : [])]);
  await fixture.command(['--project', 'app', 'admin', 'init', '--username', 'admin', '--password-stdin'], password + '\n');
  let server = await fixture.start('run', ['--install-deps'], 240000);
  browser = await chromium.launch({ headless: true });
  const admin = await browser.newContext({ locale: 'zh-CN' });
  async function headers(context, form) {
    const session = await (await context.request.get(server.origin + '/_kenvoy/session')).json();
    return { origin: server.origin, 'x-kenvoy-csrf': session.csrf_token, 'x-kenvoy-generation': String(session.generation), ...(form ? { 'x-kenvoy-form': encodeURIComponent(form) } : {}) };
  }
  async function login(context) {
    const response = await context.request.post(server.origin + '/_kenvoy/login', { headers: await headers(context), data: { username: 'admin', password } });
    assert.equal(response.status(), 200, await response.text());
  }
  await login(admin);
  for (const row of [
    { code: 'hz', name: '杭州', province: 'zj' },
    { code: 'nj', name: '南京', province: 'js' },
    ...Array.from({ length: 23 }, (_, index) => ({ code: 'z' + index, name: '浙江测试城市' + String(index).padStart(2, '0'), province: 'zj' })),
  ]) {
    const response = await admin.request.post(server.origin + '/api/cities', { headers: await headers(admin), data: { ...row, private_note: 'PRIVATE_CITY_DATA' } });
    assert.equal(response.status(), 201, await response.text());
  }
  const context = await browser.newContext({ viewport: { width: 1280, height: 1000 }, locale: 'zh-CN' });
  const page = await context.newPage();
  const errors = []; page.on('pageerror', error => errors.push(error.message));
  await page.goto(server.origin + '/entries');
  await expect(page.getByTestId('resource-summary')).toHaveText('表单使用 entries 的 6 个原生字段。');
  await expect(page.locator('.custom-form[data-resource="entries"]')).toBeVisible();
  const province = page.getByRole('combobox', { name: '省份', exact: false });
  const service = page.getByRole('combobox', { name: '服务区', exact: false });
  const city = page.getByRole('combobox', { name: '城市', exact: false });
  await expect(city).toBeDisabled();
  await page.locator('input[name="note"]').fill('待清空');
  await page.getByRole('button', { name: '测试对象拦截', exact: true }).click();
  await expect(page.getByText('必须为字符串', { exact: false }).first()).toBeVisible();
  await expect(page.locator('input[name="note"]')).toHaveValue('待清空');
  await page.getByRole('button', { name: '清空为null', exact: true }).click();
  await expect(page.locator('input[name="note"]')).toHaveValue('');
  await province.selectOption('zj');
  await expect(city).toBeEnabled();
  await service.selectOption('east');
  await city.selectOption('hz');
  await expect(page.getByRole('button', { name: '下一组选项' })).toBeEnabled();
  await page.getByRole('button', { name: '下一组选项' }).click();
  await expect(page.getByText('第 2 页，共 24 项')).toBeVisible();
  await expect(city).toHaveValue('hz');
  await page.getByLabel('城市选项搜索', { exact: true }).fill('杭州');
  await expect(page.getByText('第 1 页，共 1 项')).toBeVisible();
  await expect(city).toHaveValue('hz');
  await page.getByLabel('城市选项搜索', { exact: true }).fill('');
  await expect(page.getByText('第 1 页，共 24 项')).toBeVisible();
  await province.selectOption('js');
  await expect(service).toHaveValue('');
  await expect(city).toHaveValue('');
  await expect(city.locator('option[value="nj"]')).toHaveCount(1);
  await expect(city.locator('option[value="hz"]')).toHaveCount(0);
  await service.selectOption('north');
  await city.selectOption('nj');
  await page.getByLabel('客户姓名', { exact: false }).fill('联动客户');
  // 只检查测试字段的类型/有效状态，避免把业务字段值暴露在诊断 DOM 中。
  const quantity = page.getByLabel('测试数量', { exact: true });
  const numberState = page.getByTestId('number-binding-state');
  let createRequests = 0;
  page.on('request', request => { if (request.url().endsWith('/api/entries') && request.method() === 'POST') createRequests++; });
  await quantity.fill('not-a-number');
  await expect(quantity).toHaveValue('not-a-number');
  await expect(numberState).toHaveAttribute('data-value-type', 'null');
  await expect(numberState).toHaveAttribute('data-valid', 'false');
  await page.getByRole('button', { name: '提交信息', exact: true }).click();
  await expect(quantity).toHaveAttribute('aria-invalid', 'true');
  await expect(quantity).toHaveValue('not-a-number');
  assert.equal(createRequests, 0, '无效草稿不能因绑定值为 null 而提交');
  await quantity.fill('12.5');
  await expect(numberState).toHaveAttribute('data-value-type', 'number');
  await expect(numberState).toHaveAttribute('data-valid', 'true');
  const submitted = page.waitForResponse(response => response.url().endsWith('/api/entries') && response.request().method() === 'POST');
  await page.getByRole('button', { name: '提交信息', exact: true }).click();
  const created = await (await submitted).json();
  assert.equal(created.data.city, 'nj'); assert.equal(created.data.province, 'js');
  assert.equal(typeof created.data.city, 'string');
  assert.equal(created.data.note, null);
  assert.equal(created.data.test_quantity, 12.5);
  assert.equal(typeof created.data.test_quantity, 'number');
  assert.equal(createRequests, 1, '只有修正后的有效记录可以提交');
  await expect(page.getByText('提交成功', { exact: true })).toBeVisible();
  const application = await (await context.request.get(server.origin + '/_kenvoy/application')).json();
  assert.equal(application.resources.some(resource => resource.name === 'cities'), false);
  assert.equal((await context.request.get(server.origin + '/api/cities')).status(), 401);
  const form = application.pages[0].components.find(node => node.type === 'form').id;
  const projected = await context.request.post(server.origin + '/api/entries/options/city', { headers: await headers(context, form), data: { dependencies: { province: 'js' } } });
  assert.equal(projected.status(), 200);
  const projection = await projected.json();
  assert.deepEqual(projection.data, [{ value: 'nj', label: '南京' }]);
  assert.ok(!JSON.stringify(projection).includes('PRIVATE_CITY_DATA'));
  const forged = await context.request.post(server.origin + '/api/entries', { headers: await headers(context, form), data: { name: '伪造', province: 'zj', service: 'east', city: 'nj' } });
  assert.equal(forged.status(), 422);
  const wrongType = await context.request.post(server.origin + '/api/entries', { headers: await headers(context, form), data: { name: '对象', province: 'js', service: 'north', city: { value: 'nj', label: '南京' } } });
  assert.equal(wrongType.status(), 422);
  const patch = await admin.request.patch(server.origin + '/api/entries/' + created.data.id, { headers: await headers(admin), data: { province: 'zj' } });
  assert.equal(patch.status(), 422);
  const edit = await admin.newPage();
  await edit.goto(server.origin + '/admin/entries?view=edit&id=' + created.data.id);
  await expect(edit.getByRole('combobox', { name: '城市', exact: false })).toHaveValue('nj');
  await expect(edit.getByRole('combobox', { name: '城市', exact: false })).toBeEnabled();
  await edit.getByLabel('客户姓名', { exact: false }).fill('联动已编辑');
  await edit.getByRole('button', { name: '保存修改', exact: true }).click();
  await expect(edit.getByText(`记录 ${created.data.id} 已更新。`, { exact: true })).toBeVisible();
  await mkdir(output, { recursive: true });
  await page.screenshot({ path: join(output, 'override-desktop.png'), fullPage: true });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.screenshot({ path: join(output, 'override-mobile.png'), fullPage: true });
  assert.deepEqual(errors, []);
  await server.stop();
  // 去除重载并重启，原生控件仍使用同一资源和已持久化数据。
  const source = await readFile(join(fixture.project, 'src/app.tsx'), 'utf8');
  await writeFile(join(fixture.project, 'src/app.tsx'), source.replace(/\s*<override[^>]+\/>/g, ''));
  server = await fixture.start('run', [], 240000);
  await page.goto(server.origin + '/entries');
  await expect(page.locator('.custom-select')).toHaveCount(0);
  await page.getByRole('combobox', { name: '省份', exact: false }).selectOption('zj');
  await expect(page.getByRole('combobox', { name: '城市', exact: false })).toBeEnabled();
  await page.getByRole('combobox', { name: '城市', exact: false }).selectOption('hz');
  await page.screenshot({ path: join(output, 'native-options.png'), fullPage: true });
  await login(admin);
  const persisted = await (await admin.request.get(server.origin + '/api/entries/' + created.data.id)).json();
  assert.equal(persisted.data.name, '联动已编辑'); assert.equal(persisted.data.city, 'nj');
  console.log(JSON.stringify({ status: '通过', static_cascade: true, remote_projection: true, pagination_search_selected: true, radix_override: true, sdk_resource: true, stable_number_binding: true, invalid_draft_preserved: true, server_revalidation: true, merged_patch_validation: true, native_same_contract: true, restart_persistence: true }, null, 2));
  await server.stop();
} finally { try { if (browser) await browser.close(); } finally { await fixture.close(); } }
