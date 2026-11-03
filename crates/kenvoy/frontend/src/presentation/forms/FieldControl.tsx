import { useEffect, useId, useState } from 'react';
import { ApiError } from '../../application/errors';
import { EMPTY_UPLOAD_STATE } from '../../application/uploads';
import type { FileUploadBinding } from '../../application/uploads';
import { dependencyValues, dependenciesReady, immutable, normalizeBindingValue, optionDependencies, staticOptions } from '../../domain/options';
import { draftValue, validateField } from '../../domain/validation';
import type { FieldDefinition, FieldValue, JsonScalar, ResourceDefinition, SelectOption } from '../../domain/types';
import { FieldContext } from '../runtime/bindings';
import type { FieldBinding } from '../runtime/bindings';
import { useRuntime } from '../runtime/context';
import { OverrideView } from '../runtime/overrides';
import { errorMessage } from '../RequestError';
import { Notice } from '../ui';
import { FileField } from './FileField';
import { useI18n } from '../i18n';
import { translateError } from '../../domain/i18n/error';
import type { LocalizedMessage } from '../../domain/i18n/error';
import { constraintHint } from '../../domain/i18n/display';

function useOptions(field: FieldDefinition, resource: ResourceDefinition | undefined, values: Readonly<Record<string, FieldValue>>, validity: Readonly<Record<string, boolean>>, selected: string, formId?: string) {
  const { t } = useI18n();
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
    error: failure ? errorMessage(failure, t) : undefined, ready, page, total: field.options_source ? total : staticValues.length,
    pageSize: 20, search,
    searchOptions: (value: string) => { setSearch(value); setPage(1); },
    loadPage: (value: number) => { if (Number.isSafeInteger(value) && value > 0 && value <= Math.max(1, Math.ceil(total / 20))) setPage(value); },
    reload: () => setAttempt(value => value + 1),
  };
}

