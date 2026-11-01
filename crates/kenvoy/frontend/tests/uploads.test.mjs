import test from 'node:test';
import assert from 'node:assert/strict';
import { ApiError } from '../src/api.ts';
import { FormUploads } from '../src/uploads.ts';
import { canReadFile, isSafeImageSource, isUploadedFiles } from '../src/files.ts';

const attachment = (number, name = `文件${number}.png`) => ({ id: number.toString(16).padStart(64, '0'), name, content_type: 'image/png', size: 4 });
const field = { name: 'attachments', label: '附件', kind: 'file', required: false, integer: false, min: null, max: null, max_length: null, unique: false, options: [], upload: { accept: ['image/*'], max_size: 1024, max_files: 5 } };
const selected = name => new File(['图片'], name, { type: 'image/png' });
function deferred() {
  let resolve, reject;
  const promise = new Promise((accept, fail) => { resolve = accept; reject = fail; });
  return { promise, resolve, reject };
}
function form(transport, initial = []) {
  const values = { attachments: initial };
  const accepted = [];
  const uploads = new FormUploads(transport, { resource: 'entries', formId: '/登记#form-1', mode: 'update',
    read: name => values[name], editable: () => true, changed: () => {},
    accept: (name, files) => { values[name] = files; accepted.push(files); },
  });
  return { uploads, values, accepted };
}

test('多文件按顺序加入并保留初始数组；移除已保存文件不请求暂存删除', async () => {
  const calls = [], deleted = [];
  const original = attachment(1);
  const { uploads, values } = form({ upload: async (...args) => { calls.push(args); return attachment(calls.length + 1, args[2].name); }, removeUpload: async id => { deleted.push(id); } }, [original]);
  await uploads.upload(field, [selected('甲.png'), selected('乙.png')]);
  assert.deepEqual(values.attachments.map(file => file.name), ['文件1.png', '甲.png', '乙.png']);
  assert.equal(uploads.state(field.name).completed, 2);
  assert.equal(uploads.blocked, false);
  await uploads.remove(field.name, original.id);
  assert.deepEqual(deleted, []);
  await uploads.remove(field.name, values.attachments[0]);
  assert.deepEqual(deleted, [attachment(2).id]);
  assert.equal(values.attachments.length, 1);
  uploads.committed(); uploads.dispose();
  assert.deepEqual(deleted, [attachment(2).id], '保存后的卸载不能删除已经关联记录的文件');
});

test('取消上传后停止后续文件，迟到响应只清理暂存，不回填已重置的表单', async () => {
  const old = deferred();
  const calls = [], deleted = [];
  const { uploads, values, accepted } = form({ upload: async (_resource, _field, file) => { calls.push(file.name); return file.name === '旧.png' ? old.promise : attachment(3, file.name); }, removeUpload: async id => { deleted.push(id); } });
  const pending = uploads.upload(field, [selected('旧.png'), selected('不得发送.png')]);
  assert.equal(uploads.blocked, true);
  uploads.cancel(field.name);
  uploads.reset();
  values.attachments = [];
  await uploads.upload(field, [selected('新.png')]);
  old.resolve(attachment(2, '旧.png'));
  await pending;
  assert.deepEqual(calls, ['旧.png', '新.png']);
  assert.deepEqual(values.attachments, [attachment(3, '新.png')]);
  assert.equal(accepted.some(files => files.some(file => file.name === '旧.png')), false);
  assert.deepEqual(deleted, [attachment(2).id]);
  assert.equal(uploads.state(field.name).outcomeUnknown, false);
});

test('上传结果未知阻止提交与重复上传，重置不会重试未知请求', async () => {
  let count = 0;
  const deleted = [];
  const { uploads, values } = form({ upload: async () => { count++; throw new ApiError('结果未知', { outcomeUnknown: true }); }, removeUpload: async id => { deleted.push(id); } });
  await assert.rejects(uploads.upload(field, [selected('图片.png')]), error => error.outcomeUnknown);
  assert.equal(uploads.blocked, true);
  assert.equal(uploads.state(field.name).outcomeUnknown, true);
  await assert.rejects(uploads.upload(field, [selected('图片.png')]));
  assert.equal(count, 1);
  assert.deepEqual(values.attachments, []);
  uploads.reset();
  assert.equal(uploads.blocked, false);
  assert.deepEqual(deleted, [], '响应丢失没有附件标识，只能由服务端到期回收');
});

