import type { FieldBinding } from '../runtime/bindings';
import { formatFileSize, isUploadedFiles } from '../../domain/files';
import { AttachmentList } from '../images';
import { Notice } from '../ui';
import { useI18n } from '../i18n';
import { translateError } from '../../domain/i18n/error';

export function FileField({ binding, id, readable = true }: { binding: FieldBinding; id: string; readable?: boolean }) {
  const { t, locale, formatNumber } = useI18n();
  const { field, uploadState: state } = binding;
  const files = isUploadedFiles(binding.value) ? binding.value : [];
  const rules = field.upload;
  const label = field.label || field.name;
  const unavailable = binding.disabled || state.busy || state.removing.length > 0 || state.outcomeUnknown || !rules;
  return <div className="kv-field kv-upload-field">
    <label htmlFor={id}>{label}{field.required && <span aria-hidden="true"> *</span>}</label>
    <input id={id} name={field.name} type="file" multiple accept={rules?.accept.length ? rules.accept.join(',') : undefined}
      disabled={unavailable || Boolean(rules && files.length >= rules.max_files)} aria-invalid={Boolean(binding.error || state.error)} aria-describedby={id + '-help'}
      onChange={event => {
        const selected = Array.from(event.currentTarget.files ?? []);
        event.currentTarget.value = '';
        void binding.upload(selected).catch(() => {});
      }} />
    <span id={id + '-help'} className={binding.error ? 'kv-error' : 'kv-hint'}>{binding.error || (rules
      ? [t('ui.files.rules', { count: rules.max_files, max: formatNumber(rules.max_files), size: formatFileSize(rules.max_size, locale) }),
        rules.accept.length ? t('ui.files.accept', { types: rules.accept.join(', ') }) : '', t('ui.files.immediate')].filter(Boolean).join(' ')
      : t('ui.files.no_config'))}</span>
    {files.length > 0 && <p className="kv-hint">{t('ui.files.selected', { count: files.length, display_count: formatNumber(files.length), limit: rules ? ` / ${formatNumber(rules.max_files)}` : '' })}</p>}
    <AttachmentList files={files} readable={readable} disabled={binding.disabled || state.busy || state.outcomeUnknown} removing={state.removing}
      onRemove={binding.disabled ? undefined : file => { void binding.removeUpload(file).catch(() => {}); }} />
    {state.busy && <div className="kv-row"><p className="kv-hint" role="status">{t('ui.files.uploading', { completed: formatNumber(state.completed), total: formatNumber(state.total) })}</p>
      <button type="button" className="kv-button compact" disabled={binding.disabled} onClick={binding.cancelUpload}>{t('ui.files.cancel')}</button></div>}
    {!binding.disabled && !state.busy && !state.error && state.total > 0 && <p className="kv-hint" role="status">{t(state.canceled ? 'ui.files.canceled' : 'ui.files.completed', { count: state.completed, display_count: formatNumber(state.completed) })}</p>}
    {state.error && <Notice kind={state.outcomeUnknown ? 'warning' : 'error'}><p>{state.error_message ? translateError(state.error_message, t) : state.error}</p>
      {state.outcomeUnknown && <p>{t('ui.files.uncertain')}</p>}</Notice>}
  </div>;
}
