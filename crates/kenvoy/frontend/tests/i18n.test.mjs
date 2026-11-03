import test from 'node:test';
import assert from 'node:assert/strict';
import { localeDirection, resolveLocale, selectLocale } from '../src/domain/i18n/locale.ts';
import { createFormatters } from '../src/domain/i18n/format.ts';
import { createI18n } from '../src/domain/i18n/translate.ts';
import { LOADING_I18N, isI18nBundle } from '../src/domain/i18n/model.ts';
import { displayValue } from '../src/domain/i18n/display.ts';
import { draftValue, normalizeInput, validateField } from '../src/domain/validation.ts';

const bundle = (messages = {}) => ({
  format_version: 2, fallback: 'zh-CN',
  languages: [{ code: 'zh-CN', alias: 'cn', display: '简体中文' }, { code: 'en-US', alias: 'en', display: 'English (US)' }],
  messages: { 'zh-CN': {}, 'en-US': {}, ...messages },
});
const field = (kind, rules = {}) => ({ name: 'value', label: '值', kind, required: false, integer: false, min: null, max: null, max_length: null, unique: false, options: [], ...rules });

test('设备默认值只按代码精确匹配，不使用别名、基础语言或候选语言顺序', () => {
  const configuration = bundle();
  assert.equal(selectLocale(configuration, 'en-US'), 'en-US');
  assert.equal(selectLocale(configuration, 'EN-us'), 'en-US');
  for (const unsupported of ['en', 'en-AU', 'en-GB', 'en_US', 'de-DE', '', null, undefined, ['en-US']]) {
    assert.equal(selectLocale(configuration, unsupported), 'zh-CN');
  }
  assert.equal(selectLocale({ ...configuration, fallback: 'en-US' }, 'zh-TW'), 'en-US');
  assert.equal(selectLocale({ ...configuration, fallback: 'en-US' }, 'cn'), 'en-US');
  assert.equal(selectLocale(LOADING_I18N, 'ko-KR'), 'ko-KR');
  assert.equal(selectLocale(LOADING_I18N, 'ko'), 'zh-CN');
  const hebrew = { ...configuration, languages: [{ code: 'he', display: 'עברית' }, configuration.languages[1]], fallback: 'en-US' };
  assert.equal(selectLocale(hebrew, 'iw'), 'en-US');
});

test('页内选择接受代码或唯一别名，始终返回原始代码而不改变SDK语言清单', () => {
  const configuration = bundle();
  for (const [candidate, expected] of [['EN-us', 'en-US'], ['EN', 'en-US'], ['cn', 'zh-CN'], ['zh-CN', 'zh-CN']]) {
    assert.equal(resolveLocale(candidate, configuration.languages), expected);
    const i18n = createI18n(configuration, candidate);
    assert.equal(i18n.locale, expected);
    assert.deepEqual(i18n.locales, ['zh-CN', 'en-US']);
  }
  for (const candidate of ['en-GB', 'English (US)', '', undefined, null]) {
    assert.equal(resolveLocale(candidate, configuration.languages), undefined);
  }
  assert.equal(resolveLocale('DEUTSCH', [{ code: 'de-DE', alias: 'Deutsch', display: '德语' }]), 'de-DE');
  assert.equal(resolveLocale('한국어', [{ code: 'ko-KR', display: '한국어' }]), undefined);
});

test('语言方向仍按脚本标记处理，不影响业务时区', () => {
  assert.equal(localeDirection('ar-EG'), 'rtl');
  assert.equal(localeDirection('he-IL'), 'rtl');
  assert.equal(localeDirection('az-Arab'), 'rtl');
  assert.equal(localeDirection('az-Latn'), 'ltr');
  assert.equal(localeDirection('ku'), 'ltr');
  assert.equal(localeDirection('en-US'), 'ltr');
});

test('规范代码词典优先于系统文案，缺词保留源码fallback且参数仅插值', () => {
  const configuration = bundle({
    'zh-CN': {
      'custom.greeting': '你好 {name}; {missing}', 'custom.default': '默认 {name}',
      'custom.count.one': '{count} 条', 'custom.count.other': '{count} 条', 'ui.login': '业务登录',
    },
    'en-US': {
      'custom.greeting': 'Hello {name}; {missing}', 'custom.default': 'Default {name}',
      'custom.count.one': '{count} entry', 'custom.count.other': '{count} entries', 'ui.login': 'Sign in',
    },
  });
  assert.equal(isI18nBundle(configuration), true);
  const english = createI18n(configuration, 'en');
  assert.equal(english.t('custom.greeting', { name: '<img src=x onerror=alert(1)>' }), 'Hello <img src=x onerror=alert(1)>; {missing}');
  assert.equal(english.t('custom.default', { name: 'Ada' }), 'Default Ada');
  assert.equal(english.t('custom.unknown', { value: 3 }, 'Fallback {value}'), 'Fallback 3');
  assert.equal(english.t('custom.missing'), 'custom.missing');
  assert.equal(english.t('custom.count', { count: 1 }), '1 entry');
  assert.equal(english.t('custom.count', { count: 2 }), '2 entries');
  assert.equal(english.t('custom.count', { count: 0 }), '0 entries');
  assert.equal(english.t('ui.login'), 'Sign in');
  assert.equal(createI18n(configuration, 'de-DE').locale, 'zh-CN');
  assert.equal(createI18n(LOADING_I18N, 'zh-CN').t('ui.form.submit'), '提交信息');
});

