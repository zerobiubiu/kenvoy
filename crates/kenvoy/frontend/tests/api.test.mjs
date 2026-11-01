import test from 'node:test';
import assert from 'node:assert/strict';
import { ApiClient, developmentChange, fileUrl } from '../src/infrastructure/api.ts';
import { ApiError } from '../src/application/errors.ts';

const session = (generation = 1, user = null, csrf = 'a'.repeat(64)) => ({ user, csrf_token: csrf, expires_at: Math.floor(Date.now() / 1000) + 900, generation });
const record = { id: 1, created_at: '2026-01-01T00:00:00.000Z', updated_at: '2026-01-01T00:00:00.000Z', name: '张三' };
const application = generation => ({ format_version: 1, name: '客户管理', role: 'anonymous', generation, pages: [], resources: [], navigation: [], actions: [], permissions: [] });
const response = (value, status = 200) => new Response(JSON.stringify(value), { status, headers: { 'content-type': 'application/json', 'x-request-id': 'request-test' } });
function harness(responses) {
  const calls = [];
  const client = new ApiClient(async (path, options) => {
    calls.push({ path, ...options });
    assert.equal(options.credentials, 'same-origin');
    assert.equal(options.mode, 'same-origin');
    assert.equal(options.redirect, 'error');
    assert.equal(options.cache, 'no-store');
    assert.equal(options.headers.has('cookie'), false);
    const next = responses.shift();
    if (next instanceof Error) throw next;
    assert.ok(next, `没有为 ${path} 准备响应`);
    return next;
  });
  return { client, calls };
}

test('只读选项 POST 携带当前 CSRF、代际、表单及声明依赖，保留选中值解析', async () => {
  const result = { data: [{ value: 'sh', label: '上海' }], page: 1, page_size: 20, total: 1, generation: 7, selected: { value: 'sh', label: '上海' } };
  const { client, calls } = harness([response(application(7)), response(session(7)), response(result)]);
  await client.getApplication();
  assert.deepEqual(await client.options('entries', 'city', { dependencies: { region: 'east' }, selected: 'sh', search: '上海', page: 1 }, '/登记#form-1'), result);
  assert.equal(calls[2].path, '/api/entries/options/city');
  assert.equal(calls[2].headers.get('x-kenvoy-csrf'), 'a'.repeat(64));
  assert.equal(calls[2].headers.get('x-kenvoy-generation'), '7');
  assert.equal(calls[2].headers.get('x-kenvoy-form'), encodeURIComponent('/登记#form-1'));
  assert.deepEqual(JSON.parse(calls[2].body).dependencies, { region: 'east' });
});

test('选项读取失败可以重新加载，不标记为写入结果未知；非法查询不发请求', async () => {
  for (const failure of [new Error('连接中断'), response({ data: [{}] })]) {
    const { client, calls } = harness([response(session()), failure]);
    await assert.rejects(client.options('entries', 'city', { dependencies: {} }), error => error instanceof ApiError && !error.outcomeUnknown);
    assert.equal(calls.length, 2);
  }
  const { client, calls } = harness([]);
  for (const query of [{ dependencies: { region: {} } }, { dependencies: {}, page_size: 101 }, { dependencies: {}, page: 0 }, { dependencies: {}, search: 'x'.repeat(257) }]) {
    await assert.rejects(client.options('entries', 'city', query), ApiError);
  }
  assert.equal(calls.length, 0);
});

test('应用描述拒绝未识别的系统重载目标及无效选项条件', async () => {
  const bad = application(1); bad.component_overrides = { database: 'component-1' };
  const { client } = harness([response(bad)]);
  await assert.rejects(client.getApplication(), error => error.code === 'KV6002');
});

test('匿名提交携带 CSRF/代际/编码表单标识，真实记录响应剥离 data', async () => {
  const { client, calls } = harness([response(session()), response(application(7)), response({ data: record }, 201)]);
  await client.getSession();
  await client.getApplication();
  assert.deepEqual(await client.create('customers', { name: '张三' }, '/客户#form-1'), record);
  assert.equal(calls.length, 3);
  assert.equal(calls[2].path, '/api/customers');
  assert.equal(calls[2].headers.get('x-kenvoy-csrf'), 'a'.repeat(64));
  assert.equal(calls[2].headers.get('x-kenvoy-generation'), '7');
  assert.equal(calls[2].headers.get('x-kenvoy-form'), encodeURIComponent('/客户#form-1'));
  assert.deepEqual(JSON.parse(calls[2].body), { name: '张三' });
});

