import assert from 'node:assert/strict';
import { writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { chromium, expect } from '@playwright/test';
import { projectFixture } from '../../../../tools/tests/project-harness.mjs';

const fixture = await projectFixture();
let browser;
try {
  await writeFile(join(fixture.project, 'src/app.tsx'), `export default <app name="字段验收">
    <resource name="entries">
      <text name="title" label="标题" required maxLength={5} />
      <textarea name="notes" label="备注" maxLength={100} />
      <number name="quantity" label="数量" min={0} max={100} />
      <boolean name="active" label="启用" required default={false} />
      <date name="registered" label="登记日期" />
      <datetime name="occurred" label="发生时间" />
      <select name="category" label="类别" required><option value="a" label="甲类"/><option value="b" label="乙类"/></select>
    </resource><page path="/entries" title="字段验收"><form resource="entries" mode="create"/></page>
  </app>;`);
  const server = await fixture.start();
  browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({ locale: 'zh-CN', viewport: { width: 1280, height: 1000 } });
  const errors = []; page.on('pageerror', error => errors.push(error.message));
  await page.goto(server.origin + '/entries');
  await expect(page.getByRole('heading', { name: '字段验收', exact: true })).toBeVisible();
  await page.getByLabel('标题', { exact: false }).fill('😀'.repeat(5));
  await page.getByLabel('备注', { exact: false }).fill('第一行\n第二行');
  await page.getByLabel('数量', { exact: false }).fill('12.5');
  await page.getByLabel('登记日期', { exact: false }).fill('2024-02-29');
  await page.getByLabel('发生时间', { exact: false }).fill('2024-02-29T23:30:00');
  await page.getByRole('combobox', { name: '类别', exact: false }).click();
  await page.getByRole('option', { name: '乙类', exact: true }).click();
  await page.getByRole('button', { name: '提交信息', exact: true }).click();
  await expect(page.getByLabel('发生时间', { exact: false })).toHaveAttribute('aria-invalid', 'true');
  await page.getByLabel('发生时间', { exact: false }).fill('2024-02-29T23:30:00+08:00');
  const responsePromise = page.waitForResponse(response => response.url().endsWith('/api/entries') && response.request().method() === 'POST');
  await page.getByRole('button', { name: '提交信息', exact: true }).click();
  const response = await responsePromise; assert.equal(response.status(), 201);
  const record = (await response.json()).data;
  assert.equal(record.title, '😀'.repeat(5)); assert.equal(record.notes, '第一行\n第二行');
  assert.equal(record.quantity, 12.5); assert.equal(record.active, false);
  assert.equal(record.registered, '2024-02-29'); assert.equal(record.occurred, '2024-02-29T15:30:00.000Z');
  assert.equal(record.category, 'b');
  await expect(page.getByText('提交成功', { exact: true })).toBeVisible();
  assert.deepEqual(errors, []);
  console.log(JSON.stringify({ status: '通过', seven_field_controls: true, unicode_codepoints: true, false_default: true, explicit_timezone: true, real_sqlite_record: true }, null, 2));
  await server.stop();
} finally { if (browser) await browser.close(); await fixture.close(); }
