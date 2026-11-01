import type { FieldDefinition, FieldError, JsonValue, ResourceDefinition } from './types.ts';
import { acceptsFile, isFileConstraints, isUploadedFiles } from './files.ts';

export type ValidationResult<T> = { ok: true; value: T } | { ok: false; errors: FieldError[] };
export type WriteMode = 'create' | 'patch';
// Rust char::is_whitespace 使用 Unicode White_Space；JS trim 的 BOM/NEL 规则不同。
const WHITE_SPACE = /^[\u0009-\u000D\u0020\u0085\u00A0\u1680\u2000-\u200A\u2028\u2029\u202F\u205F\u3000]*$/u;
const own = (object: object, name: string): boolean => Object.prototype.hasOwnProperty.call(object, name);
const valid = <T>(value: T): ValidationResult<T> => ({ ok: true, value });
function invalid(field: FieldDefinition, code: string, message: string): ValidationResult<never> {
  return { ok: false, errors: [{ code, field: field.name, message }] };
}

/** 与 Rust validate_field 同步验证 JSON 值，不将字符串偷偷转为数字或布尔。 */
export function validateField(field: FieldDefinition, value: unknown): ValidationResult<JsonValue> {
  if (typeof value === 'string') {
    if (value.includes('\0')) return invalid(field, 'KV4002', '字段文本不能包含 NUL 字符');
    if ([...value].some(char => { const point = char.codePointAt(0)!; return point >= 0xD800 && point <= 0xDFFF; })) {
      return invalid(field, 'KV4002', '字段文本必须为有效 Unicode 字符');
    }
  }
  if (value === null) return field.required ? invalid(field, 'KV4001', '必填字段不能为 null') : valid(null);
  switch (field.kind) {
    case 'text':
    case 'textarea':
      if (typeof value !== 'string') return invalid(field, 'KV4002', '必须为字符串');
      if (field.required && WHITE_SPACE.test(value)) return invalid(field, 'KV4001', '必填文本不能为空或全部为空白');
      if (field.max_length != null && [...value].length > field.max_length) return invalid(field, 'KV4003', '文本长度超过 maxLength（按 Unicode 标量计数）');
      return valid(value);
    case 'number':
      if (typeof value !== 'number' || !Number.isFinite(value)) return invalid(field, 'KV4002', '必须为有限 JSON 数字，不能使用数字字符串');
      if (field.integer && !Number.isSafeInteger(value)) return invalid(field, 'KV4004', '必须为 ±9007199254740991 范围内的安全整数');
      if ((field.min != null && value < field.min) || (field.max != null && value > field.max)) return invalid(field, 'KV4005', '数字不满足 min/max 包含边界');
      return valid(value === 0 ? 0 : value);
    case 'boolean':
      return typeof value === 'boolean' ? valid(value) : invalid(field, 'KV4002', '必须为 true 或 false，不能使用 0、1 或字符串');
    case 'date':
      if (typeof value !== 'string') return invalid(field, 'KV4002', '日期必须为字符串');
      return validDate(value) ? valid(value) : invalid(field, 'KV4006', '日期必须为 0001..9999 年内的有效 YYYY-MM-DD 日历日期');
    case 'datetime': {
      if (typeof value !== 'string') return invalid(field, 'KV4002', '日期时间必须为字符串');
      const normalized = normalizeDatetime(value);
      return normalized === null ? invalid(field, 'KV4007', '日期时间必须使用显式时区 RFC3339、最多 3 位小数秒；不接受闰秒或超范围年份') : valid(normalized);
    }
    case 'select':
      if (typeof value !== 'string') return invalid(field, 'KV4002', '选项值必须为字符串');
      if (field.required && WHITE_SPACE.test(value)) return invalid(field, 'KV4001', '必填选项值不能为空或全部为空白');
      if (!field.options_source && !field.options.some(option => option.value === value)) return invalid(field, 'KV4008', '值不属于声明的选项');
      return valid(value);
    case 'file':
      if (!Array.isArray(value)) return invalid(field, 'KV4002', '文件字段必须为附件数组');
      if (field.required && value.length === 0) return invalid(field, 'KV4001', '必填文件字段至少上传一个文件');
      if (!isFileConstraints(field.upload) || !isUploadedFiles(value)) return invalid(field, 'KV4014', '附件元数据、标识或上传约束无效，不能重复使用同一附件');
      if (value.length > field.upload.max_files) return invalid(field, 'KV4014', `附件数量不能超过 ${field.upload.max_files} 个`);
      if (value.some(file => file.size > field.upload!.max_size)) return invalid(field, 'KV4014', '附件大小超过该字段的单文件限制');
      if (value.some(file => !acceptsFile(file.content_type, field.upload!.accept))) return invalid(field, 'KV4014', '附件类型不符合该字段允许的 MIME 类型');
      return valid(value);
  }
}