test('网络中断或响应丢失的写操作只发送一次，标记结果待核对', async () => {
  for (const failure of [new Error('连接已断开'), new Response('损坏响应', { status: 200 }), response({ error: { code: 'KV5051', message: '操作结果需要核对' } }, 500), response({ wrong: true }, 201)]) {
    const { client, calls } = harness([response(session()), failure]);
    await assert.rejects(client.create('customers', { name: '张三' }), error => error instanceof ApiError && error.outcomeUnknown);
    assert.equal(calls.length, 2);
  }
  const { client } = harness([new Error('读取失败')]);
  await assert.rejects(client.getApplication(), error => error instanceof ApiError && !error.outcomeUnknown);
});

test('服务端字段错误、请求标识与已知未执行过载保留，且没有自动重试', async () => {
  const fields = [{ code: 'KV4001', field: 'name', message: '必填' }];
  const { client, calls } = harness([response(session()), response({ error: { code: 'KV5022', message: '字段校验失败', fields, request_id: 'server-request' } }, 422), response({ error: { code: 'KV5053', message: '未执行' } }, 503)]);
  await assert.rejects(client.create('customers', { name: '' }), error => error instanceof ApiError && error.status === 422 && error.request_id === 'server-request' && !error.outcomeUnknown && error.fields[0].field === 'name');
  await assert.rejects(client.create('customers', { name: '张三' }), error => error.code === 'KV5053' && !error.outcomeUnknown);
  assert.equal(calls.length, 3);
});

test('刷新 Session 不会用新代际替换仍然显示的旧应用描述', async () => {
  const { client, calls } = harness([response(session(1)), response(application(1)), response(session(2)), response({ error: { code: 'KV5010', message: '应用已更新' } }, 409)]);
  await client.getSession(); await client.getApplication(); await client.getSession();
  await assert.rejects(client.create('customers', { name: '张三' }), error => error.code === 'KV5010' && !error.outcomeUnknown);
  assert.equal(calls[3].headers.get('x-kenvoy-generation'), '1');
});

test('登录退出使用旋转后的 CSRF，DELETE 无 body，筛选值按类型序列化', async () => {
  const user = { id: 1, username: 'admin', role: 'admin' };
  const { client, calls } = harness([response(session()), response(session(1, user, 'b'.repeat(64))), response({ data: [record], page: 1, page_size: 20, total: 1, generation: 1 }), new Response(null, { status: 204 }), response(session(1, null, 'c'.repeat(64)))]);
  await client.login('admin', 'password-example');
  assert.equal(client.session.user.role, 'admin');
  const listed = await client.list('customers', { page: 1, page_size: 20, search: '%_\\', descending: true, filters: { age: 25, name: '张三', enabled: false, phone: null } });
  assert.equal(listed.total, 1);
  const query = new URL(calls[2].path, 'https://example.test').searchParams;
  assert.equal(query.get('search'), '%_\\'); assert.equal(query.get('direction'), 'desc');
  assert.equal(query.get('filter_age'), '25'); assert.equal(query.get('filter_name'), '"张三"');
  assert.equal(query.get('filter_enabled'), 'false'); assert.equal(query.get('filter_phone'), 'null');
  await client.delete('customers', 1);
  assert.equal(calls[3].body, undefined);
  assert.equal(calls[3].headers.get('x-kenvoy-csrf'), 'b'.repeat(64));
  await client.logout(); assert.equal(client.session.user, null);
  assert.deepEqual(JSON.parse(calls[4].body), {});
});

test('无效 JSON 不会悄悄变为 null 或漏掉字段，也不发送写请求', async () => {
  const { client, calls } = harness([response(session())]);
  await client.getSession();
  const cyclic = {}; cyclic.self = cyclic;
  for (const input of [{ age: Infinity }, { age: NaN }, { name: undefined }, { number: 1n }, { value: new Date() }, cyclic]) {
    await assert.rejects(client.create('customers', input), error => error instanceof ApiError && !error.outcomeUnknown);
  }
  await assert.rejects(client.get('../_kenvoy_users', 1));
  await assert.rejects(client.get('customers', 9007199254740992));
  assert.equal(calls.length, 1);
});

test('并发获取初始 Session 共享一次请求，不产生相互覆盖的 Cookie', async () => {
  const { client, calls } = harness([response(session())]);
  const [first, second] = await Promise.all([client.getSession(), client.getSession()]);
  assert.deepEqual(first, second); assert.equal(calls.length, 1);
});

