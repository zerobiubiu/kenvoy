import { isUploadedFiles } from '../../domain/files';
import type { FieldDefinition } from '../../domain/types';
import { AttachmentList } from '../images';

export function displayValue(value: unknown, field?: FieldDefinition): string {
  if (value === null || value === undefined || value === '') return '—';
  if (typeof value === 'boolean') return value ? '是' : '否';
  if (field?.kind === 'select') return field.options.find(option => option.value === value)?.label ?? String(value);
  if (field?.kind === 'file' && isUploadedFiles(value)) return value.length ? value.map(file => file.name).join('、') : '未上传文件';
  return String(value);
}

export function RecordValueDisplay({ value, field, compact = false, readable = true }: { value: unknown; field?: FieldDefinition; compact?: boolean; readable?: boolean }) {
  if (field?.kind === 'file') {
    if (value == null) return <span className="kv-hint">未上传文件</span>;
    if (!isUploadedFiles(value)) return <span className="kv-error">附件信息无效，请重新加载记录。</span>;
    return <AttachmentList files={value} compact={compact} readable={readable} />;
  }
  return <>{displayValue(value, field)}</>;
}
