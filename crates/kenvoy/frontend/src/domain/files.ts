import type { ApplicationDescription, FileConstraints, FieldValue, UploadedFile } from './types.ts';

export const MAX_FILE_SIZE = 10 * 1024 * 1024;
const MIME_TOKEN = "[a-z0-9!#$%&'+.^_`|~-]+";
const MIME = new RegExp(`^${MIME_TOKEN}/${MIME_TOKEN}$`);
const ACCEPT = new RegExp(`^${MIME_TOKEN}/(?:${MIME_TOKEN}|\\*)$`);
const CONTROL = /[\u0000-\u001f\u007f-\u009f]/u;

export interface FileValidationIssue {
  message_key: string;
  params: Record<string, string | number | boolean | null>;
}
const issue = (message_key: string, params: FileValidationIssue['params'] = {}): FileValidationIssue => ({ message_key, params });

export function validFileName(value: unknown): value is string {
  return typeof value === 'string' && value !== '' && value !== '.' && value !== '..'
    && [...value].length <= 255 && !/[\\/]/.test(value) && !CONTROL.test(value)
    && ![...value].some(char => { const point = char.codePointAt(0)!; return point >= 0xd800 && point <= 0xdfff; });
}

export function validMime(value: unknown): value is string {
  return typeof value === 'string' && MIME.exec(value)?.[0] === value;
}

function uploadedFileIssue(value: unknown): FileValidationIssue | null {
  if (value === null || typeof value !== 'object' || Array.isArray(value)
    || ![Object.prototype, null].includes(Object.getPrototypeOf(value))) return issue('validation.files.metadata');
  const file = value as Record<string, unknown>;
  if (Object.keys(file).length !== 4 || !['id', 'name', 'content_type', 'size'].every(key => Object.hasOwn(file, key))) return issue('validation.files.metadata');
  if (typeof file.id !== 'string' || typeof file.name !== 'string' || typeof file.content_type !== 'string'
    || typeof file.size !== 'number' || !Number.isSafeInteger(file.size) || file.size < 0) return issue('validation.files.metadata_types');
  if (/^[a-f0-9]{64}$/.exec(file.id)?.[0] !== file.id) return issue('validation.files.id', { length: 64 });
  if (!validFileName(file.name)) return issue('validation.files.name', { max: 255 });
  if (file.size < 1 || file.size > MAX_FILE_SIZE) return issue('validation.files.size', { min: 1, max: MAX_FILE_SIZE });
  if (!validMime(file.content_type)) return issue('validation.files.mime');
  return null;
}

export function isUploadedFile(value: unknown): value is UploadedFile {
  return uploadedFileIssue(value) === null;
}

export function uploadedFilesIssue(value: unknown): FileValidationIssue | null {
  if (!Array.isArray(value)) return issue('validation.type.files');
  if (value.length > 20) return issue('validation.files.max_files', { max: 20 });
  const ids = new Set<string>();
  for (let index = 0; index < value.length; index++) {
    if (!Object.hasOwn(value, index)) return issue('validation.files.metadata');
    const file: unknown = value[index];
    const problem = uploadedFileIssue(file);
    if (problem) return problem;
    const id = (file as UploadedFile).id;
    if (ids.has(id)) return issue('validation.files.duplicate');
    ids.add(id);
  }
  return null;
}

export function isUploadedFiles(value: unknown): value is UploadedFile[] {
  return uploadedFilesIssue(value) === null;
}

export function isFieldValue(value: unknown): value is FieldValue {
  return value === null || typeof value === 'string' || typeof value === 'boolean'
    || (typeof value === 'number' && Number.isFinite(value)) || isUploadedFiles(value);
}

export function isFileConstraints(value: unknown): value is FileConstraints {
  return fileConstraintsIssue(value) === null;
}

/** 错误来自实际机器规则；不从默认中文描述反推约束。 */
export function fileConstraintsIssue(value: unknown): FileValidationIssue | null {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) return issue('validation.files.constraints.missing');
  const rules = value as Record<string, unknown>;
  if (typeof rules.max_size !== 'number' || !Number.isSafeInteger(rules.max_size) || rules.max_size < 1 || rules.max_size > MAX_FILE_SIZE) {
    return issue('validation.files.constraints.max_size', { min: 1, max: MAX_FILE_SIZE });
  }
  if (typeof rules.max_files !== 'number' || !Number.isSafeInteger(rules.max_files) || rules.max_files < 1 || rules.max_files > 20) {
    return issue('validation.files.constraints.max_files', { min: 1, max: 20 });
  }
  if (!Array.isArray(rules.accept) || !rules.accept.every(item => typeof item === 'string' && ACCEPT.exec(item)?.[0] === item)) return issue('validation.files.constraints.accept');
  return null;
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

export function formatFileSize(size: number, locale?: string): string {
  const amount = size >= 1024 * 1024 ? size / (1024 * 1024) : size >= 1024 ? size / 1024 : size;
  const unit = size >= 1024 * 1024 ? 'MiB' : size >= 1024 ? 'KiB' : 'B';
  return `${locale ? fileSizeFormatter(locale).format(amount) : size < 1024 ? size : Number(amount.toFixed(1))} ${unit}`;
}

const fileSizeFormatters = new Map<string, Intl.NumberFormat>();
function fileSizeFormatter(locale: string): Intl.NumberFormat {
  let formatter = fileSizeFormatters.get(locale);
  if (!formatter) {
    formatter = new Intl.NumberFormat(locale, { maximumFractionDigits: 1 });
    fileSizeFormatters.set(locale, formatter);
  }
  return formatter;
}