test('请求与读取响应体都有期限，超时写入不自动重试且保留未知结果', async () => {
  for (const mode of ['fetch', 'body']) {
    let calls = 0;
    let signal;
    const client = new ApiClient(async (_path, options) => {
      calls++;
      if (calls === 1) return response(session());
      signal = options.signal;
      if (mode === 'fetch') return new Promise(() => {});
      const stuck = response({ data: record });
      stuck.json = () => new Promise(() => {});
      return stuck;
    }, 10);
    await assert.rejects(client.create('customers', { name: '张三' }), error => error instanceof ApiError && error.code === 'KV6003' && error.outcomeUnknown);
    assert.equal(calls, 2);
    assert.equal(signal.aborted, true);
  }
  const client = new ApiClient(async () => new Promise(() => {}), 10);
  await assert.rejects(client.getApplication(), error => error.code === 'KV6003' && !error.outcomeUnknown);
});

test('损坏的嵌套应用描述明确失败，不能作为空页面继续运行', async () => {
  for (const patch of [
    { resources: [{ name: 'customers', fields: null }] },
    { pages: [{ path: '/customers', title: '客户', permission: 'anonymous', components: [{ type: 'group', label: null, children: {} }] }] },
    { pages: [{ path: '/customers', title: '客户', permission: 'anonymous', components: [{ type: 'form', id: 'form1', resource: 'customers', mode: 'create', fields: [], labels: null }] }] },
    { actions: [{ name: 'calculate', runtime: 'server', permission: 'admin', handler: '/private/server.ts' }] },
    { navigation: [{ label: '导航', items: [{ type: 'group', label: '组', items: 1 }] }] },
  ]) {
    const { client } = harness([response({ ...application(1), ...patch })]);
    await assert.rejects(client.getApplication(), error => error instanceof ApiError && error.code === 'KV6002' && !error.outcomeUnknown);
  }
});

test('登录等待旧 Session GET，避免迟到的 Set-Cookie 覆盖旋转后的身份', async () => {
  let resolveOld;
  let calls = 0;
  const admin = { id: 1, username: 'admin', role: 'admin' };
  const client = new ApiClient(async (_path, options) => {
    calls++;
    if (calls === 1) return response(session());
    if (calls === 2) return new Promise(resolve => { resolveOld = resolve; });
    if (calls === 3) return response(session(1, admin, 'b'.repeat(64)));
    assert.equal(options.headers.get('x-kenvoy-csrf'), 'b'.repeat(64));
    return response({ data: record }, 201);
  });
  await client.getSession();
  const old = client.getSession();
  const login = client.login('admin', 'password-example');
  const shared = client.getSession();
  assert.equal(calls, 2, '旧 GET 完成前不能发送登录');
  resolveOld(response(session()));
  await old;
  await login;
  assert.equal((await shared).user.role, 'admin');
  assert.equal(client.session.csrf_token, 'b'.repeat(64));
  await client.create('customers', { name: '张三' });
});

test('并发身份变更在发送前明确拒绝，旧数据请求401不撤销新登录', async () => {
  let resolveList;
  let resolveLogin;
  let calls = 0;
  const admin = { id: 1, username: 'admin', role: 'admin' };
  const client = new ApiClient(async path => {
    calls++;
    if (path === '/_kenvoy/session') return response(session());
    if (path === '/api/customers') return new Promise(resolve => { resolveList = resolve; });
    if (path === '/_kenvoy/login') return new Promise(resolve => { resolveLogin = resolve; });
    throw new Error('不应发送第二个身份请求');
  });
  await client.getSession();
  const list = client.list('customers');
  const login = client.login('admin', 'password-example');
  await assert.rejects(client.logout(), error => error.code === 'KV6004' && !error.outcomeUnknown);
  resolveLogin(response(session(1, admin, 'b'.repeat(64))));
  await login;
  resolveList(response({ error: { code: 'KV5001', message: '请先登录' } }, 401));
  await assert.rejects(list, error => error.status === 401);
  assert.equal(client.session.user.role, 'admin');
  assert.equal(calls, 3);
});

test('服务端动作使用相同安全通道并保持 JSON 返回值，不自动重试', async () => {
  const { client, calls } = harness([response(session()), response({ data: { discount: 0.8 }, generation: 1 })]);
  assert.deepEqual(await client.executeAction('calculateDiscount', [10000]), { discount: 0.8 });
  assert.equal(calls[1].path, '/api/actions/calculateDiscount');
  assert.deepEqual(JSON.parse(calls[1].body), { args: [10000] });
  assert.equal(calls[1].headers.get('x-kenvoy-csrf'), 'a'.repeat(64));
  await assert.rejects(client.executeAction('../secret', []));
  await assert.rejects(client.executeAction('calculateDiscount', Array(33).fill(null)));
  assert.equal(calls.length, 2);
});