test('Intl.PluralRules支持不止单复数两类，未内置语言使用默认系统文案', () => {
  const variants = ['zero', 'one', 'two', 'few', 'many', 'other'];
  const arabic = {
    format_version: 2, fallback: 'en-US',
    languages: [{ code: 'en-US', display: 'English' }, { code: 'ar-EG', display: 'العربية' }],
    messages: {
      'en-US': Object.fromEntries(variants.map(category => [`items.${category}`, `{count} ${category === 'one' ? 'item' : 'items'}`])),
      'ar-EG': Object.fromEntries(variants.map(category => [`items.${category}`, `${category} {count}`])),
    },
  };
  assert.equal(isI18nBundle(arabic), true);
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

test('启动上下文是真实三语言默认值，不代表旧配置或禁用国际化', () => {
  assert.equal(isI18nBundle(LOADING_I18N), true);
  assert.deepEqual(LOADING_I18N, {
    format_version: 2, fallback: 'zh-CN',
    languages: [
      { code: 'zh-CN', alias: 'cn', display: '简体中文' },
      { code: 'en-US', alias: 'en', display: 'English' },
      { code: 'ko-KR', alias: 'ko', display: '한국어' },
    ],
    messages: { 'zh-CN': {}, 'en-US': {}, 'ko-KR': {} },
  });
});

test('bundle2严拒旧协议、非法配置元数据、别名冲突与不完整代码矩阵', () => {
  const languages = bundle().languages;
  const withLanguages = replacements => ({ ...bundle(), languages: replacements });
  assert.equal(isI18nBundle(bundle()), true);
  for (const invalid of [
    null, [], { ...bundle(), format_version: 1 }, { ...bundle(), format_version: 3 },
    { format_version: 1, default_locale: 'zh-CN', locales: ['zh-CN'], messages: { 'zh-CN': {} } },
    { ...bundle(), default_locale: 'zh-CN' }, { ...bundle(), locales: ['zh-CN', 'en-US'] },
    { ...bundle(), fallback: 'fr-FR' }, { ...bundle(), fallback: 'cn' }, { ...bundle(), fallback: 'ZH-cn' },
    withLanguages([]), withLanguages(['zh-CN', 'en-US']),
    withLanguages([{ code: 'zh-CN', display: '中文' }, { code: 'ZH-cn', display: '重复代码' }]),
    withLanguages([languages[0], { ...languages[1], code: 'en_US' }]),
    withLanguages([{ code: 'zh-CN' }, languages[1]]),
    withLanguages([{ ...languages[0], display: '' }, languages[1]]),
    withLanguages([{ ...languages[0], display: ' \n' }, languages[1]]),
    withLanguages([{ ...languages[0], display: 1 }, languages[1]]),
    withLanguages([{ ...languages[0], alias: '' }, languages[1]]),
    withLanguages([{ ...languages[0], alias: ' \t' }, languages[1]]),
    withLanguages([{ ...languages[0], alias: null }, languages[1]]),
    withLanguages([{ ...languages[0], alias: 'ZH-cn' }, languages[1]]),
    withLanguages([{ ...languages[0], alias: 'EN-us' }, languages[1]]),
    withLanguages([languages[0], { ...languages[1], alias: 'CN' }]),
    withLanguages([{ ...languages[0], alias: 'Ä' }, { ...languages[1], alias: 'ä' }]),
    withLanguages([{ ...languages[0], label: '旧标签字段' }, languages[1]]),
    { ...bundle(), messages: null }, { ...bundle(), messages: [] },
    { ...bundle(), messages: { 'zh-CN': {} } }, { ...bundle(), messages: { 'zh-CN': {}, 'en-US': {}, 'de-DE': {} } },
    { ...bundle(), messages: { cn: {}, en: {} } },
    { ...bundle(), messages: { 'ZH-cn': {}, 'en-US': {} } },
    { ...bundle(), messages: { 'zh-CN': {}, 'en-US': {}, en: {} } },
    bundle({ 'en-US': [] }), bundle({ 'en-US': { key: {} } }),
    bundle({ 'en-US': { missing: 'incomplete' } }),
    bundle({ 'zh-CN': { first: '甲' }, 'en-US': { second: 'other' } }),
    bundle({ 'zh-CN': { key: '值' }, 'en-US': { key: '' } }),
  ]) assert.equal(isI18nBundle(invalid), false, JSON.stringify(invalid));
});

test('元数据display与可选alias原样保留，消息key自由且权限投影可整列移除', () => {
  const configuration = bundle({
    'zh-CN': { '': '空名称词条', custom: '  ', retained: '保留' },
    'en-US': { '': 'Empty-name message', custom: ' ', retained: 'Retained' },
  });
  configuration.languages = [{ code: 'zh-CN', display: '项目自定义中文名称' }, { code: 'en-US', alias: 'english', display: 'Team English' }];
  const before = structuredClone(configuration);
  assert.equal(isI18nBundle(configuration), true);
  assert.deepEqual(configuration, before);
  assert.equal(createI18n(configuration, 'english').t(''), 'Empty-name message');
  for (const messages of Object.values(configuration.messages)) {
    delete messages[''];
    delete messages.custom;
  }
  assert.equal(isI18nBundle(configuration), true);
  for (const messages of Object.values(configuration.messages)) delete messages.retained;
  assert.equal(isI18nBundle(configuration), true);
});

test('显式单语言配置只暴露配置项，不自动补入默认语言或别名', () => {
  const configuration = {
    format_version: 2, fallback: 'fr-FR',
    languages: [{ code: 'fr-FR', display: 'Français personnalisé' }],
    messages: { 'fr-FR': {} },
  };
  assert.equal(isI18nBundle(configuration), true);
  assert.equal(selectLocale(configuration, 'en-US'), 'fr-FR');
  assert.equal(resolveLocale('fr', configuration.languages), undefined);
  const i18n = createI18n(configuration, 'fr-FR');
  assert.deepEqual(i18n.locales, ['fr-FR']);
  assert.equal(i18n.t('business.unknown', undefined, 'Texte source'), 'Texte source');
});
