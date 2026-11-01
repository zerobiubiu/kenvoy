import { Component, useEffect, useId, useLayoutEffect, useRef, useState } from 'react';
import type { ComponentType, ErrorInfo, FormEvent, ReactNode } from 'react';
import { useSearchParams } from 'react-router';
import { components, expressions, functions } from 'virtual:kenvoy-registry';
import { ApiError } from './api';
import { useRuntime } from './context';
export { RuntimeContext, useRuntime } from './context';
export type { RuntimeState } from './context';
import { FieldContext, FormContext } from './bindings';
import type { FieldBinding, FormBinding, GroupBinding, PageBinding, TableBinding } from './bindings';
import { dependencyValues, dependentFields, dependenciesReady, immutable, normalizeBindingValue, optionDependencies, staticOptions } from './options';
import { Notice, Loading, NativeDialog } from './ui';
import { normalizeInput, validateField } from './validation';
import type {
  ComponentDefinition, FieldDefinition, JsonScalar, JsonValue, SelectOption,
  PageDefinition, RecordData, ResourceDefinition, SystemComponentName,
} from './types';

export function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : '暂时无法完成操作，请稍后再试。';
}

export function RequestError({ error, retry }: { error: unknown; retry?: () => void }) {
  const uncertain = error instanceof ApiError && error.outcomeUnknown;
  return <Notice kind={uncertain ? 'warning' : 'error'} title={uncertain ? '操作结果待核对' : '操作未完成'} role="alert">
    <p>{uncertain ? '网络响应中断，无法确认提交结果。请联系管理员核对，避免重复提交。' : errorMessage(error)}</p>
    {error instanceof ApiError && error.request_id && <p className="kv-hint">核对编号：{error.request_id}</p>}
    {retry && !uncertain && <button type="button" className="kv-button" onClick={retry}>重新加载</button>}
  </Notice>;
}

function draftValue(field: FieldDefinition, value: unknown): string {
  if (value === null || value === undefined) return '';
  if (field.kind === 'boolean') return value === true ? 'true' : 'false';
  return String(value);
}

function OverrideView<T extends { children?: ReactNode }>({ name, model, children }: { name: SystemComponentName; model: T; children: ReactNode }) {
  const { application } = useRuntime();
  const module = application.component_overrides?.[name];
  if (!module) return <>{children}</>;
  const Custom = components[module] as ComponentType<T> | undefined;
  if (!Custom) return <Notice kind="error">系统组件重载尚未准备完成，请刷新页面或联系管理员。</Notice>;
  return <ComponentBoundary key={module}><Custom {...model}>{children}</Custom></ComponentBoundary>;
}

function useOptions(field: FieldDefinition, resource: ResourceDefinition | undefined, values: Readonly<Record<string, JsonScalar>>, validity: Readonly<Record<string, boolean>>, selected: string, formId?: string) {
  const { client, application } = useRuntime();
  const [page, setPage] = useState(1);
  const [search, setSearch] = useState('');
  const [attempt, setAttempt] = useState(0);
  const [data, setData] = useState<SelectOption[]>([]);
  const [resolved, setResolved] = useState<SelectOption | null>(null);
  const [total, setTotal] = useState(0);
  const [busy, setBusy] = useState(Boolean(field.options_source));
  const [failure, setFailure] = useState<unknown>(null);
  const dependencyKey = JSON.stringify(dependencyValues(field, values));
  const ready = field.options_source ? dependenciesReady(field, values) && optionDependencies(field).every(name => validity[name] !== false) : true;
  const configuration = JSON.stringify(field.options_source ?? null);
  const requestKey = JSON.stringify([dependencyKey, configuration, resource?.name, field.name, formId, application.generation, page, search, selected, attempt]);
  const [acceptedKey, setAcceptedKey] = useState('');
  useEffect(() => { setPage(1); setSearch(''); }, [dependencyKey, configuration]);
  useEffect(() => {
    if (!field.options_source || !resource || !ready) {
      setData([]); setResolved(null); setTotal(0); setBusy(false); setFailure(null); return;
    }
    let current = true;
    setBusy(true); setFailure(null);
    const timer = setTimeout(() => {
      client.options(resource.name, field.name, {
        dependencies: JSON.parse(dependencyKey) as Record<string, JsonScalar>, page, page_size: 20,
        search, ...(selected !== '' ? { selected } : {}),
      }, formId).then(result => {
        if (!current) return;
        setData(result.data); setResolved(result.selected); setTotal(result.total); setAcceptedKey(requestKey);
        if (page > 1 && result.data.length === 0 && result.total > 0) setPage(Math.max(1, Math.ceil(result.total / 20)));
      }).catch(error => { if (current) { setData([]); setResolved(null); setFailure(error); setAcceptedKey(requestKey); } })
        .finally(() => { if (current) setBusy(false); });
    }, search ? 200 : 0);
    return () => { current = false; clearTimeout(timer); };
  }, [client, resource?.name, field.name, configuration, ready, dependencyKey, page, search, selected, formId, application.generation, attempt, requestKey]);
  const staticValues = staticOptions(field, values, resource, validity);
  const currentData = acceptedKey === requestKey ? data : [];
  const currentSelected = acceptedKey === requestKey ? resolved : null;
  const available = field.options_source
    ? currentSelected && !currentData.some(option => option.value === currentSelected.value) ? [currentSelected, ...currentData] : currentData
    : staticValues;
  return {
    options: available, loading: Boolean(field.options_source && ready && (busy || acceptedKey !== requestKey)),
    error: failure ? errorMessage(failure) : undefined, ready, page, total: field.options_source ? total : staticValues.length,
    pageSize: 20, search,
    searchOptions: (value: string) => { setSearch(value); setPage(1); },
    loadPage: (value: number) => { if (Number.isSafeInteger(value) && value > 0 && value <= Math.max(1, Math.ceil(total / 20))) setPage(value); },
    reload: () => setAttempt(value => value + 1),
  };
}