test('开发标记仅接受布尔值，读取普通应用不会自动请求开发状态', async () => {
  for (const extra of [{}, { development: false }, { development: true }]) {
    const { client, calls } = harness([response({ ...application(1), ...extra })]);
    assert.deepEqual(await client.getApplication(), { ...application(1), ...extra });
    assert.equal(calls.length, 1);
    assert.equal(calls[0].path, '/_kenvoy/application');
  }
  for (const development of ['true', 1, null]) {
    const { client } = harness([response({ ...application(1), development })]);
    await assert.rejects(client.getApplication(), error => error.code === 'KV6002' && !error.outcomeUnknown);
  }
});

test('开发状态只读并校验协议，通知新代际不会提前升级旧表单写入', async () => {
  const status = { revision: 3, generation: 2, state: 'ready', reload: false };
  const { client, calls } = harness([
    response(session()), response(application(1)), response(status), response({ data: record }, 201),
  ]);
  await client.getSession(); await client.getApplication();
  assert.deepEqual(await client.getDevelopment(), status);
  await client.create('customers', { name: '张三' });
  assert.equal(calls[2].path, '/_kenvoy/development');
  assert.equal(calls[2].method, 'GET');
  assert.equal(calls[3].headers.get('x-kenvoy-generation'), '1');
  for (const patch of [
    { revision: 0 }, { revision: 1.5 }, { generation: Number.MAX_SAFE_INTEGER + 1 },
    { state: 'unknown' }, { reload: 'false' }, { error: null }, { error: 'x'.repeat(8193) },
  ]) {
    const { client } = harness([response({ ...status, ...patch })]);
    await assert.rejects(client.getDevelopment(), error => error.code === 'KV6002' && !error.outcomeUnknown);
  }
});

test('开发更新先建立基线，只对新修订明确要求整页刷新，失败与构建中不刷新', () => {
  const ready = { revision: 5, generation: 2, state: 'ready', reload: false };
  assert.equal(developmentChange(null, { ...ready, reload: true }, 2), 'none');
  assert.equal(developmentChange(null, { ...ready, reload: true }, 1), 'refresh');
  assert.equal(developmentChange(ready, ready, 2), 'none');
  assert.equal(developmentChange(ready, { ...ready, revision: 6, generation: 3 }, 2), 'refresh');
  assert.equal(developmentChange(ready, { ...ready, revision: 6, reload: true }, 2), 'reload');
  assert.equal(developmentChange({ ...ready, reload: true }, { ...ready, reload: true }, 2), 'none');
  for (const state of ['building', 'error']) {
    assert.equal(developmentChange(ready, { ...ready, revision: 6, generation: 3, state, reload: true }, 2), 'none');
  }
  assert.equal(developmentChange(ready, { ...ready, revision: 1, generation: 1 }, 2), 'refresh');
  assert.equal(developmentChange(ready, { ...ready, revision: 1 }, 2), 'refresh');
  assert.equal(developmentChange(ready, ready, 2, true), 'refresh');
});

const uploaded = { id: '1'.repeat(64), name: '登记图.png', content_type: 'image/png', size: 12 };
const uploadField = { name: 'attachments', label: '附件', kind: 'file', required: false, integer: false, min: null, max: null, max_length: null, unique: false, options: [], upload: { accept: ['image/*'], max_size: 10485760, max_files: 5 } };

test('原始二进制上传复用 CSRF、代际、模式与表单，不受 JSON 的 1 MiB 限额影响', async () => {
  const file = new File([new Uint8Array(1024 * 1024 + 1)], '登记图.png', { type: 'image/png' });
  const { client, calls } = harness([response(application(7)), response(session(7)), response({ data: { ...uploaded, size: file.size }, generation: 7 }, 201), new Response(null, { status: 204 })]);
  await client.getApplication();
  assert.deepEqual(await client.upload('entries', 'attachments', file, '/登记#form-1', 'update'), { ...uploaded, size: file.size });
  const call = calls[2];
  assert.equal(call.path, '/_kenvoy/uploads/entries/attachments');
  assert.equal(call.body, file);
  assert.equal(call.headers.get('content-type'), 'image/png');
  assert.equal(call.headers.get('x-kenvoy-filename'), encodeURIComponent('登记图.png'));
  assert.equal(call.headers.get('x-kenvoy-upload-mode'), 'update');
  assert.equal(call.headers.get('x-kenvoy-form'), encodeURIComponent('/登记#form-1'));
  assert.equal(call.headers.get('x-kenvoy-generation'), '7');
  assert.equal(call.headers.get('x-kenvoy-csrf'), 'a'.repeat(64));
  await client.removeUpload(uploaded.id);
  assert.equal(calls[3].path, fileUrl(uploaded));
  assert.equal(calls[3].method, 'DELETE');
  assert.equal(calls[3].body, undefined);
  assert.equal(calls[3].headers.has('content-type'), false);
});

