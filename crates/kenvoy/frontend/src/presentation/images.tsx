import { useState } from 'react';
import { fileUrl } from '../infrastructure/api';
import { formatFileSize, isImageFile, isSafeImageSource, isUploadedFiles } from '../domain/files';
import type { UploadedFile } from '../domain/types';
import { useI18n } from './i18n';

export interface ImageProps {
  src: string;
  alt: string;
  width?: number;
  height?: number;
  fit?: 'contain' | 'cover';
}

/** 图片 URL 与上传图片共用安全来源和可辨别的加载、失败状态。 */
export function Image({ src, alt, width, height, fit = 'contain' }: ImageProps) {
  const { t } = useI18n();
  const [result, setResult] = useState<{ src: string; state: 'loaded' | 'failed' } | null>(null);
  if (src === '') return <span className="kv-image-fallback kv-hint" role="status">{t('ui.image.empty')}</span>;
  if (!isSafeImageSource(src) || typeof alt !== 'string' || ![width, height].every(size => size === undefined || (Number.isInteger(size) && size >= 1 && size <= 4096))
    || (fit !== 'contain' && fit !== 'cover')) return <span className="kv-image-fallback kv-error" role="alert">{t('ui.image.invalid')}</span>;
  const state = result?.src === src ? result.state : 'loading';
  return <span className={'kv-image' + (state === 'failed' ? ' kv-image-failed' : '')} style={{ width, minHeight: state !== 'loaded' ? height ?? 80 : undefined }} aria-busy={state === 'loading'}>
    {state !== 'failed' && <img key={src} src={src} alt={alt} width={width} height={height} loading="lazy" decoding="async" referrerPolicy="no-referrer"
      style={{ width: width ? '100%' : undefined, height, objectFit: fit }} onLoad={() => setResult({ src, state: 'loaded' })} onError={() => setResult({ src, state: 'failed' })} />}
    {state === 'loading' && <span className="kv-image-status kv-hint" role="status">{t('ui.image.loading')}</span>}
    {state === 'failed' && <span className="kv-image-fallback kv-error" role="status">{alt && <>{alt}{t('ui.field.hint_separator')}</>}{t('ui.image.failed')}</span>}
  </span>;
}

export function AttachmentList({ files, compact = false, readable = true, disabled = false, removing = [], onRemove }: {
  files: readonly UploadedFile[]; compact?: boolean; readable?: boolean; disabled?: boolean;
  removing?: readonly string[]; onRemove?: (file: UploadedFile) => void;
}) {
  const { t, locale } = useI18n();
  if (!isUploadedFiles(files)) return <span className="kv-error">{t('ui.files.invalid')}</span>;
  if (files.length === 0) return <span className="kv-hint">{t('ui.files.empty')}</span>;
  return <ul className={'kv-attachments' + (compact ? ' compact' : '')} aria-label={t('ui.files.list')}>{files.map(file => <li key={file.id} className="kv-attachment">
    {readable && isImageFile(file) && <Image src={fileUrl(file)} alt={file.name} width={compact ? 96 : 160} height={compact ? 72 : 112} />}
    <div className="kv-attachment-info">
      {readable ? <a className="kv-file-link" href={fileUrl(file, true)} download={file.name}>{file.name}<span className="kv-file-action">{t('ui.files.download')}</span></a>
        : <span className="kv-wrap">{file.name}</span>}
      <span className="kv-hint">{formatFileSize(file.size, locale)}</span>
      {!readable && <span className="kv-hint">{t('ui.files.no_access')}</span>}
    </div>
    {onRemove && <button type="button" className="kv-button compact danger" disabled={disabled || removing.includes(file.id)}
      aria-label={t('ui.files.remove_label', { name: file.name })} onClick={() => onRemove(file)}>{t(removing.includes(file.id) ? 'ui.files.removing' : 'ui.files.remove')}</button>}
  </li>)}</ul>;
}
