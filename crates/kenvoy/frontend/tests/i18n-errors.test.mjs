import test from 'node:test';
import assert from 'node:assert/strict';
import { ApiError } from '../src/application/errors.ts';
import { FormUploads } from '../src/application/uploads.ts';
import { ApiClient } from '../src/infrastructure/api.ts';
import { createI18n } from '../src/domain/i18n/translate.ts';
import { LEGACY_I18N } from '../src/domain/i18n/model.ts';
import { translateError } from '../src/domain/i18n/error.ts';
import { validateField } from '../src/domain/validation.ts';

const configuration = { format_version: 1, default_locale: 'zh-CN', locales: ['zh-CN', 'en-US'], messages: { 'zh-CN': {}, 'en-US': {} } };
const english = createI18n(configuration, 'en-US');
const chinese = createI18n(configuration, 'zh-CN');
const field = (kind, rules = {}) => ({ name: 'value', label: '值', kind, required: false, integer: false, min: null, max: null, max_length: null, unique: false, options: [], ...rules });
const json = (value, status = 200) => new Response(JSON.stringify(value), { status, headers: { 'content-type': 'application/json', 'x-request-id': 'reference-1' } });
const session = { user: null, csrf_token: 'a'.repeat(64), expires_at: Math.floor(Date.now() / 1000) + 900, generation: 7 };
const application = () => ({ format_version: 2, i18n: structuredClone(configuration), name: '应用', role: 'anonymous', generation: 7, resources: [], pages: [], actions: [], navigation: [], permissions: [] });

test('规则参数直接驱动错误翻译，无需解析默认中文且切语言不改变校验结果', () => {
  const cases = [
    [field('number', { min: 3 }), 2, 'validation.number.min', { min: 3 }, 'greater than or equal to 3'],
    [field('number', { max: 9 }), 10, 'validation.number.max', { max: 9 }, 'less than or equal to 9'],
    [field('text', { max_length: 2 }), '😀甲乙', 'validation.text.max_length', { max: 2 }, '2 Unicode characters'],
  ];
  for (const [definition, value, key, params, expected] of cases) {
    const result = validateField(definition, value);
    assert.equal(result.ok, false);
    const error = result.errors[0];
    assert.equal(error.message_key, key);
    assert.deepEqual(error.params, params);
    const renamedFallback = { ...error, message: '完全不同的业务默认文本，没有任何限制数字' };
    assert.ok(translateError(renamedFallback, english.t).includes(expected));
    assert.notEqual(translateError(renamedFallback, chinese.t), translateError(renamedFallback, english.t));
    assert.deepEqual(validateField(definition, value), result);
  }
});

test('文件数量和大小错误使用独立机器规则与真实约束参数', () => {
  const definition = field('file', { upload: { accept: [], max_size: 10, max_files: 1 } });
  const file = { id: 'a'.repeat(64), name: 'document.txt', size: 11, content_type: 'text/plain' };
  const size = validateField(definition, [file]).errors[0];
  assert.equal(size.message_key, 'validation.files.max_size');
  assert.deepEqual(size.params, { max: 10 });
  assert.equal(translateError({ ...size, message: '自定义回退' }, english.t), 'Each attachment must be no larger than 10 bytes.');
  const count = validateField(definition, [{ ...file, size: 1 }, { ...file, id: 'b'.repeat(64), size: 1 }]).errors[0];
  assert.equal(count.message_key, 'validation.files.max_files');
  assert.deepEqual(count.params, { max: 1 });
  assert.equal(translateError(count, english.t), 'Use at most 1 attachments.');
});