test('附件响应保留真实数组，但不普遍放开任意数组或对象字段', async () => {
  const withFiles = { ...record, attachments: [uploaded, { ...uploaded, id: '2'.repeat(64) }] };
  const { client } = harness([response({ data: withFiles })]);
  assert.deepEqual(await client.get('entries', 1), withFiles);
  for (const attachments of [[{}], ['文件'], { ...uploaded }, [uploaded, uploaded], [{ ...uploaded, extra: '未知字段' }]]) {
    const { client } = harness([response({ data: { ...record, attachments } })]);
    await assert.rejects(client.get('entries', 1), error => error instanceof ApiError && error.code === 'KV6002');
  }
});

test('上传响应丢失、损坏或超时仅发送一次，结果未知且不自动重试', async () => {
  for (const failure of [new Error('连接中断'), response({ data: { ...uploaded, id: '../x' }, generation: 1 }, 201), response({ data: uploaded }, 201)]) {
    const { client, calls } = harness([response(session()), failure]);
    await assert.rejects(client.upload('entries', 'attachments', new File(['图片'], '图片.png', { type: 'image/png' })), error => error instanceof ApiError && error.outcomeUnknown);
    assert.equal(calls.length, 2);
  }
  let calls = 0;
  let signal;
  const client = new ApiClient(async (_path, options) => {
    if (++calls === 1) return response(session());
    signal = options.signal;
    return new Promise(() => {});
  }, 10);
  await assert.rejects(client.upload('entries', 'attachments', new File(['图片'], '图片.png', { type: 'image/png' })), error => error.code === 'KV6003' && error.outcomeUnknown);
  assert.equal(calls, 2);
  assert.equal(signal.aborted, true);
});

test('非法上传目标、附件标识、空文件与过大文件在发送前拒绝', async () => {
  const { client, calls } = harness([]);
  for (const [resource, field, file, mode] of [
    ['../private', 'attachments', new File(['a'], 'a.txt'), 'create'],
    ['entries', '../attachments', new File(['a'], 'a.txt'), 'create'],
    ['entries', 'attachments', new File([], 'a.txt'), 'create'],
    ['entries', 'attachments', new File([new Uint8Array(10485761)], 'a.bin'), 'create'],
    ['entries', 'attachments', new File(['a'], '../a.txt'), 'create'],
    ['entries', 'attachments', new File(['a'], 'a.txt'), 'delete'],
  ]) await assert.rejects(client.upload(resource, field, file, undefined, mode), error => error instanceof ApiError && !error.outcomeUnknown);
  for (const id of ['../secret', 'a'.repeat(63), 'A'.repeat(64), 'a'.repeat(64) + '\\n']) {
    assert.throws(() => fileUrl(id), ApiError);
    await assert.rejects(client.removeUpload(id), ApiError);
  }
  assert.equal(fileUrl(uploaded, true), `/_kenvoy/files/${uploaded.id}?download=1`);
  assert.equal(calls.length, 0);
});

test('公开描述接受文件和图片重载，并拒绝不安全图片来源与缺失文件约束', async () => {
  const value = { ...application(1), component_overrides: { file: 'file-view', image: 'image-view' },
    resources: [{ name: 'entries', fields: [uploadField] }],
    pages: [{ path: '/entries', title: '登记', permission: 'anonymous', components: [{ type: 'image', src: '/images/example.png', alt: '说明', width: 320, height: null, fit: 'contain' }] }] };
  const { client } = harness([response(value)]);
  assert.deepEqual(await client.getApplication(), value);
  for (const src of ['javascript:alert(1)', '//external.test/a.png', 'https://user:pass@example.test/a.png', 'http:////example.test/a.png']) {
    const broken = structuredClone(value); broken.pages[0].components[0].src = src;
    const { client } = harness([response(broken)]);
    await assert.rejects(client.getApplication(), error => error.code === 'KV6002');
  }
  const broken = structuredClone(value); delete broken.resources[0].fields[0].upload;
  const { client: invalid } = harness([response(broken)]);
  await assert.rejects(invalid.getApplication(), error => error.code === 'KV6002');
});