test('暂存移除结果未知时保留文件并锁定提交，卸载不能自动重试删除', async () => {
  let deletes = 0;
  const { uploads, values } = form({ upload: async () => attachment(2), removeUpload: async () => { deletes++; throw new ApiError('删除结果未知', { outcomeUnknown: true }); } });
  await uploads.upload(field, [selected('图片.png')]);
  await assert.rejects(uploads.remove(field.name, attachment(2).id), error => error.outcomeUnknown);
  assert.deepEqual(values.attachments, [attachment(2)]);
  assert.equal(uploads.blocked, true);
  uploads.dispose();
  assert.equal(deletes, 1);
});

test('提交期间卸载不清理附件，已知失败才补清理；未知结果或成功均保留', async () => {
  for (const outcome of ['known-failure', 'unknown', 'saved']) {
    const deleted = [];
    const { uploads } = form({ upload: async () => attachment(2), removeUpload: async id => { deleted.push(id); } });
    await uploads.upload(field, [selected('图片.png')]);
    uploads.retainForSubmission(); uploads.dispose();
    assert.deepEqual(deleted, []);
    if (outcome === 'saved') uploads.committed();
    else uploads.submissionFailed(outcome === 'unknown');
    assert.deepEqual(deleted, outcome === 'known-failure' ? [attachment(2).id] : []);
  }
});

test('重置前清理的迟到错误不能污染新表单，字段替换同样清理被移除的暂存', async () => {
  const cleanup = deferred();
  const deleted = [];
  let number = 1;
  const { uploads, values } = form({ upload: async () => attachment(++number), removeUpload: async id => { deleted.push(id); if (id === attachment(2).id) return cleanup.promise; } });
  await uploads.upload(field, [selected('旧.png')]);
  uploads.reset(); values.attachments = [];
  await uploads.upload(field, [selected('新.png')]);
  cleanup.reject(new ApiError('旧表单的清理结果未知', { outcomeUnknown: true }));
  await Promise.resolve(); await Promise.resolve();
  assert.equal(uploads.state(field.name).outcomeUnknown, false);
  assert.equal(uploads.state(field.name).error, undefined);
  await uploads.replace(field.name, null);
  assert.equal(values.attachments, null);
  assert.deepEqual(deleted, [attachment(2).id, attachment(3).id]);
});

test('数量、大小、类型与文件名检查在发送前阻止无效整批选择', async () => {
  let count = 0;
  const { uploads } = form({ upload: async () => { count++; return attachment(2); }, removeUpload: async () => {} });
  for (const files of [Array.from({ length: 6 }, (_, index) => selected(`${index}.png`)), [new File([], '空.png', { type: 'image/png' })], [new File([new Uint8Array(1025)], '过大.png', { type: 'image/png' })], [new File(['文本'], '文本.txt', { type: 'text/plain' })], [selected('../路径.png')]]) {
    await assert.rejects(uploads.upload(field, files), ApiError);
    assert.equal(uploads.blocked, false);
  }
  assert.equal(count, 0);
});

test('图片安全来源与服务端一致，附件数组不接受任意对象或稀疏项', () => {
  for (const src of ['/image.png', '/', 'https://example.test/a.png?q=1', 'HTTP://example.test/a.png']) assert.equal(isSafeImageSource(src), true, src);
  for (const src of ['', '//example.test/a.png', '/\\example.test/a.png', 'javascript:alert(1)', 'data:image/png;base64,x', 'blob:https://example.test/id', 'https://user:pass@example.test/a.png', 'http:////example.test/a.png', 'http:example.test/a.png', ' https://example.test/a.png', '/image\n.png']) assert.equal(isSafeImageSource(src), false, src);
  assert.equal(isUploadedFiles([attachment(1), attachment(2)]), true);
  for (const files of [[attachment(1), attachment(1)], [{}], ['字符串'], new Array(1), [{ ...attachment(1), extra: 1 }]]) assert.equal(isUploadedFiles(files), false);
});

test('匿名提交后没有字段级 Read 权限时不显示已保存文件请求入口', () => {
  const policy = { target: { kind: 'resource', name: 'entries' }, operation: 'create', fields: ['attachments'], form: null };
  const application = { permissions: [policy] };
  assert.equal(canReadFile(application, 'entries', 'attachments'), false);
  application.permissions.push({ ...policy, operation: 'read', fields: ['another'] });
  assert.equal(canReadFile(application, 'entries', 'attachments'), false);
  application.permissions.push({ ...policy, operation: 'read', form: '/登记#form-1' });
  assert.equal(canReadFile(application, 'entries', 'attachments'), false);
  application.permissions.push({ ...policy, operation: 'read' });
  assert.equal(canReadFile(application, 'entries', 'attachments'), true);
});
