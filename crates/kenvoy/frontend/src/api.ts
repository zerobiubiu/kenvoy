import type { ApplicationDescription, DevelopmentStatus, FieldError, JsonValue, ListQuery, ListResult, RecordData, SessionView } from './types.ts';

interface ErrorOptions { status?: number; code?: string; fields?: FieldError[]; request_id?: string; outcomeUnknown?: boolean }
export class ApiError extends Error {
  readonly status: number;
  readonly code: string;
  readonly fields: FieldError[];
  readonly request_id: string | undefined;
  readonly outcomeUnknown: boolean;
  constructor(message: string, options: ErrorOptions = {}) {
    super(message);
    this.name = 'ApiError';
    this.status = options.status ?? 0;
    this.code = options.code ?? 'KV6000';
    this.fields = options.fields ?? [];
    this.request_id = options.request_id;
    this.outcomeUnknown = options.outcomeUnknown ?? false;
  }
}

interface RequestOptions { method?: 'GET' | 'POST' | 'PATCH' | 'DELETE'; body?: unknown; form?: string; write?: boolean }
interface ResponseValue { value: unknown; requestId?: string }
const isObject = (value: unknown): value is Record<string, unknown> => value !== null && typeof value === 'object' && !Array.isArray(value);
const positiveInteger = (value: unknown): value is number => typeof value === 'number' && Number.isSafeInteger(value) && value > 0;
const role = (value: unknown): boolean => value === 'anonymous' || value === 'user' || value === 'admin';
const identifier = (value: string): boolean => /^[a-z][a-z0-9_]*$/.exec(value)?.[0] === value;
const strings = (value: unknown): value is string[] => Array.isArray(value) && value.every(item => typeof item === 'string');
const nullableString = (value: unknown): boolean => value === null || typeof value === 'string';
const finiteOrNull = (value: unknown): boolean => value === null || (typeof value === 'number' && Number.isFinite(value));
function isField(value: unknown): boolean {
  return isObject(value) && typeof value.name === 'string' && typeof value.label === 'string'
    && ['text', 'textarea', 'number', 'boolean', 'date', 'datetime', 'select'].includes(String(value.kind))
    && typeof value.required === 'boolean' && typeof value.integer === 'boolean' && typeof value.unique === 'boolean'
    && finiteOrNull(value.min) && finiteOrNull(value.max)
    && (value.max_length === null || (typeof value.max_length === 'number' && Number.isSafeInteger(value.max_length) && value.max_length >= 0))
    && Array.isArray(value.options) && value.options.every(option => isObject(option) && typeof option.value === 'string' && typeof option.label === 'string')
    && (!Object.prototype.hasOwnProperty.call(value, 'default') || serializable(value.default));
}
function isComponent(value: unknown, depth = 0): boolean {
  if (depth > 64 || !isObject(value)) return false;
  switch (value.type) {
    case 'form': return typeof value.id === 'string' && typeof value.resource === 'string' && (value.mode === 'create' || value.mode === 'edit') && strings(value.fields)
      && isObject(value.labels) && Object.values(value.labels).every(label => typeof label === 'string');
    case 'table': return typeof value.resource === 'string' && strings(value.columns);
    case 'text': return typeof value.value === 'string';
    case 'group': return nullableString(value.label) && Array.isArray(value.children) && value.children.every(child => isComponent(child, depth + 1));
    case 'custom': return typeof value.module === 'string' && isObject(value.props)
      && Object.values(value.props).every(prop => isObject(prop) && (prop.kind === 'literal' ? serializable(prop.value) : prop.kind === 'module' && typeof prop.value === 'string'))
      && Array.isArray(value.children) && value.children.every(child => isComponent(child, depth + 1));
    default: return false;
  }
}
function isNavigation(value: unknown, depth = 0): boolean {
  if (depth > 64 || !isObject(value)) return false;
  if (value.type === 'link') return typeof value.label === 'string' && typeof value.path === 'string';
  return value.type === 'group' && nullableString(value.label) && Array.isArray(value.items) && value.items.every(item => isNavigation(item, depth + 1));
}
function isApplication(value: unknown): value is ApplicationDescription {
  return isObject(value) && value.format_version === 1 && typeof value.name === 'string' && positiveInteger(value.generation) && role(value.role)
    && (!Object.hasOwn(value, 'development') || typeof value.development === 'boolean')
    && Array.isArray(value.pages) && value.pages.every(page => isObject(page) && typeof page.path === 'string' && typeof page.title === 'string' && role(page.permission) && Array.isArray(page.components) && page.components.every(node => isComponent(node)))
    && Array.isArray(value.resources) && value.resources.every(resource => isObject(resource) && typeof resource.name === 'string' && Array.isArray(resource.fields) && resource.fields.every(isField))
    && Array.isArray(value.navigation) && value.navigation.every(nav => isObject(nav) && nullableString(nav.label) && Array.isArray(nav.items) && nav.items.every(item => isNavigation(item)))
    && Array.isArray(value.actions) && value.actions.every(action => isObject(action) && typeof action.name === 'string' && role(action.permission)
      && (action.runtime === 'server' ? !Object.prototype.hasOwnProperty.call(action, 'handler') : (action.runtime === 'client' || action.runtime === 'shared') && typeof action.handler === 'string'))
    && Array.isArray(value.permissions) && value.permissions.every(policy => isObject(policy) && isObject(policy.target) && (policy.target.kind === 'resource' || policy.target.kind === 'action')
      && typeof policy.target.name === 'string' && ['create', 'list', 'read', 'update', 'delete', 'execute'].includes(String(policy.operation)) && strings(policy.fields) && nullableString(policy.form));
}

