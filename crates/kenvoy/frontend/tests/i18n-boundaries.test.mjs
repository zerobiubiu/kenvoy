import test from 'node:test';
import assert from 'node:assert/strict';
import { createFormatters } from '../src/domain/i18n/format.ts';
import { createI18n } from '../src/domain/i18n/translate.ts';
import { constraintHint, displayValue } from '../src/domain/i18n/display.ts';
import { validateField } from '../src/domain/validation.ts';

const configuration = () => ({ format_version: 1, default_locale: 'en-US', locales: ['en-US', 'zh-CN'], messages: {
  'en-US': { 'custom.items.one': '{count} item', 'custom.items.other': '{count} items' }, 'zh-CN': {},
} });

test('SDK样式日期选项可用于组件展示，日历日仍锚定UTC', () => {
  const format = createFormatters('en-US');
  assert.equal(format.formatDate('2026-01-02', { dateStyle: 'long', timeZone: 'America/Los_Angeles' }), 'January 2, 2026');
  const dateTime = format.formatDateTime('2026-01-02T03:04:05Z', { dateStyle: 'long', timeStyle: 'short', hour12: false, timeZone: 'UTC' });
  assert.match(dateTime, /January 2, 2026/);
  assert.match(dateTime, /\b03:04\b/);
});

test('回退文案按词典实际语言选择复数，不把中文other套到英文singular', () => {
  const source = configuration();
  const { t } = createI18n(source, 'zh-CN');
  assert.equal(t('custom.items', { count: 1 }), '1 item');
  assert.equal(t('custom.items', { count: 2 }), '2 items');
  source.messages['zh-CN'] = { 'custom.items': '本地 {count}' };
  assert.equal(createI18n(source, 'zh-CN').t('custom.items', { count: 1 }), '本地 1');
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
  const source = { format_version: 1, default_locale: 'en-US', locales: ['en-US', 'ar'], messages: { 'en-US': {}, ar: {} } };
  assert.equal(displayValue(1234, undefined, createI18n(source, 'ar')), '1234');
});