export function FieldControl({ field, value, onChange, onValueChange: acceptValue, canonicalValue, canonicalValid, error, disabled = false, resource, values = {}, validity = {}, formId }: {
  field: FieldDefinition; value: string; onChange: (value: string) => void;
  onValueChange?: (value: JsonScalar) => void; canonicalValue?: JsonScalar; canonicalValid?: boolean;
  error?: string; disabled?: boolean; resource?: ResourceDefinition; values?: Readonly<Record<string, JsonScalar>>; validity?: Readonly<Record<string, boolean>>; formId?: string;
}) {
  const id = useId();
  const options = useOptions(field, resource, values, validity, value, formId);
  const [bindingError, setBindingError] = useState('');
  const onDraftChange = (draft: string) => {
    if (disabled) return;
    if (typeof draft !== 'string') { setBindingError('字段草稿必须是字符串'); return; }
    setBindingError(''); onChange(draft);
  };
  const onValueChange = (next: JsonScalar) => {
    if (disabled) return;
    const result = validateField(field, next);
    if (!result.ok) { setBindingError(result.errors.map(item => item.message).join('；')); return; }
    setBindingError('');
    if (acceptValue) acceptValue(result.value as JsonScalar); else onChange(draftValue(field, result.value));
  };
  const normalized = normalizeBindingValue(field, value);
  const valid = canonicalValid ?? normalized.valid;
  const message = error || bindingError || (!valid && value !== '' ? normalized.errors.map(item => item.message).join('；') : undefined);
  const binding: FieldBinding = {
    field: immutable(field), resource: resource ? immutable(resource) : undefined, formId,
    value: valid ? canonicalValue === undefined ? normalized.value : canonicalValue : null, valid,
    draft: value, values: immutable(values), valuesValidity: immutable(validity), error: message,
    disabled: disabled || (field.kind === 'select' && (!options.ready || options.loading)),
    options: immutable(options.options), optionsLoading: options.loading, optionsError: options.error,
    optionsReady: options.ready, optionsPage: options.page, optionsTotal: options.total, optionsPageSize: options.pageSize,
    optionsSearch: options.search, onChange: onValueChange, onDraftChange,
    searchOptions: options.searchOptions, loadOptionsPage: options.loadPage, reloadOptions: options.reload,
  };
  const label = field.label || field.name;
  const hint = message || (field.kind === 'boolean' ? '选择是或否；“否”也是有效值。'
    : field.kind === 'datetime' ? '填写含时区的日期时间，例如 2026-11-01T09:00:00+08:00。'
    : field.kind === 'number' ? [field.integer ? '请输入整数' : '请输入数字', field.min != null ? '最小 ' + field.min : '', field.max != null ? '最大 ' + field.max : ''].filter(Boolean).join('，')
    : field.max_length != null ? '最多 ' + field.max_length + ' 个字符' : '');
  const common = { id, name: field.name, value, required: field.required, disabled: binding.disabled, 'aria-invalid': Boolean(message), 'aria-describedby': id + '-help',
    onChange: (event: { target: { value: string } }) => onDraftChange(event.target.value) };
  const native = <div className="kv-field">
    <label htmlFor={id}>{label}{field.required && <span aria-hidden="true"> *</span>}</label>
    {field.kind === 'select' || field.kind === 'boolean'
      ? <select {...common}>
        {(field.kind === 'boolean' || !options.options.some(option => option.value === '')) && <option value="">{field.kind === 'select' && !options.ready ? '请先选择上级字段' : options.loading ? '正在加载选项…' : '请选择'}</option>}
        {field.kind === 'boolean' ? <><option value="true">是</option><option value="false">否</option></>
          : options.options.map(option => <option key={option.value} value={option.value}>{option.label}</option>)}
        {field.kind === 'select' && value && !options.options.some(option => option.value === value) && <option value={value} disabled>{options.loading ? '正在核对：' : '当前值待重新选择：'}{value}</option>}
      </select>
      : field.kind === 'textarea' ? <textarea {...common} rows={4} />
      : <input {...common} type={field.kind === 'date' ? 'date' : 'text'}
        min={field.kind === 'date' ? '0001-01-01' : undefined} max={field.kind === 'date' ? '9999-12-31' : undefined}
        inputMode={field.kind === 'number' ? field.integer ? 'numeric' : 'decimal' : undefined}
        placeholder={field.kind === 'datetime' ? '2026-11-01T09:00:00+08:00' : undefined} />}
    <span id={id + '-help'} className={message ? 'kv-error' : 'kv-hint'}>{hint}</span>
  </div>;
  return <FieldContext.Provider value={binding}><div className="kv-field-container">
    <OverrideView name={field.kind} model={binding}>{native}</OverrideView>
    {field.kind === 'select' && field.options_source && <div className="kv-options">
      <label className="kv-field"><span>搜索{label}选项</span><input type="search" value={options.search}
        disabled={disabled || !options.ready} maxLength={256} aria-label={label + '选项搜索'}
        onChange={event => options.searchOptions(event.target.value)} /></label>
      <div className="kv-row"><button type="button" className="kv-button" disabled={disabled || options.loading || options.page <= 1} onClick={() => options.loadPage(options.page - 1)}>上一组选项</button>
        <span className="kv-hint" role="status">第 {options.page} 页，共 {options.total} 项</span>
        <button type="button" className="kv-button" disabled={disabled || options.loading || options.page * options.pageSize >= options.total} onClick={() => options.loadPage(options.page + 1)}>下一组选项</button></div>
      {options.loading && <span role="status" className="kv-hint">正在加载{label}选项…</span>}
      {options.error && <Notice kind="error"><p>{options.error}</p><button type="button" className="kv-button" onClick={options.reload} disabled={disabled}>重新加载选项</button></Notice>}
      {options.ready && !options.loading && !options.error && options.total === 0 && <p className="kv-hint">没有匹配选项，请调整搜索或上级字段。</p>}
    </div>}
    {bindingError && !error && <span className="kv-error" role="alert">{bindingError}</span>}
  </div></FieldContext.Provider>;
}

