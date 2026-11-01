import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import type { FormEvent } from 'react';
import { ApiError } from '../../application/errors';
import { FormUploads } from '../../application/uploads';
import { canReadFile } from '../../domain/files';
import { dependencyValues, dependentFields, dependenciesReady, immutable, normalizeBindingValue, staticOptions } from '../../domain/options';
import { draftValue, normalizeInput, validateField } from '../../domain/validation';
import type { FieldDefinition, FieldValue, JsonValue, RecordData, ResourceDefinition } from '../../domain/types';
import { FormContext } from '../runtime/bindings';
import type { FormBinding } from '../runtime/bindings';
import { useRuntime } from '../runtime/context';
import { OverrideView } from '../runtime/overrides';
import { errorMessage, RequestError } from '../RequestError';
import { Notice } from '../ui';
import { FieldControl } from './FieldControl';

export interface ResourceFormProps {
  resource: ResourceDefinition; fields?: string[]; labels?: Record<string, string>; formId?: string;
  initial?: RecordData; mode?: 'create' | 'edit'; onSaved?: (record: RecordData) => void; onCancel?: () => void;
}
export function ResourceForm(props: ResourceFormProps) {
  const { application } = useRuntime();
  const scope = JSON.stringify([props.resource.name, props.formId, props.mode ?? 'create', props.initial?.id, props.initial?.updated_at, props.fields, application.generation]);
  return <ResourceFormContent key={scope} {...props} />;
}

