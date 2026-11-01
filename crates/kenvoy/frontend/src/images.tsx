import { useState } from 'react';
import { fileUrl } from './api';
import { formatFileSize, isImageFile, isSafeImageSource, isUploadedFiles } from './files';
import type { UploadedFile } from './types';

export interface ImageProps {
  src: string;
  alt: string;
  width?: number;
  height?: number;
  fit?: 'contain' | 'cover';
}

/** 图片 URL 与上传图片共用安全来源和可辨别的加载、失败状态。 */
export function Image({ src, alt, width, height, fit = 'contain' }: ImageProps) {
  const [result, setResult] = useState<{ src: string; state: 'loaded' | 'failed' } | null>(null);
  if (src === '') return <span className="kv-image-fallback kv-hint" role="status">暂无图片</span>;
  if (!isSafeImageSource(src) || typeof alt !== 'string' || ![width, height].every(size => size === undefined || (Number.isInteger(size) && size >= 1 && size <= 4096))
    || (fit !== 'contain' && fit !== 'cover')) return <span className="kv-image-fallback kv-error" role="alert">图片地址或尺寸无效，无法加载。</span>;
  const state = result?.src === src ? result.state : 'loading';
  return <span className={'kv-image' + (state === 'failed' ? ' kv-image-failed' : '')} style={{ width, minHeight: state !== 'loaded' ? height ?? 80 : undefined }} aria-busy={state === 'loading'}>
    {state !== 'failed' && <img key={src} src={src} alt={alt} width={width} height={height} loading="lazy" decoding="async" referrerPolicy="no-referrer"
      style={{ width: width ? '100%' : undefined, height, objectFit: fit }} onLoad={() => setResult({ src, state: 'loaded' })} onError={() => setResult({ src, state: 'failed' })} />}
    {state === 'loading' && <span className="kv-image-status kv-hint" role="status">图片加载中…</span>}
    {state === 'failed' && <span className="kv-image-fallback kv-error" role="status">{alt ? `${alt}：` : ''}图片未能加载，请检查地址或访问权限。</span>}
  </span>;
}

export function AttachmentList({ files, compact = false, readable = true, disabled = false, removing = [], onRemove }: {
  files: readonly UploadedFile[]; compact?: boolean; readable?: boolean; disabled?: boolean;
  removing?: readonly string[]; onRemove?: (file: UploadedFile) => void;
}) {
  if (!isUploadedFiles(files)) return <span className="kv-error">附件信息无效，请重新加载记录。</span>;
  if (files.length === 0) return <span className="kv-hint">未上传文件</span>;
  return <ul className={'kv-attachments' + (compact ? ' compact' : '')} aria-label="附件列表">{files.map(file => <li key={file.id} className="kv-attachment">
    {readable && isImageFile(file) && <Image src={fileUrl(file)} alt={file.name} width={compact ? 96 : 160} height={compact ? 72 : 112} />}
    <div className="kv-attachment-info">
      {readable ? <a className="kv-file-link" href={fileUrl(file, true)} download={file.name}>{file.name}<span className="kv-file-action">下载</span></a>
        : <span className="kv-wrap">{file.name}</span>}
      <span className="kv-hint">{formatFileSize(file.size)}</span>
      {!readable && <span className="kv-hint">文件已提交；当前账号没有读取权限。</span>}
    </div>
    {onRemove && <button type="button" className="kv-button compact danger" disabled={disabled || removing.includes(file.id)}
      aria-label={'移除文件 ' + file.name} onClick={() => onRemove(file)}>{removing.includes(file.id) ? '正在移除…' : '移除'}</button>}
  </li>)}</ul>;
}