export interface ResourceFormProps {
  resource: ResourceDefinition; fields?: string[]; labels?: Record<string, string>; formId?: string;
  initial?: RecordData; mode?: 'create' | 'edit'; onSaved?: (record: RecordData) => void; onCancel?: () => void;
}
export function ResourceForm({ resource, fields, labels, formId, initial, mode = 'create', onSaved, onCancel }: ResourceFormProps) {
  const { client, refresh } = useRuntime();
  const selected = resource.fields.filter(field => !fields || fields.includes(field.name));
  const makeValues = () => Object.fromEntries(selected.map(field => [field.name, draftValue(field, initial ? initial[field.name] : field.default)]));
  const [values, setValues] = useState<Record<string, string>>(makeValues);
  const [exactValues, setExactValues] = useState<Record<string, JsonScalar>>({});
  const [dirty, setDirty] = useState<Set<string>>(() => new Set());
  const [unchanged, setUnchanged] = useState(false);
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [failure, setFailure] = useState<unknown>(null);
  const [busy, setBusy] = useState(false);
  const [saved, setSaved] = useState<RecordData | null>(null);
  const [uncertain, setUncertain] = useState(false);
  const [copyState, setCopyState] = useState('');
  const form = useRef<HTMLFormElement>(null);
  const feedback = useRef<HTMLDivElement>(null);
  const [focusRequest, setFocusRequest] = useState<{ kind: 'fields'; names: string[] } | { kind: 'feedback' } | null>(null);
  const pending = useRef(false);
  const live = useRef(true);
  useEffect(() => { live.current = true; return () => { live.current = false; }; }, []);
  const normalizedValues = Object.fromEntries(selected.map(field => [field.name, normalizeBindingValue(field,
    Object.hasOwn(exactValues, field.name) ? exactValues[field.name]
      : dirty.has(field.name) ? values[field.name] ?? ''
      : initial ? initial[field.name] ?? null : field.default ?? null)]));
  const jsonValues = Object.fromEntries(Object.entries(normalizedValues).map(([name, result]) => [name, result.value]));
  const validity = Object.fromEntries(Object.entries(normalizedValues).map(([name, result]) => [name, result.valid]));
  const focusFirst = (names: string[]) => setFocusRequest(names.length ? { kind: 'fields', names } : null);
  useLayoutEffect(() => {
    // 等待错误、重置和 busy 状态提交到 DOM，在绘制前消费请求，避免迟到的帧回调抢走下一次输入。
    if (!focusRequest || busy) return;
    if (focusRequest.kind === 'feedback') feedback.current?.focus();
    else for (const name of focusRequest.names) {
      const input = form.current?.elements.namedItem(name);
      if (input instanceof HTMLElement && !input.matches(':disabled')) {
        input.focus();
        if (document.activeElement === input) break;
      }
    }
    setFocusRequest(null);
  }, [focusRequest, busy]);
  const change = (field: FieldDefinition, value: string, exact?: { value: JsonScalar }) => {
    if (busy || saved || uncertain) return;
    if (values[field.name] === value && (!exact || exact.value === jsonValues[field.name]) && !Object.hasOwn(exactValues, field.name)) return;
    const cleared = dependentFields(resource, field.name);
    setValues(current => ({ ...current, ...Object.fromEntries(cleared.map(name => [name, ''])), [field.name]: value }));
    setExactValues(current => {
      const next = { ...current, ...Object.fromEntries(cleared.map(name => [name, null])) };
      if (exact) next[field.name] = exact.value; else delete next[field.name];
      return next;
    });
    setDirty(current => new Set([...current, field.name, ...cleared]));
    setUnchanged(false);
    setErrors(current => ({ ...current, ...Object.fromEntries([field.name, ...cleared].map(name => [name, ''])) }));
  };
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (pending.current || busy || saved || uncertain) return;
    if (mode === 'edit' && !initial) { setFailure(new Error('请先选择需要编辑的记录。')); return; }
    if (mode === 'edit' && dirty.size === 0) { setUnchanged(true); return; }
    const nextErrors: Record<string, string> = {};
    const input: Record<string, JsonValue> = {};
    const complete: Record<string, JsonScalar> = { ...jsonValues };
    for (const field of selected) {
      if (mode === 'edit' && !dirty.has(field.name)) continue;
      const touched = dirty.has(field.name);
      const value = touched ? Object.hasOwn(exactValues, field.name) ? exactValues[field.name] : normalizeInput(field, values[field.name] ?? '') : field.default ?? null;
      const result = validateField(field, value);
      if (result.ok) {
        complete[field.name] = result.value as JsonScalar;
        if (touched) input[field.name] = result.value;
      } else nextErrors[field.name] = result.errors.map(item => item.message).join('；');
    }
    for (const field of selected) {
      const value = complete[field.name];
      if (field.kind === 'select' && value !== null && value !== undefined && !field.options_source
        && !staticOptions(field, complete, resource).some(option => option.value === value)) nextErrors[field.name] = '当前值不属于所选上级字段的选项';
    }
    setErrors(nextErrors); setFailure(null);
    if (Object.keys(nextErrors).length) { focusFirst(Object.keys(nextErrors)); return; }
    pending.current = true; setBusy(true);
    try {
      // 搜索分页不等于完整枚举；提交前独立核对选中值，服务端在事务内仍作最终验证。
      for (const field of selected.filter(item => item.options_source)) {
        const value = complete[field.name];
        if (value === null || value === undefined) continue;
        if (!dependenciesReady(field, complete)) { nextErrors[field.name] = '请先填写选项依赖字段'; continue; }
        const result = await client.options(resource.name, field.name, { dependencies: dependencyValues(field, complete), selected: String(value), page_size: 1 }, formId);
        if (!result.selected) nextErrors[field.name] = '当前选项已失效或不属于所选上级字段，请重新选择';
      }
      if (!live.current) return;
      if (Object.keys(nextErrors).length) { setErrors(nextErrors); focusFirst(Object.keys(nextErrors)); return; }
      const record = mode === 'edit' ? await client.update(resource.name, Number(initial!.id), input) : await client.create(resource.name, input, formId);
      if (!live.current) return;
      setSaved(record); onSaved?.(record); setFocusRequest({ kind: 'feedback' });
    } catch (error) {
      if (!live.current) return;
      setFailure(error);
      if (error instanceof ApiError) {
        setUncertain(error.outcomeUnknown);
        const serverErrors: Record<string, string> = {};
        for (const item of error.fields ?? []) if (item.field) serverErrors[item.field] = item.message;
        setErrors(serverErrors); focusFirst(Object.keys(serverErrors));
      }
    } finally { pending.current = false; if (live.current) setBusy(false); }
  };
  const content = mode === 'edit' && !initial ? <Notice>请先从数据列表选择需要编辑的记录。</Notice> : <>
    <div className="field-grid">{selected.map(field => <FieldControl key={field.name}
      field={{ ...field, label: labels?.[field.name] ?? field.label }} resource={resource} values={jsonValues} validity={validity} formId={formId}
      value={values[field.name] ?? ''} canonicalValue={jsonValues[field.name]} canonicalValid={validity[field.name]} disabled={busy || Boolean(saved) || uncertain} error={errors[field.name]}
      onChange={value => change(field, value)} onValueChange={value => change(field, draftValue(field, value), { value })} />)}</div>
    {failure != null && <RequestError error={failure} />}
    {unchanged && <Notice>内容没有变化，无需保存。</Notice>}
    {failure instanceof ApiError && !uncertain && [401, 403, 409].includes(failure.status) && <button type="button" className="kv-button" onClick={() => { void refresh().then(() => setFailure(null)).catch(setFailure); }}>刷新登录与页面状态</button>}
    {saved && <div ref={feedback} tabIndex={-1} className="form-feedback"><Notice kind="success" role="status" title={mode === 'edit' ? '修改已保存' : '提交成功'}>
      记录编号：{String(saved.id)}。{mode === 'create' ? '您的信息已保存。' : '记录已更新。'}</Notice></div>}
    <div className="kv-row kv-form-actions">
      {!saved && <button type="submit" className="kv-button primary" disabled={busy || uncertain}>{busy ? '正在保存…' : mode === 'edit' ? '保存修改' : '提交信息'}</button>}
      {saved && !onSaved && mode === 'create' && <button type="button" className="kv-button" onClick={() => {
        setValues(makeValues()); setExactValues({}); setDirty(new Set()); setUnchanged(false); setSaved(null); setErrors({}); setCopyState(''); focusFirst(selected.map(field => field.name));
      }}>再登记一条</button>}
      {onCancel && <button type="button" className="kv-button" onClick={onCancel} disabled={busy}>{saved ? '返回列表' : '取消'}</button>}
      {uncertain && <button type="button" className="kv-button" onClick={() => {
        void navigator.clipboard.writeText(JSON.stringify(values, null, 2)).then(() => setCopyState('填写内容已复制，可交给管理员核对。')).catch(() => setCopyState('无法访问剪贴板，请保留当前页面并联系管理员。'));
      }}>复制填写内容</button>}
    </div>
    {copyState && <p role="status" className="kv-hint">{copyState}</p>}
  </>;
  const binding: FormBinding = { resource: immutable(resource), mode, formId, fields: selected.map(field => field.name),
    values: immutable(jsonValues), validity: immutable(validity), errors: immutable(errors), busy, saved: saved ? immutable(saved) : null, submit, children: content };
  return <FormContext.Provider value={binding}><form ref={form} noValidate onSubmit={submit} className="kv-stack" aria-busy={busy}>
    <OverrideView name="form" model={binding}>{content}</OverrideView>
  </form></FormContext.Provider>;
}

