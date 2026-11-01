import type { FieldBinding } from '../runtime/bindings';
import { formatFileSize, isUploadedFiles } from '../../domain/files';
import { AttachmentList } from '../images';
import { Notice } from '../ui';

export function FileField({ binding, id, readable = true }: { binding: FieldBinding; id: string; readable?: boolean }) {
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
      ? `最多 ${rules.max_files} 个文件，每个不超过 ${formatFileSize(rules.max_size)}。${rules.accept.length ? `允许：${rules.accept.join('、')}。` : ''}选择后立即上传，保存表单后关联记录。`
      : '此字段缺少上传配置，请联系管理员。')}</span>
    {files.length > 0 && <p className="kv-hint">已选 {files.length}{rules ? ` / ${rules.max_files}` : ''} 个文件</p>}
    <AttachmentList files={files} readable={readable} disabled={binding.disabled || state.busy || state.outcomeUnknown} removing={state.removing}
      onRemove={binding.disabled ? undefined : file => { void binding.removeUpload(file).catch(() => {}); }} />
    {state.busy && <div className="kv-row"><p className="kv-hint" role="status">正在上传，已完成 {state.completed} / {state.total} 个文件。</p>
      <button type="button" className="kv-button compact" disabled={binding.disabled} onClick={binding.cancelUpload}>取消本次上传</button></div>}
    {!binding.disabled && !state.busy && !state.error && state.total > 0 && <p className="kv-hint" role="status">{state.canceled ? `本次上传已取消，已完成的 ${state.completed} 个文件保留。` : `本次已上传 ${state.completed} 个文件，可继续选择或逐项移除。`}</p>}
    {state.error && <Notice kind={state.outcomeUnknown ? 'warning' : 'error'}><p>{state.error}</p>
      {state.outcomeUnknown && <p>请取消本次填写后重新开始，不要重复上传。无法确认的暂存文件会自动过期。</p>}</Notice>}
  </div>;
}
