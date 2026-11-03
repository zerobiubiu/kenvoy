import test from 'node:test';
import assert from 'node:assert/strict';
import { createI18n } from '../src/domain/i18n/translate.ts';
import { isI18nBundle } from '../src/domain/i18n/model.ts';
import { localizeApplication } from '../src/domain/i18n/view.ts';
import { validateRecord } from '../src/domain/validation.ts';

const field = (name, kind, rules = {}) => ({ name, label: name, kind, required: false, integer: false, min: null, max: null, max_length: null, unique: false, options: [], ...rules });
const source = () => ({
  format_version: 3, name: '原始应用', generation: 17, role: 'admin', development: false,
  i18n: { format_version: 2, fallback: 'zh-CN',
    languages: [{ code: 'zh-CN', alias: 'cn', display: '简体中文' }, { code: 'en-US', alias: 'en', display: 'English' }],
    messages: {
    'zh-CN': {
      'app.name': '原始应用', 'resources.entries.label': 'entries',
      'resources.entries.fields.name.label': '姓名', 'resources.entries.fields.status.label': '状态',
      'resources.entries.fields.status.options.待确认.label': '待确认', 'pages./客户.title': '客户登记',
      'text:导航': '导航', 'text:入口': '入口', 'text:分组': '分组', 'text:说明': '说明',
      'text:姓名覆盖': '姓名覆盖', 'text:照片': '照片', 'text:业务数据': '不替换业务数据',
      'text:/images/照片.png': '不替换图片地址',
    },
    'en-US': {
      'app.name': 'Entries', 'resources.entries.label': 'Customer entries',
      'resources.entries.fields.name.label': 'Name', 'resources.entries.fields.status.label': 'Status',
      'resources.entries.fields.status.options.待确认.label': 'Pending', 'pages./客户.title': 'Customer registration',
      'text:导航': 'Navigation', 'text:入口': 'Registration', 'text:分组': 'Group', 'text:说明': 'Instructions',
      'text:姓名覆盖': 'Your name', 'text:照片': 'Photo', 'text:业务数据': 'Not a business-data replacement',
      'text:/images/照片.png': 'Not an image URL',
    },
  } },
  resources: [{ name: 'entries', fields: [
    field('name', 'text', { label: '姓名', required: true }),
    field('status', 'select', { label: '状态', default: '待确认', options: [{ value: '待确认', label: '待确认', when: { name: '业务数据' } }] }),
    field('city', 'select', { options_source: { resource: 'cities', value: 'code', label: 'name', permission: 'admin', filters: { parent: 'status' } } }),
  ] }],
  pages: [{ path: '/客户', title: '客户登记', permission: 'anonymous', components: [{ type: 'group', label: '分组', children: [
    { type: 'text', value: '说明' }, { type: 'image', src: '/images/照片.png', alt: '照片', fit: 'contain', width: 80, height: 60 },
    { type: 'form', id: '/客户#form-1', resource: 'entries', mode: 'create', fields: ['name', 'status'], labels: { name: '姓名覆盖' } },
    { type: 'table', resource: 'entries', columns: ['name', 'status'] },
    { type: 'custom', module: 'customer-component', props: { value: { kind: 'literal', value: '业务数据' }, handler: { kind: 'module', value: 'module-id' } }, children: [{ type: 'text', value: '说明' }] },
  ] }] }],
  navigation: [{ label: '导航', items: [{ type: 'group', label: '分组', items: [{ type: 'link', path: '/客户', label: '入口' }] }] }],
  actions: [{ name: 'approve', runtime: 'server', permission: 'admin' }],
  permissions: [{ target: { kind: 'resource', name: 'entries' }, operation: 'create', fields: ['name', 'status'], form: '/客户#form-1' }],
  component_overrides: { text: 'text-override' },
});

