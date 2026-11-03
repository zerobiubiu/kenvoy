import test from 'node:test';
import assert from 'node:assert/strict';
import { localeDirection, matchLocale, selectLocale } from '../src/domain/i18n/locale.ts';
import { createFormatters } from '../src/domain/i18n/format.ts';
import { createI18n } from '../src/domain/i18n/translate.ts';
import { LEGACY_I18N, isI18nBundle } from '../src/domain/i18n/model.ts';
import { displayValue } from '../src/domain/i18n/display.ts';
import { draftValue, normalizeInput, validateField } from '../src/domain/validation.ts';

const bundle = (messages = {}) => ({ format_version: 1, default_locale: 'zh-CN', locales: ['zh-CN', 'en-US'], messages: { 'zh-CN': {}, 'en-US': {}, ...messages } });
const field = (kind, rules = {}) => ({ name: 'value', label: '值', kind, required: false, integer: false, min: null, max: null, max_length: null, unique: false, options: [], ...rules });

test('语言选择优先保存偏好，其次浏览器顺序与基础语言匹配，最后项目默认', () => {
  const configuration = bundle();
  assert.equal(selectLocale(configuration, 'en-US', ['zh-CN']), 'en-US');
  assert.equal(selectLocale(configuration, 'EN-us', ['zh-CN']), 'en-US');
  assert.equal(selectLocale(configuration, 'en-AU', ['zh-CN']), 'en-US');
  assert.equal(selectLocale(configuration, 'de-DE', ['fr-FR', 'en-GB', 'zh-CN']), 'en-US');
  assert.equal(selectLocale(configuration, null, ['zh-TW', 'en-US']), 'zh-CN');
  assert.equal(selectLocale(configuration, 'ar', ['de-DE']), 'zh-CN');
  assert.equal(selectLocale(LEGACY_I18N, 'en-US', ['en-US']), 'zh-CN');
  assert.equal(matchLocale('en_US', configuration.locales), undefined);
  assert.equal(matchLocale('en-GB', ['en-US', 'en-GB']), 'en-GB');
  assert.equal(localeDirection('ar-EG'), 'rtl');
  assert.equal(localeDirection('he-IL'), 'rtl');
  assert.equal(localeDirection('az-Arab'), 'rtl');
  assert.equal(localeDirection('az-Latn'), 'ltr');
  assert.equal(localeDirection('ku'), 'ltr');
  assert.equal(localeDirection('en-US'), 'ltr');
});

test('词条按语言与默认语言回退，命名参数仅插值而不执行HTML', () => {
  const configuration = bundle({
    'zh-CN': { 'custom.default': '默认 {name}', 'custom.empty': '', 'custom.count.other': '{count} 条' },
    'en-US': { 'custom.greeting': 'Hello {name}; {missing}', 'custom.count.one': '{count} entry', 'custom.count.other': '{count} entries', 'ui.login': 'Sign in' },
  });
  const english = createI18n(configuration, 'en-US');
  assert.equal(english.t('custom.greeting', { name: '<img src=x onerror=alert(1)>' }), 'Hello <img src=x onerror=alert(1)>; {missing}');
  assert.equal(english.t('custom.default', { name: 'Ada' }), '默认 Ada');
  assert.equal(english.t('custom.empty', {}, 'fallback'), '');
  assert.equal(english.t('custom.unknown', { value: 3 }, 'Fallback {value}'), 'Fallback 3');
  assert.equal(english.t('custom.missing'), 'custom.missing');
  assert.equal(english.t('custom.count', { count: 1 }), '1 entry');
  assert.equal(english.t('custom.count', { count: 2 }), '2 entries');
  assert.equal(english.t('custom.count', { count: 0 }), '0 entries');
  assert.equal(english.t('ui.login'), 'Sign in');
  assert.equal(createI18n(configuration, 'de-DE').locale, 'zh-CN');
  assert.equal(createI18n(LEGACY_I18N, 'zh-CN').t('ui.form.submit'), '提交信息');
});