export function displayValue(value: unknown, field?: FieldDefinition): string {
  if (value === null || value === undefined || value === '') return '—';
  if (typeof value === 'boolean') return value ? '是' : '否';
  if (field?.kind === 'select') return field.options.find(option => option.value === value)?.label ?? String(value);
  return String(value);
}

export function ResourceTable({ resource, columns, onEdit, onInspect, onDelete, highlightedId, refreshToken = 0 }: {
  resource: ResourceDefinition; columns?: string[]; onEdit?: (record: RecordData) => void; onInspect?: (record: RecordData) => void;
  onDelete?: (record: RecordData) => void; highlightedId?: number; refreshToken?: number;
}) {
  const { client, application } = useRuntime();
  const [items, setItems] = useState<RecordData[]>([]);
  const [total, setTotal] = useState(0);
  const [page, setPage] = useState(0);
  const [pageSize, setPageSize] = useState(10);
  const [search, setSearch] = useState('');
  const [query, setQuery] = useState('');
  const [sort, setSort] = useState('id');
  const [direction, setDirection] = useState<'asc' | 'desc'>('desc');
  const [busy, setBusy] = useState(true);
  const [failure, setFailure] = useState<unknown>(null);
  const [refreshKey, setRefreshKey] = useState(0);
  const [filterOpen, setFilterOpen] = useState(false);
  const [filterField, setFilterField] = useState(resource.fields[0]?.name ?? '');
  const [filterDraft, setFilterDraft] = useState('');
  const [filterError, setFilterError] = useState('');
  const [filters, setFilters] = useState<Record<string, JsonScalar>>({});
  const [actionRecord, setActionRecord] = useState<RecordData | null>(null);
  const tableId = useId();
  const filterDefinition = resource.fields.find(field => field.name === filterField);
  const hasConditions = Boolean(query) || Object.keys(filters).length > 0;
  const selected = resource.fields.filter(field => !columns || columns.includes(field.name));
  const actions = Boolean(onEdit || onInspect || onDelete);
  const cellCount = selected.length + 1 + Number(actions);
  useEffect(() => {
    let current = true;
    setBusy(true); setFailure(null);
    client.list(resource.name, { page: page + 1, page_size: pageSize, search: query, sort, descending: direction === 'desc', filters }).then(result => {
      if (!current) return;
      setItems(result.data); setTotal(result.total);
      if (page > 0 && result.data.length === 0) setPage(Math.max(0, Math.ceil(result.total / pageSize) - 1));
    }).catch(error => { if (current) setFailure(error); }).finally(() => { if (current) setBusy(false); });
    return () => { current = false; };
  }, [client, resource.name, page, pageSize, query, sort, direction, filters, refreshKey, refreshToken, highlightedId, application.generation]);
  const sortColumn = (name: string) => { setDirection(sort === name && direction === 'asc' ? 'desc' : 'asc'); setSort(name); setPage(0); };
  const chooseAction = (action: ((record: RecordData) => void) | undefined) => { if (actionRecord) { const record = actionRecord; setActionRecord(null); action?.(record); } };
  const refresh = () => setRefreshKey(value => value + 1);
  const content = <div className="kv-stack">
    <form className="kv-row kv-table-toolbar" onSubmit={event => { event.preventDefault(); setQuery(search); setPage(0); }}>
      <label className="kv-field kv-search"><span>搜索记录</span><input value={search} maxLength={256} onChange={event => setSearch(event.target.value)} /></label>
      <button type="submit" className="kv-button" disabled={busy}>搜索</button>
      {query && <button type="button" className="kv-button" onClick={() => { setSearch(''); setQuery(''); setPage(0); }}>清除搜索</button>}
      <button type="button" className="kv-button" onClick={() => setFilterOpen(value => !value)} aria-expanded={filterOpen}>筛选</button>
      <button type="button" className="kv-button" onClick={refresh} disabled={busy}>刷新列表</button>
    </form>
    {filterOpen && <div className="kv-panel kv-stack"><div className="kv-row">
      <label className="kv-field"><span>筛选字段</span><select aria-label="筛选字段" value={filterField} onChange={event => { setFilterField(event.target.value); setFilterDraft(''); setFilterError(''); }}>
        {resource.fields.map(field => <option key={field.name} value={field.name}>{field.label || field.name}</option>)}</select></label>
      {filterDefinition && <FieldControl field={{ ...filterDefinition, label: '筛选值' }} resource={resource} values={filters} value={filterDraft} onChange={setFilterDraft} error={filterError} />}
    </div><div className="kv-row">
      <button type="button" className="kv-button" disabled={busy} onClick={() => {
        if (!filterDefinition) return;
        const result = validateField(filterDefinition, normalizeInput(filterDefinition, filterDraft));
        if (!result.ok) { setFilterError(result.errors.map(error => error.message).join('；')); return; }
        if (typeof result.value === 'object' && result.value !== null) return;
        setFilterError(''); setFilters(current => ({ ...current, [filterField]: result.value as JsonScalar })); setPage(0);
      }}>应用筛选</button>
      <button type="button" className="kv-button" onClick={() => { setFilters({}); setFilterDraft(''); setFilterError(''); setPage(0); }}>清除筛选</button>
    </div></div>}
    {Object.keys(filters).length > 0 && <p className="kv-hint" role="status">当前筛选：{Object.entries(filters).map(([name, value]) => (resource.fields.find(field => field.name === name)?.label || name) + ' = ' + displayValue(value, resource.fields.find(field => field.name === name))).join('，')}</p>}
    {failure != null && <RequestError error={failure} retry={refresh} />}
    <p id={tableId + '-scroll-hint'} className="kv-hint mobile-only">左右滑动查看完整字段{actions ? '，右侧可管理记录。' : '。'}</p>
    <div tabIndex={0} role="region" aria-label={resource.name + ' 数据表，可左右滚动'} aria-describedby={tableId + '-scroll-hint'} className="kv-table-scroll">
      <table className="kv-table" aria-label={resource.name + ' 数据列表'} aria-busy={busy}>
        <thead><tr>{[{ name: 'id', label: '编号' }, ...selected].map(field => <th key={field.name} aria-sort={sort === field.name ? direction === 'asc' ? 'ascending' : 'descending' : undefined}>
          <button type="button" className="kv-sort" disabled={busy} onClick={() => sortColumn(field.name)}>{field.label || field.name}{sort === field.name && <svg aria-hidden="true" width="18" height="18" viewBox="0 0 18 18" fill="none" stroke="currentColor" strokeWidth="1.5"><path d={direction === 'asc' ? 'M9 14V4M5 8l4-4 4 4' : 'M9 4v10M5 10l4 4 4-4'} /></svg>}</button></th>)}
          {actions && <th className="kv-actions-cell">操作</th>}</tr></thead>
        <tbody>{busy ? <tr><td colSpan={cellCount}><Loading label="正在加载记录" /></td></tr>
          : failure ? <tr><td colSpan={cellCount} className="kv-empty">列表暂时无法加载，请查看上方提示。</td></tr>
          : items.length === 0 ? <tr><td colSpan={cellCount} className="kv-empty"><strong>{hasConditions ? '没有找到匹配的记录' : '还没有记录'}</strong><p>{hasConditions ? '试试其他关键词或筛选条件，或清除查询条件查看全部记录。' : '提交表单后，已保存的数据会显示在这里。'}</p></td></tr>
          : items.map(record => <tr key={record.id} className={record.id === highlightedId ? 'row-updated' : undefined}>
            <td className="data-cell">{record.id}</td>{selected.map(field => <td key={field.name} className="data-cell">{displayValue(record[field.name], field)}</td>)}
            {actions && <td className="kv-actions-cell"><div className="kv-row desktop-only">
              {onInspect && <button type="button" className="kv-button" onClick={() => onInspect(record)} aria-label={'查看记录 ' + record.id}>详情</button>}
              {onEdit && <button type="button" className="kv-button" onClick={() => onEdit(record)} aria-label={'编辑记录 ' + record.id}>编辑</button>}
              {onDelete && <button type="button" className="kv-button danger" onClick={() => onDelete(record)} aria-label={'删除记录 ' + record.id}>删除</button>}
            </div><button type="button" className="kv-button mobile-only" aria-label={'管理记录 ' + record.id} aria-haspopup="dialog" onClick={() => setActionRecord(record)}>管理</button></td>}
          </tr>)}</tbody>
      </table>
    </div>
    <NativeDialog open={Boolean(actionRecord)} title={'管理记录 ' + (actionRecord?.id ?? '')} onClose={() => setActionRecord(null)}>
      <div role="menu" className="kv-stack" aria-label="记录操作">
        {onInspect && <button type="button" role="menuitem" className="kv-button" aria-label={'查看记录 ' + actionRecord?.id} onClick={() => chooseAction(onInspect)}>详情</button>}
        {onEdit && <button type="button" role="menuitem" className="kv-button" aria-label={'编辑记录 ' + actionRecord?.id} onClick={() => chooseAction(onEdit)}>编辑</button>}
        {onDelete && <button type="button" role="menuitem" className="kv-button danger" aria-label={'删除记录 ' + actionRecord?.id} onClick={() => chooseAction(onDelete)}>删除</button>}
      </div>
    </NativeDialog>
    <div className="kv-pagination">
      <label>每页 <select aria-label="每页" value={pageSize} onChange={event => { setPageSize(Number(event.target.value)); setPage(0); }}>{[10, 20, 50].map(size => <option key={size} value={size}>{size}</option>)}</select></label>
      <span>{total ? page * pageSize + 1 : 0}–{Math.min((page + 1) * pageSize, total)} 条，共 {total} 条</span>
      <button type="button" className="kv-button" aria-label="上一页" disabled={busy || page === 0} onClick={() => setPage(value => value - 1)}>上一页</button>
      <button type="button" className="kv-button" aria-label="下一页" disabled={busy || (page + 1) * pageSize >= total} onClick={() => setPage(value => value + 1)}>下一页</button>
    </div>
  </div>;
  const binding: TableBinding = { resource: immutable(resource), records: immutable(items), columns: selected.map(field => field.name), busy, page: page + 1, pageSize, total, refresh, children: content };
  return <OverrideView name="table" model={binding}>{content}</OverrideView>;
}

