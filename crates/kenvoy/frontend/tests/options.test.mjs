import test from 'node:test';
import assert from 'node:assert/strict';
import { dependentFields, dependencyValues, dependenciesReady, immutable, normalizeBindingValue, staticOptions } from '../src/domain/options.ts';
import { normalizeInput, validateField } from '../src/domain/validation.ts';

const field = (name, rules = {}) => ({ name, label: name, kind: 'select', required: false, integer: false, min: null, max: null, max_length: null, unique: false, options: [], ...rules });

test('字段绑定的非法草稿不会改变原生值类型或伪装成合法 null', () => {
  for (const [kind, draft, rules] of [
    ['number', 'abc', {}], ['number', '1.2', { integer: true }], ['number', '1e999', {}],
    ['text', '', { required: true }], ['number', '', { required: true }], ['boolean', '0', {}],
    ['date', '2026-02-30', {}], ['datetime', '2026-11-01T09:00:00', {}],
    ['select', { value: 'a' }, { options: [{ value: 'a', label: '甲' }] }],
  ]) {
    const result = normalizeBindingValue(field('input', { kind, ...rules }), draft);
    assert.equal(result.valid, false, kind + ':' + String(draft));
    assert.equal(result.value, null);
    assert.equal(result.errors[0].field, 'input');
    // 输入由调用方保留，helper 的 null 只表示无有效值，不能取代提交时对草稿的再次校验。
    assert.equal(validateField(field('input', { kind, ...rules }), normalizeInput(field('input', { kind, ...rules }), draft)).ok, false);
  }
});

test('字段绑定保留有效数字、false、UTC 时间以及可空文本与空串的差异', () => {
  for (const [kind, draft, expected] of [
    ['number', '12.5', 12.5], ['boolean', 'false', false], ['text', null, null], ['text', '', ''],
    ['date', '2024-02-29', '2024-02-29'], ['datetime', '2026-11-01T09:00:00+08:00', '2026-11-01T01:00:00.000Z'],
  ]) {
    const result = normalizeBindingValue(field('input', { kind }), draft);
    assert.deepEqual(result, { value: expected, valid: true, errors: [] });
  }
});

test('无效父草稿映射为不可用依赖，不会误匹配 when 的合法 null', () => {
  const parent = field('amount', { kind: 'number' });
  const remote = field('remote', { options_source: { filters: { amount: 'amount' } } });
  const child = field('child', { options: [{ value: 'empty', label: '未填写', when: { amount: null } }] });
  const invalid = normalizeBindingValue(parent, 'abc');
  const values = { amount: invalid.value };
  assert.equal(dependenciesReady(remote, values), false);
  assert.deepEqual(staticOptions(child, values, { name: 'entries', fields: [parent, child] }, { amount: invalid.valid }), []);
  assert.equal(staticOptions(child, { amount: null }, { name: 'entries', fields: [parent, child] }, { amount: true }).length, 1);
});

test('静态选项按多个依赖匹配，false 与零不能误作未选择', () => {
  const model = field('city', { options: [
    { value: 'a', label: '甲', when: { region: 'east', active: false, count: 0 } },
    { value: 'b', label: '乙', when: { region: 'west' } },
    { value: 'common', label: '通用' },
  ] });
  const values = { region: 'east', active: false, count: 0, secret: '不提交此字段' };
  assert.equal(dependenciesReady(model, values), true);
  assert.deepEqual(staticOptions(model, values).map(item => item.value), ['a', 'common']);
  assert.deepEqual(dependencyValues(model, values), { region: 'east', active: false, count: 0 });
  assert.equal(dependenciesReady(model, { ...values, region: null }), false);
  assert.equal(dependenciesReady(field('choice', { options_source: { filters: { code: 'parent' } } }), { parent: '' }), true);
});

test('源表筛选映射转换成本资源依赖，父级变更传递失效而不影响无关字段', () => {
  const resource = { name: 'entries', fields: [
    field('country'), field('region', { options_source: { filters: { parent_code: 'country' } } }),
    field('city', { options_source: { filters: { region_code: 'region' } } }), field('note', { kind: 'text' }),
  ] };
  assert.deepEqual(dependentFields(resource, 'country'), ['region', 'city']);
  assert.deepEqual(dependencyValues(resource.fields[1], { country: 'cn', region: 'east' }), { country: 'cn' });
  assert.deepEqual(dependentFields(resource, 'note'), []);
});

test('重载与自定义组件收到独立冻结模型，修改嵌套约束或选项不会污染核心', () => {
  const original = { name: 'entries', fields: [field('city', { options: [{ value: 'a', label: '甲', when: { region: 'east' } }] })] };
  const copy = immutable(original);
  assert.throws(() => { copy.fields[0].kind = 'number'; }, TypeError);
  assert.throws(() => { copy.fields[0].options[0].when.region = 'west'; }, TypeError);
  assert.throws(() => { copy.fields.push(field('extra')); }, TypeError);
  assert.equal(Object.isFrozen(original), false);
  assert.equal(original.fields[0].options[0].when.region, 'east');
});

test('远程选项仍是字符串协议，静态枚举和重载对象输入不能绕过原生校验', () => {
  const remote = field('city', { options_source: { resource: 'cities', value: 'code', label: 'name', filters: {}, permission: 'anonymous' } });
  assert.equal(validateField(remote, 'cn-sh').ok, true);
  for (const value of [{ value: 'cn-sh', label: '上海' }, ['cn-sh'], 1, true, undefined]) assert.equal(validateField(remote, value).ok, false);
  assert.equal(normalizeInput(remote, ''), null);
  assert.equal(validateField(field('city', { options: [{ value: 'a', label: '甲' }] }), 'b').ok, false);
});

test('静态条件使用原生字段的规范值比较日期时间和数字', () => {
  const choice = field('choice', { options: [{ value: 'a', label: '甲', when: { at: '2026-11-01T09:00:00+08:00', amount: 1 } }] });
  const resource = { name: 'entries', fields: [field('at', { kind: 'datetime' }), field('amount', { kind: 'number', integer: true }), choice] };
  assert.equal(staticOptions(choice, { at: '2026-11-01T01:00:00.000Z', amount: 1.0 }, resource).length, 1);
  assert.equal(staticOptions(choice, { at: '2026-11-01T01:00:00', amount: 1 }, resource).length, 0);
});

test('附件数组保留在字段绑定中，不得伪装成选项标量依赖', () => {
  const uploaded = { id: 'a'.repeat(64), name: '图片.png', content_type: 'image/png', size: 12 };
  const definition = field('attachments', { kind: 'file', upload: { accept: [], max_size: 1024, max_files: 5 } });
  assert.deepEqual(normalizeBindingValue(definition, [uploaded]), { value: [uploaded], valid: true, errors: [] });
  const choice = field('choice', { options: [{ value: 'empty', label: '空值', when: { attachments: null } }] });
  const values = { attachments: [uploaded] };
  assert.equal(dependenciesReady(choice, values), false);
  assert.deepEqual(dependencyValues(choice, values), { attachments: null });
  assert.deepEqual(staticOptions(choice, values, { name: 'entries', fields: [definition, choice] }), []);
  assert.deepEqual(staticOptions(choice, { attachments: null }, { name: 'entries', fields: [definition, choice] }), []);
});