/** 状态通知不能自行升级写入代际；调用方必须先获取并显示新的应用描述。 */
export function developmentChange(previous: DevelopmentStatus | null, next: DevelopmentStatus, generation: number, reconnected = false): 'none' | 'refresh' | 'reload' {
  if (next.state !== 'ready') return 'none';
  if (previous && next.revision !== previous.revision && next.reload) return 'reload';
  if (next.generation !== generation || (previous && next.revision < previous.revision) || reconnected) return 'refresh';
  return 'none';
}
const pathResource = (resource: string): string => {
  if (!identifier(resource) || resource.startsWith('_kenvoy_')) throw new ApiError('资源名称无效');
  return `/api/${resource}`;
};
function pathRecord(resource: string, id: number): string {
  if (!positiveInteger(id)) throw new ApiError('记录 id 必须为安全正整数');
  return `${pathResource(resource)}/${id}`;
}
function isRecord(value: unknown): value is RecordData {
  return isObject(value) && positiveInteger(value.id) && typeof value.created_at === 'string' && typeof value.updated_at === 'string'
    && Object.values(value).every(item => item === null || typeof item === 'string' || typeof item === 'boolean' || (typeof item === 'number' && Number.isFinite(item)));
}
function isSession(value: unknown): value is SessionView {
  return isObject(value) && typeof value.csrf_token === 'string' && value.csrf_token.length === 64 && /^[a-f0-9]{64}$/.test(value.csrf_token)
    && positiveInteger(value.expires_at) && positiveInteger(value.generation)
    && (value.user === null || (isObject(value.user) && positiveInteger(value.user.id) && typeof value.user.username === 'string' && (value.user.role === 'user' || value.user.role === 'admin')));
}
function protocolError(write: boolean, requestId?: string): ApiError {
  return new ApiError(write ? '服务器响应无法确认，操作结果需要核对，请勿重复提交' : '服务器返回了无法识别的响应', { code: 'KV6002', request_id: requestId, outcomeUnknown: write });
}
function serializable(value: unknown, ancestors = new Set<object>(), depth = 0): boolean {
  if (value === null || typeof value === 'string' || typeof value === 'boolean') return true;
  if (typeof value === 'number') return Number.isFinite(value);
  if (typeof value !== 'object' || depth > 64 || ancestors.has(value)) return false;
  if (!Array.isArray(value) && ![Object.prototype, null].includes(Object.getPrototypeOf(value))) return false;
  if (Object.getOwnPropertySymbols(value).length) return false;
  ancestors.add(value);
  const accepted = Object.getOwnPropertyDescriptors(value);
  const result = Object.values(accepted).every(descriptor => 'value' in descriptor && serializable(descriptor.value, ancestors, depth + 1));
  ancestors.delete(value);
  return result;
}