/** Create 处理缺失默认值，Patch 只验证显式字段；调用者仍须按表单/权限裁剪字段。 */
export function validateRecord(resource: ResourceDefinition, input: unknown, mode: WriteMode = 'create'): ValidationResult<Record<string, JsonValue>> {
  if (input === null || typeof input !== 'object' || Array.isArray(input) || ![Object.prototype, null].includes(Object.getPrototypeOf(input))) {
    return { ok: false, errors: [{ code: 'KV4010', field: null, message: '记录必须为 JSON 对象' }] };
  }
  const object = input as Record<string, unknown>;
  const errors: FieldError[] = [];
  for (const name of Object.keys(object)) {
    if (!resource.fields.some(field => field.name === name)) errors.push({ code: 'KV4011', field: name, message: '字段未在资源中声明或属于只读系统字段' });
  }
  const entries: [string, JsonValue][] = [];
  for (const field of resource.fields) {
    if (mode === 'patch' && !own(object, field.name)) continue;
    const value = own(object, field.name) ? object[field.name] : own(field, 'default') ? field.default : null;
    const result = validateField(field, value);
    if (result.ok) entries.push([field.name, result.value]); else errors.push(...result.errors);
  }
  return errors.length ? { ok: false, errors } : valid(Object.fromEntries(entries));
}

/** 仅用于 DOM 草稿到 JSON 的显式转换，随后必须调用 validateField/validateRecord。 */
export function normalizeInput(field: FieldDefinition, draft: unknown): unknown {
  if (typeof draft !== 'string') return draft;
  if (field.kind === 'file') return draft;
  if (field.kind === 'text' || field.kind === 'textarea') return draft;
  if (field.kind === 'select') return draft === '' && !field.options.some(option => option.value === '') ? null : draft;
  if (draft === '') return null;
  if (field.kind === 'boolean') return draft === 'true' ? true : draft === 'false' ? false : draft;
  if (field.kind === 'number' && /^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?$/.exec(draft)?.[0] === draft) {
    // 先检查十进制原文，防止大数的小数部分在 Number 舍入后伪装成整数。
    if (field.integer) {
      const [mantissa, exponent = '0'] = draft.replace(/^[+-]/, '').split(/[eE]/);
      const fraction = mantissa!.split('.')[1] ?? '';
      const scale = fraction.length - Number(exponent);
      const digits = mantissa!.replace('.', '');
      if (scale > 0 && /[1-9]/.test(digits.slice(-scale))) return draft;
    }
    return Number(draft);
  }
  return draft;
}

function validDate(value: string): boolean {
  if (value.length !== 10 || !/^\d{4}-\d{2}-\d{2}$/.test(value)) return false;
  const year = Number(value.slice(0, 4));
  const month = Number(value.slice(5, 7));
  const day = Number(value.slice(8, 10));
  if (year < 1 || year > 9999 || month < 1 || month > 12 || day < 1) return false;
  const leap = year % 4 === 0 && (year % 100 !== 0 || year % 400 === 0);
  const days = [31, leap ? 29 : 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
  return day <= days[month - 1]!;
}

function normalizeDatetime(value: string): string | null {
  const match = /^(\d{4}-\d{2}-\d{2})T(\d{2}):(\d{2}):(\d{2})(?:\.(\d{1,3}))?(Z|[+-]\d{2}:\d{2})$/.exec(value);
  if (!match || match[0] !== value || !validDate(match[1]!)) return null;
  const hour = Number(match[2]), minute = Number(match[3]), second = Number(match[4]);
  if (hour > 23 || minute > 59 || second > 59) return null;
  const zone = match[6]!;
  const zoneHour = zone === 'Z' ? 0 : Number(zone.slice(1, 3));
  const zoneMinute = zone === 'Z' ? 0 : Number(zone.slice(4, 6));
  if (zoneHour > 23 || zoneMinute > 59) return null;
  // 避免 Date.UTC 将 00..99 年隐式映射到 1900..1999 年。
  const date = new Date(0);
  date.setUTCFullYear(Number(value.slice(0, 4)), Number(value.slice(5, 7)) - 1, Number(value.slice(8, 10)));
  date.setUTCHours(hour, minute, second, Number((match[5] ?? '').padEnd(3, '0')));
  const offset = (zoneHour * 60 + zoneMinute) * (zone[0] === '-' ? -1 : 1);
  date.setTime(date.getTime() - offset * 60_000);
  if (date.getUTCFullYear() < 1 || date.getUTCFullYear() > 9999) return null;
  return date.toISOString();
}
