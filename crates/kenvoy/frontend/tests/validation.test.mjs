import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { normalizeInput, validateField, validateRecord } from '../src/validation.ts';

const vectors = JSON.parse(await readFile(new URL('../../tests/fixtures/validation.json', import.meta.url), 'utf8'));
const field = (kind = 'text', rules = {}) => ({ name: 'value', label: '值', kind, required: false, integer: false, min: null, max: null, max_length: null, unique: false, options: [], ...rules });

assert.equal(vectors.length, 68, '共享向量数量变化时需同时检查 Rust 和浏览器测试记录');
for (const vector of vectors) {
  test(`共享 Rust 向量：${vector.name}`, () => {
    const rules = { ...vector.rules, options: (vector.rules.options ?? []).map(value => ({ value, label: value })) };
    const result = validateField(field(vector.kind, rules), vector.input);
    assert.equal(result.ok, vector.valid);
    if (vector.valid) assert.deepEqual(result.value, vector.output);
    else assert.equal(result.errors[0].field, 'value');
  });
}

test('Create 的默认值与 null，以及 Patch 缺失语义与 Rust 相同', () => {
  const resource = { name: 'records', fields: [field('text', { name: 'name', required: true, default: '未命名' }), field('number', { name: 'age', integer: true }), field('boolean', { name: 'enabled', default: false })] };
  assert.deepEqual(validateRecord(resource, {}, 'create'), { ok: true, value: { name: '未命名', age: null, enabled: false } });
  assert.deepEqual(validateRecord(resource, {}, 'patch'), { ok: true, value: {} });
  assert.deepEqual(validateRecord(resource, { age: null }, 'patch'), { ok: true, value: { age: null } });
  assert.equal(validateRecord(resource, { name: null }, 'patch').ok, false);
  assert.equal(validateRecord(resource, { id: 1 }, 'patch').errors[0].code, 'KV4011');
  assert.equal(validateRecord(resource, { age: undefined }, 'patch').ok, false);
  assert.equal(validateRecord(resource, [], 'patch').errors[0].code, 'KV4010');
  assert.equal(validateRecord(resource, null, 'create').ok, false);
  assert.equal(validateRecord(resource, new Date(), 'create').ok, false);
  const defaults = { name: 'records', fields: [field('text', { required: true, default: null })] };
  assert.equal(validateRecord(defaults, {}).errors[0].code, 'KV4001');
});

test('DOM 草稿只有显式转换，文本不修剪，日期时间不猜测时区', () => {
  assert.equal(normalizeInput(field('number'), ''), null);
  assert.equal(normalizeInput(field('number'), '25'), 25);
  assert.equal(normalizeInput(field('number'), '-1.25e2'), -125);
  assert.equal(normalizeInput(field('number'), '.5'), .5);
  for (const input of [' ', '0xff', 'NaN', 'Infinity', '12a', '25\n']) assert.equal(normalizeInput(field('number'), input), input);
  assert.equal(normalizeInput(field('number', { integer: true }), '9007199254740991.1'), '9007199254740991.1');
  assert.equal(normalizeInput(field('number', { integer: true }), '0.1e1'), 1);
  assert.equal(normalizeInput(field('number', { integer: true }), '10e-1'), 1);
  assert.equal(normalizeInput(field('number', { integer: true }), '0.000'), 0);
  assert.equal(normalizeInput(field('boolean'), 'false'), false);
  assert.equal(normalizeInput(field('boolean'), ''), null);
  assert.equal(normalizeInput(field('boolean'), '0'), '0');
  assert.equal(normalizeInput(field('text'), ' 文字 '), ' 文字 ');
  assert.equal(normalizeInput(field('text'), ''), '');
  assert.equal(normalizeInput(field('date'), ''), null);
  assert.equal(normalizeInput(field('datetime'), '2026-01-01T09:00'), '2026-01-01T09:00');
  assert.equal(normalizeInput(field('select'), ''), null);
  assert.equal(normalizeInput(field('select', { options: [{ value: '', label: '空串' }] }), ''), '');
});

test('不可经 JSON 表达的数值和孤立代理项拒绝，UTC 一年没有 1900 偏移', () => {
  for (const input of [NaN, Infinity, -Infinity, undefined, 1n]) assert.equal(validateField(field('number'), input).ok, false);
  for (const input of ['\ud800', '\udfff']) assert.equal(validateField(field('text'), input).ok, false);
  assert.equal(validateField(field('text', { max_length: 1 }), '😀').ok, true);
  assert.equal(validateField(field('datetime'), '0001-01-01T00:00:00Z').value, '0001-01-01T00:00:00.000Z');
  assert.equal(validateField(field('datetime'), '2024-01-01T23:59:59.999-23:59').value, '2024-01-02T23:58:59.999Z');
  for (const input of ['2024-01-01T00:00:00+24:00', '2024-01-01T00:00:00+00:60', '2024-01-01T00:00:00.Z', '2024-01-01T00:00:00Z\n', '2024-01-01T00:00:00Z\r']) assert.equal(validateField(field('datetime'), input).ok, false);
  assert.equal(validateField(field('date'), '2024-01-01\n').ok, false);
});

test('对象原型字段不会被当作输入或污染输出', () => {
  const resource = { name: 'records', fields: [field('text', { name: '__proto__' })] };
  const result = validateRecord(resource, JSON.parse('{"__proto__":"值"}'));
  assert.equal(result.ok, true);
  assert.equal(Object.getPrototypeOf(result.value), Object.prototype);
  assert.equal(result.value.__proto__, '值');
});
