import type { ApplicationDescription, FileConstraints, FieldValue, UploadedFile } from './types.ts';

export const MAX_FILE_SIZE = 10 * 1024 * 1024;
const MIME_TOKEN = "[a-z0-9!#$%&'+.^_`|~-]+";
const MIME = new RegExp(`^${MIME_TOKEN}/${MIME_TOKEN}$`);
const ACCEPT = new RegExp(`^${MIME_TOKEN}/(?:${MIME_TOKEN}|\\*)$`);
const CONTROL = /[\u0000-\u001f\u007f-\u009f]/u;

export function validFileName(value: unknown): value is string {
  return typeof value === 'string' && value !== '' && value !== '.' && value !== '..'
    && [...value].length <= 255 && !/[\\/]/.test(value) && !CONTROL.test(value)
    && ![...value].some(char => { const point = char.codePointAt(0)!; return point >= 0xd800 && point <= 0xdfff; });
}

export function validMime(value: unknown): value is string {
  return typeof value === 'string' && MIME.exec(value)?.[0] === value;
}

export function isUploadedFile(value: unknown): value is UploadedFile {
  if (value === null || typeof value !== 'object' || Array.isArray(value)
    || ![Object.prototype, null].includes(Object.getPrototypeOf(value))) return false;
  const file = value as Record<string, unknown>;
  return Object.keys(file).length === 4 && typeof file.id === 'string' && /^[a-f0-9]{64}$/.exec(file.id)?.[0] === file.id
    && validFileName(file.name) && validMime(file.content_type)
    && typeof file.size === 'number' && Number.isSafeInteger(file.size) && file.size >= 1 && file.size <= MAX_FILE_SIZE;
}

export function isUploadedFiles(value: unknown): value is UploadedFile[] {
  if (!Array.isArray(value) || value.length > 20) return false;
  const ids = new Set<string>();
  for (let index = 0; index < value.length; index++) {
    if (!Object.hasOwn(value, index)) return false;
    const file: unknown = value[index];
    if (!isUploadedFile(file) || ids.has(file.id)) return false;
    ids.add(file.id);
  }
  return true;
}

export function isFieldValue(value: unknown): value is FieldValue {
  return value === null || typeof value === 'string' || typeof value === 'boolean'
    || (typeof value === 'number' && Number.isFinite(value)) || isUploadedFiles(value);
}

export function isFileConstraints(value: unknown): value is FileConstraints {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) return false;
  const rules = value as Record<string, unknown>;
  return typeof rules.max_size === 'number' && Number.isSafeInteger(rules.max_size) && rules.max_size >= 1 && rules.max_size <= MAX_FILE_SIZE
    && typeof rules.max_files === 'number' && Number.isSafeInteger(rules.max_files) && rules.max_files >= 1 && rules.max_files <= 20
    && Array.isArray(rules.accept) && rules.accept.every(item => typeof item === 'string' && ACCEPT.exec(item)?.[0] === item);
}

export function acceptsFile(contentType: string, accept: readonly string[]): boolean {
  return accept.length === 0 || accept.some(rule => rule === contentType || (rule.endsWith('/*') && contentType.startsWith(rule.slice(0, -1))));
}

export function isImageFile(file: UploadedFile): boolean {
  return ['image/png', 'image/jpeg', 'image/gif', 'image/webp'].includes(file.content_type);
}

/** 仅避免界面继续发出必定无权读取的请求；服务端始终执行实际授权。 */
export function canReadFile(application: ApplicationDescription, resource: string, field: string): boolean {
  return application.permissions.some(policy => policy.target.kind === 'resource' && policy.target.name === resource
    && policy.operation === 'read' && policy.form === null && policy.fields.includes(field));
}

/** 只接受显式站内路径或无凭据的 HTTP(S)，非法来源不交给浏览器加载。 */
export function isSafeImageSource(src: unknown): src is string {
  if (typeof src !== 'string' || src === '' || src.trim() !== src || src.includes('\\') || CONTROL.test(src)) return false;
  if (src.startsWith('/')) return !src.startsWith('//');
  try {
    const authority = src.slice(src.indexOf('://') + 3).split(/[/?#]/, 1)[0]!;
    if (!authority || authority.includes('@')) return false;
    const url = new URL(src);
    return (url.protocol === 'https:' || url.protocol === 'http:') && /^https?:\/\//i.test(src)
      && url.username === '' && url.password === '';
  } catch { return false; }
}

export function formatFileSize(size: number): string {
  return size >= 1024 * 1024 ? `${Number((size / (1024 * 1024)).toFixed(1))} MiB`
    : size >= 1024 ? `${Number((size / 1024).toFixed(1))} KiB` : `${size} B`;
}