test('Intl.PluralRules支持不止单复数两类，未内置语言使用默认系统文案', () => {
  const arabic = { format_version: 1, default_locale: 'en-US', locales: ['en-US', 'ar-EG'], messages: {
    'en-US': {}, 'ar-EG': { 'items.zero': 'zero {count}', 'items.one': 'one {count}', 'items.two': 'two {count}', 'items.few': 'few {count}', 'items.many': 'many {count}', 'items.other': 'other {count}' },
  } };
  const { t } = createI18n(arabic, 'ar-EG');
  for (const [count, expected] of [[0, 'zero'], [1, 'one'], [2, 'two'], [3, 'few'], [11, 'many'], [100, 'other']]) {
    assert.equal(t('items', { count }), `${expected} ${count}`);
  }
  assert.equal(t('ui.form.submit'), 'Submit information');
});

test('日期展示固定日历日，日期时间时区独立于语言且不猜测无时区输入', () => {
  const english = createFormatters('en-US');
  const chinese = createFormatters('zh-CN');
  const calendar = '2026-01-02';
  assert.equal(english.formatDate(calendar), '01/02/2026');
  assert.equal(english.formatDate(calendar, { timeZone: 'America/Los_Angeles' }), '01/02/2026');
  assert.equal(english.formatDate('0001-01-01'), '01/01/1');
  for (const invalid of ['2026-02-30', '2026-13-01', '0000-01-01', '2026-01-02T00:00:00']) assert.equal(english.formatDate(invalid), invalid);
  const timestamp = '2026-01-02T01:00:00+08:00';
  const instant = new Date(timestamp);
  const options = { year: 'numeric', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit', second: '2-digit', timeZone: 'America/Los_Angeles' };
  assert.equal(english.formatDateTime(timestamp, options), new Intl.DateTimeFormat('en-US', options).format(instant));
  assert.equal(chinese.formatDateTime(timestamp, options), new Intl.DateTimeFormat('zh-CN', options).format(instant));
  assert.equal(english.formatDateTime('2026-01-02T01:00'), '2026-01-02T01:00');
  assert.equal(timestamp, '2026-01-02T01:00:00+08:00');
});

test('数字/布尔/选项展示使用locale，草稿与有效JSON仍保留规范值', () => {
  const i18n = createI18n(bundle(), 'en-US');
  const number = field('number');
  const calendar = field('date');
  assert.equal(displayValue(12345.125, number, i18n), '12,345.125');
  assert.equal(displayValue(1e-25, number, i18n), new Intl.NumberFormat('en-US', { maximumSignificantDigits: 21 }).format(1e-25));
  assert.equal(displayValue(false, field('boolean'), i18n), 'No');
  assert.equal(displayValue('yes', field('select', { options: [{ value: 'yes', label: 'Approved' }] }), i18n), 'Approved');
  assert.equal(displayValue('2026-01-02', calendar, i18n), '01/02/2026');
  assert.equal(draftValue(number, 12345.125), '12345.125');
  assert.equal(normalizeInput(number, '12345.125'), 12345.125);
  assert.equal(validateField(number, 12345.125).value, 12345.125);
  assert.equal(normalizeInput(number, '12,345.125'), '12,345.125');
  assert.equal(validateField(number, '12,345.125').ok, false);
  assert.equal(draftValue(calendar, '2026-01-02'), '2026-01-02');
  assert.equal(validateField(calendar, '2026-01-02').value, '2026-01-02');
});

test('客户端bundle拒绝非法语言、语言重复、缺省不在白名单及非平面消息', () => {
  assert.equal(isI18nBundle(bundle()), true);
  assert.equal(isI18nBundle(LEGACY_I18N), true);
  for (const invalid of [null, [], { ...bundle(), format_version: 2 }, { ...bundle(), default_locale: 'fr-FR' },
    { ...bundle(), locales: ['en-US', 'EN-us'], default_locale: 'en-US' }, { ...bundle(), locales: ['zh-CN', 'en_US'] },
    { ...bundle(), messages: { 'zh-CN': {} } }, { ...bundle(), messages: { 'zh-CN': {}, 'en-US': {}, 'de-DE': {} } },
    bundle({ 'en-US': [] }), bundle({ 'en-US': { key: {} } }), bundle({ 'en-US': { '': 'empty key' } }),
  ]) assert.equal(isI18nBundle(invalid), false, JSON.stringify(invalid));
});