export function FieldControl({ field, value, onChange, onValueChange: acceptValue, canonicalValue, canonicalValid, error, disabled = false, resource, values = {}, validity = {}, formId, uploads, readable = true }: {
  field: FieldDefinition; value: string; onChange: (value: string) => void;
  onValueChange?: (value: FieldValue) => void; canonicalValue?: FieldValue; canonicalValid?: boolean;
  error?: string; disabled?: boolean; resource?: ResourceDefinition; values?: Readonly<Record<string, FieldValue>>; validity?: Readonly<Record<string, boolean>>; formId?: string;
  uploads?: FileUploadBinding; readable?: boolean;
}) {
  const i18n = useI18n();
  const { t, formatNumber } = i18n;
  const id = useId();
  const options = useOptions(field, resource, values, validity, value, formId);
  const [bindingErrors, setBindingErrors] = useState<LocalizedMessage[]>([]);
  const bindingError = bindingErrors.map(item => translateError(item, t)).join(t('ui.separator'));
  const onDraftChange = (draft: string) => {
    if (disabled) return;
    if (typeof draft !== 'string') { setBindingErrors([{ message: '', message_key: 'ui.field.draft' }]); return; }
    if (field.kind === 'file') { setBindingErrors([{ message: '', message_key: 'ui.field.file_draft' }]); return; }
    setBindingErrors([]); onChange(draft);
  };
  const onValueChange = (next: FieldValue) => {
    if (disabled) return;
    const result = validateField(field, next);
    if (!result.ok) { setBindingErrors(result.errors); return; }
    setBindingErrors([]);
    if (acceptValue) acceptValue(immutable(result.value as FieldValue)); else onChange(draftValue(field, result.value));
  };
  const normalized = normalizeBindingValue(field, field.kind === 'file' ? canonicalValue ?? null : value);
  const valid = canonicalValid ?? normalized.valid;
  const message = error || bindingError || (!valid && value !== '' ? normalized.errors.map(item => translateError(item, t)).join(t('ui.separator')) : undefined);
  const uploadState = uploads?.uploadState ?? EMPTY_UPLOAD_STATE;
  const binding: FieldBinding = {
    field: immutable(field), resource: resource ? immutable(resource) : undefined, formId,
    value: immutable(valid ? canonicalValue === undefined ? normalized.value : canonicalValue : null), valid,
    draft: value, values: immutable(values), valuesValidity: immutable(validity), error: message,
    disabled: disabled || (field.kind === 'select' && (!options.ready || options.loading)),
    options: immutable(options.options), optionsLoading: options.loading, optionsError: options.error,
    optionsReady: options.ready, optionsPage: options.page, optionsTotal: options.total, optionsPageSize: options.pageSize,
    optionsSearch: options.search, onChange: onValueChange, onDraftChange,
    searchOptions: options.searchOptions, loadOptionsPage: options.loadPage, reloadOptions: options.reload,
    uploadState: uploadState.error_message ? immutable({ ...uploadState, error: translateError(uploadState.error_message, t) }) : uploadState,
    upload: uploads?.upload ?? (() => Promise.reject(new ApiError('此字段不在可上传的资源表单内', { message_key: 'ui.field.no_upload' }))),
    removeUpload: uploads?.removeUpload ?? (() => Promise.reject(new ApiError('此字段不在可上传的资源表单内', { message_key: 'ui.field.no_upload' }))),
    cancelUpload: uploads?.cancelUpload ?? (() => { throw new ApiError('此字段不在可上传的资源表单内', { message_key: 'ui.field.no_upload' }); }),
  };
  const label = field.label || field.name;
  const hint = message || constraintHint(field, i18n);
  const common = { id, name: field.name, value, required: field.required, disabled: binding.disabled, 'aria-invalid': Boolean(message), 'aria-describedby': id + '-help',
    onChange: (event: { target: { value: string } }) => onDraftChange(event.target.value) };
  const native = field.kind === 'file' ? <FileField binding={binding} id={id} readable={readable} /> : <div className="kv-field">
    <label htmlFor={id}>{label}{field.required && <span aria-hidden="true"> *</span>}</label>
    {field.kind === 'select' || field.kind === 'boolean'
      ? <select {...common}>
        {(field.kind === 'boolean' || !options.options.some(option => option.value === '')) && <option value="">{t(field.kind === 'select' && !options.ready ? 'ui.options.parent' : options.loading ? 'ui.options.loading' : 'ui.options.choose')}</option>}
        {field.kind === 'boolean' ? <><option value="true">{t('ui.yes')}</option><option value="false">{t('ui.no')}</option></>
          : options.options.map(option => <option key={option.value} value={option.value}>{option.label}</option>)}
        {field.kind === 'select' && value && !options.options.some(option => option.value === value) && <option value={value} disabled>{t(options.loading ? 'ui.options.checking' : 'ui.options.invalid', { value })}</option>}
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
      <label className="kv-field"><span>{t('ui.options.search', { label })}</span><input type="search" value={options.search}
        disabled={disabled || !options.ready} maxLength={256} aria-label={t('ui.options.search_label', { label })}
        onChange={event => options.searchOptions(event.target.value)} /></label>
      <div className="kv-row"><button type="button" className="kv-button" disabled={disabled || options.loading || options.page <= 1} onClick={() => options.loadPage(options.page - 1)}>{t('ui.options.previous')}</button>
        <span className="kv-hint" role="status">{t('ui.options.page', { page: formatNumber(options.page), total: formatNumber(options.total) })}</span>
        <button type="button" className="kv-button" disabled={disabled || options.loading || options.page * options.pageSize >= options.total} onClick={() => options.loadPage(options.page + 1)}>{t('ui.options.next')}</button></div>
      {options.loading && <span role="status" className="kv-hint">{t('ui.options.loading_label', { label })}</span>}
      {options.error && <Notice kind="error"><p>{options.error}</p><button type="button" className="kv-button" onClick={options.reload} disabled={disabled}>{t('ui.options.reload')}</button></Notice>}
      {options.ready && !options.loading && !options.error && options.total === 0 && <p className="kv-hint">{t('ui.options.empty')}</p>}
    </div>}
    {bindingError && !error && <span className="kv-error" role="alert">{bindingError}</span>}
  </div></FieldContext.Provider>;
}
