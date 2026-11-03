import { isUploadedFiles } from '../../domain/files';
import { AttachmentList } from '../images';
import { useI18n } from '../i18n';
import { displayValue } from '../../domain/i18n/display';
import type { DisplayField } from '../../domain/i18n/display';


export function RecordValueDisplay({ value, field, compact = false, readable = true }: { value: unknown; field?: DisplayField; compact?: boolean; readable?: boolean }) {
  const i18n = useI18n();
  if (field?.kind === 'file') {
    if (value == null) return <span className="kv-hint">{i18n.t('ui.files.empty')}</span>;
    if (!isUploadedFiles(value)) return <span className="kv-error">{i18n.t('ui.files.invalid')}</span>;
    return <AttachmentList files={value} compact={compact} readable={readable} />;
  }
  return <>{displayValue(value, field, i18n)}</>;
}