test('结果未知优先于网络或失败词条，未声明的业务错误保留明确fallback', () => {
  for (const code of ['KV5051', 'KV6001', 'KV6002', 'KV6003']) {
    const error = new ApiError('服务器自定义说明', { code, message_key: 'ui.network.read', outcomeUnknown: true });
    const translated = translateError(error, english.t);
    assert.match(translated, /unknown.*verify.*Do not submit again/s);
    assert.doesNotMatch(translated, /reload|try again|retry/i);
  }
  assert.equal(translateError(new ApiError('Business policy rejected this record.', { code: 'CUSTOM_POLICY' }), english.t), 'Business policy rejected this record.');
  assert.equal(translateError(new ApiError('ignored', { message_key: 'ui.network.read' }), english.t), 'Network connection failed. Check your connection and reload.');
});

test('HTTP错误保留机器键、参数、字段与核对编号，且只发送一次写请求', async () => {
  const fields = [{ code: 'KV4005', field: 'amount', message: '替换过的服务端说明', message_key: 'validation.number.min', params: { min: 3 } }];
  const responses = [json(session), json({ error: { code: 'KV5022', message: '任意默认消息', message_key: 'KV5022', params: { resource: 'entries' }, fields, request_id: 'server-reference' } }, 422)];
  let calls = 0;
  const client = new ApiClient(async () => { calls++; return responses.shift(); });
  await assert.rejects(client.create('entries', { amount: 2 }), error => {
    assert.equal(error.message_key, 'KV5022');
    assert.deepEqual(error.params, { resource: 'entries' });
    assert.equal(error.request_id, 'server-reference');
    assert.equal(error.outcomeUnknown, false);
    assert.deepEqual(error.fields, fields);
    assert.equal(translateError(error.fields[0], english.t), 'The number must be greater than or equal to 3.');
    return true;
  });
  assert.equal(calls, 2);
});

test('上传控制器保留机器错误供切语言展示，结果未知后仍锁定且不重试', async () => {
  let requests = 0;
  const uploads = new FormUploads({
    upload: async () => { requests++; throw new ApiError('默认网络文案', { code: 'KV6001', outcomeUnknown: true }); },
    removeUpload: async () => { throw new Error('没有附件标识时不能发送移除'); },
  }, { resource: 'entries', mode: 'create', read: () => [], accept: () => {}, changed: () => {}, editable: () => true });
  const definition = field('file', { upload: { accept: [], max_size: 100, max_files: 1 } });
  const file = new File(['data'], 'file.txt', { type: 'text/plain' });
  await assert.rejects(uploads.upload(definition, [file, file]), error => error.message_key === 'ui.files.too_many');
  const countError = uploads.state('value').error_message;
  assert.deepEqual(countError.params, { max: 1 });
  assert.match(translateError(countError, english.t), /limit is 1/);
  assert.notEqual(translateError(countError, english.t), translateError(countError, chinese.t));
  assert.equal(requests, 0);
  await assert.rejects(uploads.upload(definition, [file]), error => error.outcomeUnknown);
  assert.equal(requests, 1);
  assert.match(translateError(uploads.state('value').error_message, english.t), /unknown.*Do not submit again/s);
  assert.equal(uploads.blocked, true);
  await assert.rejects(uploads.upload(definition, [file]));
  assert.equal(requests, 1);
  uploads.dispose();
});

test('format2描述严拒无词典、旧版本、无效语言与嵌套词条，不悄悄接收损坏协议', async () => {
  const broken = [
    { ...application(), format_version: 1 }, { ...application(), i18n: undefined },
    { ...application(), i18n: { ...configuration, format_version: 2 } },
    { ...application(), i18n: { ...configuration, default_locale: 'fr-FR' } },
    { ...application(), i18n: { ...configuration, messages: { 'zh-CN': {}, 'en-US': { broken: { nested: 'not flat' } } } } },
  ];
  for (const value of broken) {
    const client = new ApiClient(async () => json(value));
    await assert.rejects(client.getApplication(), error => error.code === 'KV6002' && !error.outcomeUnknown);
  }
  const legacy = { ...application(), i18n: LEGACY_I18N };
  assert.deepEqual(await new ApiClient(async () => json(legacy)).getApplication(), legacy);
});