test('自动词条翻译完整声明树，仅改变展示字段且视图不可变', () => {
  const original = source();
  assert.equal(isI18nBundle(original.i18n), true);
  const before = structuredClone(original);
  const english = localizeApplication(original, createI18n(original.i18n, 'en-US').t);
  assert.equal(english.name, 'Entries');
  assert.equal(english.resources[0].label, 'Customer entries');
  assert.equal(english.resources[0].fields[0].label, 'Name');
  assert.equal(english.resources[0].fields[1].options[0].label, 'Pending');
  assert.equal(english.pages[0].title, 'Customer registration');
  const group = english.pages[0].components[0];
  assert.equal(group.label, 'Group');
  assert.equal(group.children[0].value, 'Instructions');
  assert.equal(group.children[1].alt, 'Photo');
  assert.equal(group.children[1].src, '/images/照片.png');
  assert.equal(group.children[2].labels.name, 'Your name');
  assert.equal(group.children[4].children[0].value, 'Instructions');
  assert.equal(group.children[4].props.value.value, '业务数据');
  assert.equal(group.children[4].props.handler.value, 'module-id');
  assert.equal(english.navigation[0].label, 'Navigation');
  assert.equal(english.navigation[0].items[0].items[0].label, 'Registration');
  assert.equal(english.resources[0].name, 'entries');
  assert.equal(english.resources[0].fields[1].default, '待确认');
  assert.equal(english.resources[0].fields[1].options[0].value, '待确认');
  assert.deepEqual(english.resources[0].fields[1].options[0].when, { name: '业务数据' });
  assert.deepEqual(english.resources[0].fields[2].options_source, before.resources[0].fields[2].options_source);
  assert.equal(english.pages[0].path, '/客户');
  assert.equal(group.children[2].id, '/客户#form-1');
  assert.equal(english.pages[0].permission, 'anonymous');
  assert.equal(english.role, 'admin');
  assert.equal(english.generation, 17);
  assert.deepEqual(english.permissions, before.permissions);
  assert.deepEqual(english.actions, before.actions);
  assert.deepEqual(english.component_overrides, before.component_overrides);
  assert.deepEqual(original, before);
  assert.throws(() => { english.resources[0].fields[1].options[0].value = 'Pending'; }, TypeError);
  assert.throws(() => { group.children[4].props.value.value = 'changed'; }, TypeError);
});

test('在双语视图间切换仍接受同一规范payload，展示译文不能冒充业务枚举值', () => {
  const original = source();
  const input = { name: '业务数据', status: '待确认' };
  const snapshot = JSON.stringify(input);
  const chinese = localizeApplication(original, createI18n(original.i18n, 'zh-CN').t);
  const english = localizeApplication(original, createI18n(original.i18n, 'en-US').t);
  assert.equal(chinese.name, '原始应用');
  assert.equal(chinese.resources[0].fields[0].label, '姓名');
  assert.equal(chinese.resources[0].label, 'entries');
  assert.deepEqual(validateRecord(chinese.resources[0], input), validateRecord(english.resources[0], input));
  assert.equal(validateRecord(english.resources[0], input).ok, true);
  assert.equal(validateRecord(english.resources[0], { ...input, status: 'Pending' }).ok, false);
  assert.equal(JSON.stringify(input), snapshot);
});

test('metadata-only词典不强制业务自动键，所有语言都保留DSL源码默认展示', () => {
  const original = source();
  original.i18n.messages = { 'zh-CN': {}, 'en-US': {} };
  assert.equal(isI18nBundle(original.i18n), true);
  for (const { code } of original.i18n.languages) {
    const localized = localizeApplication(original, createI18n(original.i18n, code).t);
    assert.equal(localized.name, '原始应用');
    assert.equal(localized.resources[0].fields[0].label, '姓名');
    assert.equal(localized.resources[0].fields[1].options[0].label, '待确认');
    assert.equal(localized.pages[0].title, '客户登记');
    assert.equal(localized.pages[0].components[0].children[0].value, '说明');
    assert.equal(localized.pages[0].components[0].children[2].labels.name, '姓名覆盖');
    assert.equal(localized.navigation[0].items[0].items[0].label, '入口');
  }
});
