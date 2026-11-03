import assert from 'node:assert/strict';
import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { chromium, expect } from '@playwright/test';
import { projectFixture } from '../../../kenvoy-tests/browser/fixture.mjs';

const fixture = await projectFixture();
let browser;
const errors = [];
const displays = { 'zh-CN': '中文（测试配置）', 'en-US': 'English · configured', 'ko-KR': '한국어 · 지정' };

async function languageContext(test) {
  const context = await browser.newContext({ locale: 'zh-CN' });
  await context.addInitScript(({ device, unreadable }) => {
    window.__languageAccess = { preferences: 0, reads: 0, writes: 0 };
    // 用真实存储中的旧选择干扰加载，并独立记录是否仍访问持久化偏好。
    localStorage.setItem('kenvoy.locale', 'en-US');
    const getItem = Storage.prototype.getItem;
    const setItem = Storage.prototype.setItem;
    Storage.prototype.getItem = function (key) {
      window.__languageAccess.reads += 1;
      return getItem.call(this, key);
    };
    Storage.prototype.setItem = function (key, value) {
      window.__languageAccess.writes += 1;
      return setItem.call(this, key, value);
    };
    Object.defineProperty(navigator, 'language', {
      configurable: true,
      get() { if (unreadable) throw new Error('测试设备语言不可读'); return device; },
    });
    Object.defineProperty(navigator, 'languages', {
      configurable: true,
      get() { window.__languageAccess.preferences += 1; return ['en-US', 'ko-KR']; },
    });
  }, test);
  const page = await context.newPage();
  page.on('pageerror', error => errors.push(error.message));
  return { context, page };
}

async function assertLocale(page, code) {
  await expect(page.locator('html')).toHaveAttribute('lang', code);
  await expect(page.getByTestId('sdk-locale')).toHaveText(code);
  await expect(page.getByRole('combobox')).toHaveValue(code);
}

async function assertNoPreferenceAccess(page) {
  assert.deepEqual(await page.evaluate(() => window.__languageAccess), { preferences: 0, reads: 0, writes: 0 });
}

try {
  await writeFile(join(fixture.project, 'kenvoy.toml'), `[project]
name='language-browser'
version='0.1.0'
min_cli_version='0.4.0'
format_version='0.4.0'
[server]
port=0
[languages]
fallback='zh-CN'
[languages.zh-CN]
alias='cn'
display='${displays['zh-CN']}'
[languages.en-US]
alias='en'
display='${displays['en-US']}'
[languages.ko-KR]
alias='ko'
display='${displays['ko-KR']}'
`);
  const translationsPath = join(fixture.project, 'translations.json');
  const translations = JSON.parse(await readFile(translationsPath, 'utf8'));
  translations['custom.greeting'] = { cn: '你好', en: 'Hello', ko: '안녕하세요' };
  await writeFile(translationsPath, JSON.stringify(translations));
  const source = join(fixture.project, 'src');
  await writeFile(join(source, 'LanguageProbe.tsx'), `import {useState} from 'react';
import {useI18n} from '@kenvoy/client';
export function LanguageProbe() {
  const {locale,t,setLocale}=useI18n();
  const [count,setCount]=useState(0);
  const [draft,setDraft]=useState('');
  return <section aria-label="语言SDK验收">
    <output data-testid="sdk-locale">{locale}</output>
    <p data-testid="greeting">{t('custom.greeting')}</p>
    <p data-testid="counter">{count}</p>
    <input aria-label="语言组件草稿" value={draft} onChange={event=>setDraft(event.target.value)}/>
    <button onClick={()=>setCount(value=>value+1)}>增加计数</button>
    <button onClick={()=>setLocale('EN')}>英文别名</button>
    <button onClick={()=>setLocale('KO')}>韩文别名</button>
    <button onClick={()=>setLocale('fr-FR')}>未配置语言</button>
    <button onClick={()=>setLocale('English · configured')}>显示名不是别名</button>
  </section>;
}
`);
  const appPath = join(source, 'app.tsx');
  const app = await readFile(appPath, 'utf8');
  assert.ok(app.includes('</page>'));
  await writeFile(appPath, `import {LanguageProbe} from './LanguageProbe';\n${app.replace('</page>', '<LanguageProbe/></page>')}`);
  const server = await fixture.start('run');
  browser = await chromium.launch({ headless: true });

  for (const test of [
    { device: 'en-US', expected: 'en-US' },
    { device: 'EN-us', expected: 'en-US' },
    { device: 'ko-KR', expected: 'ko-KR' },
    { device: 'en-GB', expected: 'zh-CN' },
    { device: 'en', expected: 'zh-CN' },
    { device: '', expected: 'zh-CN' },
    { unreadable: true, expected: 'zh-CN' },
  ]) {
    const { context, page } = await languageContext(test);
    try {
      await page.goto(server.origin + '/customers');
      await assertLocale(page, test.expected);
      const options = await page.getByRole('combobox').locator('option').evaluateAll(nodes => Object.fromEntries(nodes.map(node => [node.value, node.textContent])));
      assert.deepEqual(options, displays);
      await assertNoPreferenceAccess(page);
    } finally { await context.close(); }
  }

  const { context, page } = await languageContext({ device: 'zh-CN' });
  try {
    await page.goto(server.origin + '/customers');
    await assertLocale(page, 'zh-CN');
    await page.getByLabel('客户姓名', { exact: false }).fill('切换语言不能改变业务值');
    await page.getByLabel('语言组件草稿', { exact: true }).fill('保留Hooks输入');
    await page.getByRole('button', { name: '增加计数', exact: true }).click();
    await page.getByRole('combobox').selectOption('en-US');
    await assertLocale(page, 'en-US');
    await expect(page.getByRole('heading', { name: 'Customer registration', exact: true })).toBeVisible();
    await expect(page.getByLabel('Customer name', { exact: false })).toHaveValue('切换语言不能改变业务值');
    await expect(page.getByLabel('语言组件草稿', { exact: true })).toHaveValue('保留Hooks输入');
    await expect(page.getByTestId('counter')).toHaveText('1');
    await expect(page.getByTestId('greeting')).toHaveText('Hello');
    await page.getByRole('button', { name: '韩文别名', exact: true }).click();
    await assertLocale(page, 'ko-KR');
    await expect(page.getByTestId('greeting')).toHaveText('안녕하세요');
    await page.getByRole('button', { name: '显示名不是别名', exact: true }).click();
    await assertLocale(page, 'ko-KR');
    await page.getByRole('button', { name: '未配置语言', exact: true }).click();
    await assertLocale(page, 'ko-KR');
    await page.getByRole('button', { name: '英文别名', exact: true }).click();
    await assertLocale(page, 'en-US');
    await expect(page.getByTestId('counter')).toHaveText('1');
    await assertNoPreferenceAccess(page);
    await page.reload();
    await assertLocale(page, 'zh-CN');
    await expect(page.getByTestId('greeting')).toHaveText('你好');
    await expect(page.getByTestId('counter')).toHaveText('0');
    await assertNoPreferenceAccess(page);
  } finally { await context.close(); }

  assert.deepEqual(errors, []);
  console.log(JSON.stringify({ status: '通过', exact_device_language: true, unreadable_or_empty_uses_fallback: true, no_secondary_preferences_or_storage: true, configured_display: true, code_alias_lookup: true, in_page_draft_and_hooks_preserved: true, reload_resets_to_device: true }, null, 2));
  await server.stop();
} finally { try { if (browser) await browser.close(); } finally { await fixture.close(); } }