function ResourceFormContent({ resource, fields, labels, formId, initial, mode = 'create', onSaved, onCancel }: ResourceFormProps) {
  const { client, refresh, application } = useRuntime();
  const selected = resource.fields.filter(field => !fields || fields.includes(field.name));
  const makeValues = () => Object.fromEntries(selected.map(field => [field.name, draftValue(field, initial ? initial[field.name] : field.default)]));
  const [values, setValues] = useState<Record<string, string>>(makeValues);
  const [exactValues, setExactValues] = useState<Record<string, FieldValue>>({});
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
  const [, setUploadRevision] = useState(0);
  const uploadValues = useRef<Record<string, FieldValue>>({});
  const uploadEditable = useRef(true);
  const controller = useRef<FormUploads | null>(null);
  if (controller.current === null) controller.current = new FormUploads(client, {
    resource: resource.name, formId, mode: mode === 'edit' ? 'update' : 'create',
    read: name => uploadValues.current[name],
    editable: () => uploadEditable.current,
    changed: () => setUploadRevision(revision => revision + 1),
    accept: (name, files) => {
      setValues(current => ({ ...current, [name]: '' }));
      setExactValues(current => ({ ...current, [name]: files }));
      setDirty(current => new Set([...current, name]));
      setUnchanged(false);
      setErrors(current => ({ ...current, [name]: '' }));
    },
  });
  const uploads = controller.current;
  useLayoutEffect(() => { uploads.activate(); return () => uploads.dispose(); }, [uploads]);
  const normalizedValues = Object.fromEntries(selected.map(field => [field.name, normalizeBindingValue(field,
    Object.hasOwn(exactValues, field.name) ? exactValues[field.name]
      : dirty.has(field.name) ? values[field.name] ?? ''
      : initial ? initial[field.name] ?? null : field.default ?? null)]));
  const jsonValues = Object.fromEntries(Object.entries(normalizedValues).map(([name, result]) => [name, result.value]));
  const validity = Object.fromEntries(Object.entries(normalizedValues).map(([name, result]) => [name, result.valid]));
  uploadValues.current = jsonValues;
  uploadEditable.current = !busy && !saved && !uncertain && !pending.current;
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
  const change = (field: FieldDefinition, value: string, exact?: { value: FieldValue }) => {
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
    if (pending.current || busy || saved || uncertain || uploads.blocked) return;
    if (mode === 'edit' && !initial) { setFailure(new Error('请先选择需要编辑的记录。')); return; }
    if (mode === 'edit' && dirty.size === 0) { setUnchanged(true); return; }
    const nextErrors: Record<string, string> = {};
    const input: Record<string, JsonValue> = {};
    const complete: Record<string, FieldValue> = { ...jsonValues };
    for (const field of selected) {
      if (mode === 'edit' && !dirty.has(field.name)) continue;
      const touched = dirty.has(field.name);
      const value = touched ? Object.hasOwn(exactValues, field.name) ? exactValues[field.name] : normalizeInput(field, values[field.name] ?? '') : field.default ?? null;
      const result = validateField(field, value);
      if (result.ok) {
        complete[field.name] = result.value as FieldValue;
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
      uploads.retainForSubmission();
      const record = mode === 'edit' ? await client.update(resource.name, Number(initial!.id), input) : await client.create(resource.name, input, formId);
      uploads.committed();
      if (!live.current) return;
      setSaved(record); onSaved?.(record); setFocusRequest({ kind: 'feedback' });
    } catch (error) {
      uploads.submissionFailed(error instanceof ApiError && error.outcomeUnknown);
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
  const reset = () => {
    uploads.reset();
    setValues(makeValues()); setExactValues({}); setDirty(new Set()); setUnchanged(false); setSaved(null);
    setFailure(null); setUncertain(false); setErrors({}); setCopyState(''); focusFirst(selected.map(field => field.name));
  };
  const content = mode === 'edit' && !initial ? <Notice>请先从数据列表选择需要编辑的记录。</Notice> : <>
    <div className="field-grid">{selected.map(field => <FieldControl key={field.name}
      field={{ ...field, label: labels?.[field.name] ?? field.label }} resource={resource} values={jsonValues} validity={validity} formId={formId}
      value={values[field.name] ?? ''} canonicalValue={jsonValues[field.name]} canonicalValid={validity[field.name]} disabled={busy || Boolean(saved) || uncertain} error={errors[field.name]}
      readable={!saved || canReadFile(application, resource.name, field.name)}
      uploads={field.kind === 'file' ? {
        uploadState: immutable(uploads.state(field.name)),
        upload: files => uploads.upload(field, files),
        removeUpload: file => uploads.remove(field.name, file),
        cancelUpload: () => uploads.cancel(field.name),
      } : undefined}
      onChange={value => change(field, value)} onValueChange={value => {
        if (field.kind === 'file') void uploads.replace(field.name, Array.isArray(value) ? value : null).catch(error => setErrors(current => ({ ...current, [field.name]: errorMessage(error) })));
        else change(field, draftValue(field, value), { value });
      }} />)}</div>
    {failure != null && <RequestError error={failure} />}
    {unchanged && <Notice>内容没有变化，无需保存。</Notice>}
    {failure instanceof ApiError && !uncertain && [401, 403, 409].includes(failure.status) && <button type="button" className="kv-button" onClick={() => { void refresh().then(() => setFailure(null)).catch(setFailure); }}>刷新登录与页面状态</button>}
    {saved && <div ref={feedback} tabIndex={-1} className="form-feedback"><Notice kind="success" role="status" title={mode === 'edit' ? '修改已保存' : '提交成功'}>
      记录编号：{String(saved.id)}。{mode === 'create' ? '您的信息已保存。' : '记录已更新。'}</Notice></div>}
    <div className="kv-row kv-form-actions">
      {!saved && <button type="submit" className="kv-button primary" disabled={busy || uncertain || uploads.blocked}>{busy ? '正在保存…' : uploads.blocked ? '请先完成文件操作' : mode === 'edit' ? '保存修改' : '提交信息'}</button>}
      {saved && !onSaved && mode === 'create' && <button type="button" className="kv-button" onClick={reset}>再登记一条</button>}
      {!saved && !uncertain && !onCancel && uploads.hasWork && <button type="button" className="kv-button" onClick={reset} disabled={busy}>取消本次填写</button>}
      {onCancel && <button type="button" className="kv-button" onClick={() => { uploads.dispose(); onCancel(); }} disabled={busy}>{saved ? '返回列表' : '取消'}</button>}
      {uncertain && <button type="button" className="kv-button" onClick={() => {
        void navigator.clipboard.writeText(JSON.stringify({ ...values, ...exactValues }, null, 2)).then(() => setCopyState('填写内容已复制，可交给管理员核对。')).catch(() => setCopyState('无法访问剪贴板，请保留当前页面并联系管理员。'));
      }}>复制填写内容</button>}
    </div>
    {copyState && <p role="status" className="kv-hint">{copyState}</p>}
  </>;
  const binding: FormBinding = { resource: immutable(resource), mode, formId, fields: selected.map(field => field.name),
    values: immutable(jsonValues), validity: immutable(validity), errors: immutable(errors), busy: busy || uploads.blocked, saved: saved ? immutable(saved) : null, submit, children: content };
  return <FormContext.Provider value={binding}><form ref={form} noValidate onSubmit={submit} className="kv-stack" aria-busy={busy || uploads.blocked}>
    <OverrideView name="form" model={binding}>{content}</OverrideView>
  </form></FormContext.Provider>;
}