/** Cookie 仅由浏览器管理，CSRF/代际仅驻留内存；所有写入只发送一次。 */
export class ApiClient {
  #fetch: typeof fetch;
  #session: SessionView | null = null;
  #sessionPending: Promise<SessionView> | null = null;
  #sessionEpoch = 0;
  #authPending: Promise<SessionView> | null = null;
  #generation: number | null = null;
  #timeoutMs: number;
  constructor(fetcher: typeof fetch = globalThis.fetch.bind(globalThis), timeoutMs = 30_000) {
    if (!Number.isInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 30_000) throw new ApiError('请求期限必须在 1 至 30000 毫秒范围内');
    this.#fetch = fetcher;
    this.#timeoutMs = timeoutMs;
  }
  get session(): SessionView | null { return this.#session; }

  getSession(): Promise<SessionView> {
    if (this.#authPending) return this.#authPending;
    return this.#refreshSession();
  }
  #refreshSession(): Promise<SessionView> {
    if (this.#sessionPending) return this.#sessionPending;
    const epoch = this.#sessionEpoch;
    const pending = this.#request('/_kenvoy/session').then(response => {
      if (epoch !== this.#sessionEpoch) {
        if (this.#session) return this.#session;
        throw new ApiError('会话状态已经变化，请重新获取会话', { code: 'KV6004' });
      }
      return this.#acceptSession(response, false);
    });
    this.#sessionPending = pending;
    void pending.finally(() => { if (this.#sessionPending === pending) this.#sessionPending = null; }).catch(() => {});
    return pending;
  }
  async getApplication(): Promise<ApplicationDescription> {
    const { value, requestId } = await this.#request('/_kenvoy/application');
    if (!isApplication(value)) throw protocolError(false, requestId);
    this.#generation = value.generation;
    return value;
  }
  async getDevelopment(): Promise<DevelopmentStatus> {
    const { value, requestId } = await this.#request('/_kenvoy/development');
    if (!isObject(value) || !positiveInteger(value.revision) || !positiveInteger(value.generation)
      || !['ready', 'building', 'error'].includes(String(value.state)) || typeof value.reload !== 'boolean'
      || (Object.hasOwn(value, 'error') && (typeof value.error !== 'string' || value.error.length > 8192))) {
      throw protocolError(false, requestId);
    }
    return value as unknown as DevelopmentStatus;
  }
  async login(username: string, password: string): Promise<SessionView> {
    return this.#changeSession('/_kenvoy/login', { username, password });
  }
  async logout(): Promise<SessionView> { return this.#changeSession('/_kenvoy/logout', {}); }
  #changeSession(path: string, body: unknown): Promise<SessionView> {
    if (this.#authPending) return Promise.reject(new ApiError('登录或退出正在进行，本次请求尚未执行', { code: 'KV6004', status: 409 }));
    const pending = this.#performSessionChange(path, body);
    this.#authPending = pending;
    void pending.finally(() => { if (this.#authPending === pending) this.#authPending = null; }).catch(() => {});
    return pending;
  }
  async #performSessionChange(path: string, body: unknown): Promise<SessionView> {
    try {
      // Cookie 由浏览器写入：仅忽略旧 JSON 仍无法防止旧 GET 的 Set-Cookie 覆盖登录。
      if (this.#sessionPending) await this.#sessionPending;
      if (!this.#session || this.#session.expires_at * 1000 <= Date.now()) await this.#refreshSession();
      const response = await this.#request(path, { method: 'POST', body, write: true });
      this.#sessionEpoch++;
      return this.#acceptSession(response, true);
    } catch (error) {
      if (error instanceof ApiError && (error.outcomeUnknown || (error.status === 401 && error.code !== 'KV5101'))) this.#invalidateSession();
      throw error;
    }
  }
  #invalidateSession(): void { this.#sessionEpoch++; this.#session = null; }
  #acceptSession({ value, requestId }: ResponseValue, write: boolean): SessionView {
    if (!isSession(value)) throw protocolError(write, requestId);
    this.#session = value;
    // 刷新 Session 不能将旧页面描述悄悄升级到新的 Schema 代际。
    if (this.#generation === null) this.#generation = value.generation;
    return value;
  }
  async #ensureSession(): Promise<void> {
    if (this.#authPending) await this.#authPending;
    if (!this.#session || this.#session.expires_at * 1000 <= Date.now()) await this.getSession();
  }
  async list(resource: string, query: ListQuery = {}): Promise<ListResult> {
    const search = new URLSearchParams();
    for (const key of ['page', 'page_size', 'search', 'sort'] as const) {
      const value = query[key];
      if (value !== undefined) search.set(key, String(value));
    }
    if (query.descending !== undefined) search.set('direction', query.descending ? 'desc' : 'asc');
    if (query.filters) for (const [field, value] of Object.entries(query.filters)) {
      if (!identifier(field) || !serializable(value) || (typeof value === 'object' && value !== null)) throw new ApiError('筛选字段或标量值无效');
      search.set(`filter_${field}`, JSON.stringify(value));
    }
    const { value, requestId } = await this.#request(`${pathResource(resource)}${search.size ? `?${search}` : ''}`);
    if (!isObject(value) || !Array.isArray(value.data) || !value.data.every(isRecord) || !positiveInteger(value.page) || !positiveInteger(value.page_size)
      || typeof value.total !== 'number' || !Number.isSafeInteger(value.total) || value.total < 0 || !positiveInteger(value.generation)) throw protocolError(false, requestId);
    return value as unknown as ListResult;
  }
  async get(resource: string, id: number): Promise<RecordData> { return this.#record(await this.#request(pathRecord(resource, id)), false); }
  async create(resource: string, data: Record<string, JsonValue>, form?: string): Promise<RecordData> {
    const path = pathResource(resource);
    await this.#ensureSession();
    return this.#record(await this.#request(path, { method: 'POST', body: data, form, write: true }), true);
  }
  async update(resource: string, id: number, data: Record<string, JsonValue>): Promise<RecordData> {
    const path = pathRecord(resource, id);
    await this.#ensureSession();
    return this.#record(await this.#request(path, { method: 'PATCH', body: data, write: true }), true);
  }
  async delete(resource: string, id: number): Promise<void> {
    const path = pathRecord(resource, id);
    await this.#ensureSession();
    const response = await this.#request(path, { method: 'DELETE', write: true });
    if (response.value !== null) throw protocolError(true, response.requestId);
  }
  async executeAction(name: string, args: JsonValue[] = []): Promise<JsonValue> {
    if (/^[A-Za-z][A-Za-z0-9_]*$/.exec(name)?.[0] !== name || !Array.isArray(args) || args.length > 32) throw new ApiError('动作名称或参数数量无效');
    await this.#ensureSession();
    const response = await this.#request(`/api/actions/${name}`, { method: 'POST', body: { args }, write: true });
    if (!isObject(response.value) || !Object.prototype.hasOwnProperty.call(response.value, 'data') || !serializable(response.value.data)
      || !positiveInteger(response.value.generation)) throw protocolError(true, response.requestId);
    return response.value.data as JsonValue;
  }
  #record({ value, requestId }: ResponseValue, write: boolean): RecordData {
    if (!isObject(value) || !isRecord(value.data)) throw protocolError(write, requestId);
    return value.data;
  }
  async #request(path: string, options: RequestOptions = {}): Promise<ResponseValue> {
    const write = options.write ?? false;
    const headers = new Headers({ accept: 'application/json' });
    if (this.#generation !== null) headers.set('x-kenvoy-generation', String(this.#generation));
    if (write) {
      if (!this.#session) throw new ApiError('请先获取有效会话', { code: 'KV5001', status: 401 });
      headers.set('x-kenvoy-csrf', this.#session.csrf_token);
    }
    if (options.form !== undefined) headers.set('x-kenvoy-form', encodeURIComponent(options.form));
    let body: string | undefined;
    if (options.body !== undefined) {
      if (!isObject(options.body) || !serializable(options.body)) throw new ApiError('请求必须为有效 JSON 对象，不能含非有限数字、undefined 或循环引用');
      body = JSON.stringify(options.body);
      if (new TextEncoder().encode(body).length > 1024 * 1024) throw new ApiError('请求数据超过 1 MiB 限制');
      headers.set('content-type', 'application/json');
    }
    const controller = new AbortController();
    let timer: ReturnType<typeof setTimeout> | undefined;
    try {
      return await Promise.race([
        this.#perform(path, { method: options.method ?? 'GET', headers, body, credentials: 'same-origin', mode: 'same-origin', redirect: 'error', cache: 'no-store', signal: controller.signal }, write, this.#sessionEpoch),
        new Promise<never>((_, reject) => {
          timer = setTimeout(() => {
            reject(new ApiError(write ? '请求超时，操作结果需要核对，请勿重复提交' : '请求超时，请稍后重新加载', { code: 'KV6003', outcomeUnknown: write }));
            controller.abort();
          }, this.#timeoutMs);
        }),
      ]);
    } finally { if (timer !== undefined) clearTimeout(timer); }
  }
  async #perform(path: string, options: RequestInit, write: boolean, epoch: number): Promise<ResponseValue> {
    let response: Response;
    try {
      response = await this.#fetch(path, options);
    } catch {
      throw new ApiError(write ? '网络连接中断，操作结果需要核对，请勿重复提交' : '网络连接失败，请检查连接后重新加载', { code: 'KV6001', outcomeUnknown: write });
    }
    const requestId = response.headers.get('x-request-id') ?? undefined;
    if (response.status === 204 && response.ok) return { value: null, requestId };
    let value: unknown;
    try { value = await response.json(); } catch { throw protocolError(write, requestId); }
    if (options.signal?.aborted) throw protocolError(write, requestId);
    if (!response.ok) {
      const error = isObject(value) && isObject(value.error) ? value.error : null;
      if (!error || typeof error.code !== 'string' || typeof error.message !== 'string') throw protocolError(write, requestId);
      const fields = Array.isArray(error.fields) ? error.fields.filter((item): item is FieldError => isObject(item) && typeof item.code === 'string' && (item.field === null || typeof item.field === 'string') && typeof item.message === 'string') : [];
      const unknown = write && (error.code === 'KV5051' || (response.status >= 500 && !['KV5053', 'KV5153'].includes(error.code)));
      if (response.status === 401 && error.code !== 'KV5101' && epoch === this.#sessionEpoch) this.#invalidateSession();
      throw new ApiError(unknown ? '操作结果需要核对，请联系管理员确认，勿重复提交' : error.message, { code: error.code, status: response.status, fields, request_id: typeof error.request_id === 'string' ? error.request_id : requestId, outcomeUnknown: unknown });
    }
    return { value, requestId };
  }
}

export const api = new ApiClient();
