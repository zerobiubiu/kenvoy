import test from 'node:test';
import assert from 'node:assert/strict';
import { createFormatters } from '../src/domain/i18n/format.ts';
import { createI18n } from '../src/domain/i18n/translate.ts';
import { isI18nBundle } from '../src/domain/i18n/model.ts';
import { constraintHint, displayValue } from '../src/domain/i18n/display.ts';
import { validateField } from '../src/domain/validation.ts';

const configuration = () => ({
  format_version: 2, fallback: 'en-US',
  languages: [{ code: 'en-US', display: 'English' }, { code: 'zh-CN', display: '简体中文' }],
  messages: {
    'en-US': { 'custom.items.one': '{count} item', 'custom.items.other': '{count} items' },
    'zh-CN': { 'custom.items.one': '{count} 条', 'custom.items.other': '{count} 条' },
  },
});

test('SDK样式日期选项可用于组件展示，日历日仍锚定UTC', () => {
  const format = createFormatters('en-US');
  assert.equal(format.formatDate('2026-01-02', { dateStyle: 'long', timeZone: 'America/Los_Angeles' }), 'January 2, 2026');
  const dateTime = format.formatDateTime('2026-01-02T03:04:05Z', { dateStyle: 'long', timeStyle: 'short', hour12: false, timeZone: 'UTC' });
  assert.match(dateTime, /January 2, 2026/);
  assert.match(dateTime, /\b03:04\b/);
});

test('完整业务矩阵按当前语言选择复数，不依赖某种语言缺词', () => {
  const source = configuration();
  assert.equal(isI18nBundle(source), true);
  const chinese = createI18n(source, 'zh-CN');
  const english = createI18n(source, 'en-US');
  assert.equal(chinese.t('custom.items', { count: 1 }), '1 条');
  assert.equal(chinese.t('custom.items', { count: 2 }), '2 条');
  assert.equal(english.t('custom.items', { count: 1 }), '1 item');
  assert.equal(english.t('custom.items', { count: 2 }), '2 items');
});

test('未内置语言回退系统文案时按词典实际语言选择复数', () => {
  const source = {
    format_version: 2, fallback: 'en-US',
    languages: [{ code: 'en-US', display: 'English' }, { code: 'ko-KR', display: '한국어' }],
    messages: { 'en-US': {}, 'ko-KR': {} },
  };
  assert.equal(isI18nBundle(source), true);
  const { t } = createI18n(source, 'ko-KR');
  assert.equal(t('ui.files.selected', { count: 1, display_count: 1, limit: '' }), '1 file selected');
  assert.equal(t('ui.files.selected', { count: 2, display_count: 2, limit: '' }), '2 files selected');
});

test('原生数字约束提示保留实际边界精度，与输入校验含义一致', () => {
  const field = { name: 'amount', label: '数值', kind: 'number', required: false, integer: false, min: 0.0001, max: 1.2345, max_length: null, unique: false, options: [] };
  const hint = constraintHint(field, createI18n(configuration(), 'en-US'));
  assert.match(hint, /\b0\.0001\b/);
  assert.match(hint, /\b1\.2345\b/);
  assert.equal(validateField(field, 0).ok, false);
  assert.equal(validateField(field, 0.0001).ok, true);
  assert.equal(validateField(field, 1.2345).ok, true);
  assert.equal(validateField(field, 1.235).ok, false);
});

test('没有业务数字类型的系统标识不因语言切换改成另一种数字字形', () => {
  const source = {
    format_version: 2, fallback: 'en-US',
    languages: [{ code: 'en-US', display: 'English' }, { code: 'ar', display: 'العربية' }],
    messages: { 'en-US': {}, ar: {} },
  };
  assert.equal(displayValue(1234, undefined, createI18n(source, 'ar')), '1234');
});