class ComponentBoundary extends Component<{ children: ReactNode }, { failed: boolean }> {
  state = { failed: false };
  static getDerivedStateFromError() { return { failed: true }; }
  componentDidCatch(_error: Error, _info: ErrorInfo) { /* 用户组件异常只影响当前区域。 */ }
  render() { return this.state.failed ? <Notice kind="error">此页面组件暂时无法显示，请联系应用管理员。</Notice> : this.props.children; }
}

function BoundForm({ node, resource }: { node: Extract<ComponentDefinition, { type: 'form' }>; resource: ResourceDefinition }) {
  const { client } = useRuntime();
  const [search] = useSearchParams();
  const rawId = search.get('id');
  const id = rawId && /^[1-9]\d*$/.test(rawId) && Number.isSafeInteger(Number(rawId)) ? Number(rawId) : null;
  const [record, setRecord] = useState<RecordData | null>(null);
  const [failure, setFailure] = useState<unknown>(null);
  const [busy, setBusy] = useState(node.mode === 'edit' && id !== null);
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    if (node.mode !== 'edit' || id === null) return;
    let current = true;
    setBusy(true); setFailure(null); setRecord(null);
    client.get(resource.name, id).then(result => { if (current) setRecord(result); })
      .catch(error => { if (current) setFailure(error); }).finally(() => { if (current) setBusy(false); });
    return () => { current = false; };
  }, [client, resource.name, node.mode, id, attempt]);
  if (node.mode === 'edit') {
    if (id === null) return <Notice>请从数据列表选择记录，或使用包含有效记录编号的编辑链接。</Notice>;
    if (busy) return <Loading label="正在加载编辑记录" height={260} />;
    if (failure != null) return <RequestError error={failure} retry={() => setAttempt(value => value + 1)} />;
    if (!record) return null;
  }
  return <ResourceForm key={node.id + '-' + (record?.id ?? 'new')} resource={resource} formId={node.id} fields={node.fields} labels={node.labels} mode={node.mode} initial={record ?? undefined} />;
}

function RegisteredComponent({ node }: { node: Extract<ComponentDefinition, { type: 'custom' }> }) {
  const Custom = components[node.module];
  if (!Custom) return <Notice kind="error">页面组件尚未准备完成，请联系应用管理员。</Notice>;
  const props = Object.fromEntries(Object.entries(node.props).map(([name, value]) => {
    if (value.kind === 'literal') return [name, value.value];
    if (Object.hasOwn(expressions, value.value)) return [name, expressions[value.value]()];
    if (Object.hasOwn(functions, value.value)) return [name, functions[value.value]];
    throw new Error('页面组件引用尚未登记');
  }));
  return <Custom {...props}>{node.children.map((child, index) => <RenderNode key={index} node={child} />)}</Custom>;
}

export function RenderNode({ node }: { node: ComponentDefinition }) {
  const { application } = useRuntime();
  if (node.type === 'text') return <p className="kv-text">{node.value}</p>;
  if (node.type === 'group') {
    const content = <section className="kv-stack">{node.label && <h2>{node.label}</h2>}{node.children.map((child, index) => <RenderNode key={index} node={child} />)}</section>;
    const model: GroupBinding = { label: node.label, children: content };
    return <OverrideView name="group" model={model}>{content}</OverrideView>;
  }
  if (node.type === 'custom') return <ComponentBoundary key={node.module}><RegisteredComponent node={node} /></ComponentBoundary>;
  const resource = application.resources.find(item => item.name === node.resource);
  if (!resource) return <Notice kind="error">此页面的数据资源暂不可用，请刷新页面或联系管理员。</Notice>;
  if (node.type === 'form') return <BoundForm resource={resource} node={node} />;
  if (node.type === 'table') return <ResourceTable resource={resource} columns={node.columns} />;
  return null;
}
export function RuntimePage({ page }: { page: PageDefinition }) {
  const hasTable = page.components.some(node => node.type === 'table');
  const content = <div className={'page-content' + (hasTable ? ' kv-page-wide' : '')}><h1>{page.title}</h1>
    <div className="kv-panel kv-page-panel kv-stack">{page.components.map((node, index) => <RenderNode key={page.path + '-' + index} node={node} />)}</div></div>;
  const model: PageBinding = { page: immutable(page), children: content };
  return <OverrideView name="page" model={model}>{content}</OverrideView>;
}
