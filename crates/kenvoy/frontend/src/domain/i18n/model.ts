export type MessageParams = Readonly<Record<string, string | number | boolean | null>>;
export interface LanguageOption {
  code: string;
  alias?: string;
  display: string;
}
export interface I18nBundle {
  format_version: 2;
  fallback: string;
  languages: LanguageOption[];
  messages: Record<string, Record<string, string>>;
}
export type Translate = (key: string, params?: MessageParams, fallback?: string) => string;
export interface I18nFormatters {
  formatNumber: (value: number, options?: Intl.NumberFormatOptions) => string;
  formatDate: (value: string | Date, options?: Intl.DateTimeFormatOptions) => string;
  formatDateTime: (value: string | Date, options?: Intl.DateTimeFormatOptions) => string;
}
export interface I18n extends I18nFormatters {
  locale: string;
  locales: readonly string[];
  t: Translate;
}
/** 应用描述尚未载入时的启动上下文；不是缺失或损坏配置的兼容回退。 */
export const LOADING_I18N: I18nBundle = {
  format_version: 2,
  fallback: 'zh-CN',
  languages: [
    { code: 'zh-CN', alias: 'cn', display: '简体中文' },
    { code: 'en-US', alias: 'en', display: 'English' },
    { code: 'ko-KR', alias: 'ko', display: '한국어' },
  ],
  messages: { 'zh-CN': {}, 'en-US': {}, 'ko-KR': {} },
};

const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const nonblank = (value: unknown): value is string => typeof value === 'string' && value.trim().length > 0;
function languageCode(value: unknown): value is string {
  if (typeof value !== 'string' || value.length > 64 || !/^[A-Za-z]{2,8}(?:-[A-Za-z0-9]{1,8})*$/.test(value)) return false;
  try { return Intl.getCanonicalLocales(value).length === 1; } catch { return false; }
}

function languageOption(value: unknown): value is LanguageOption {
  return object(value) && Object.keys(value).every(key => ['code', 'alias', 'display'].includes(key))
    && languageCode(value.code) && nonblank(value.display)
    && (!Object.hasOwn(value, 'alias') || nonblank(value.alias));
}

/** 只接受服务端规范化后的代码词典；别名仅用于选择，不形成第二套查词协议。 */
export function isI18nBundle(value: unknown): value is I18nBundle {
  if (!object(value) || Object.keys(value).some(key => !['format_version', 'fallback', 'languages', 'messages'].includes(key))
    || value.format_version !== 2 || !Array.isArray(value.languages) || !value.languages.length
    || !value.languages.every(languageOption)) return false;
  const languages = value.languages as LanguageOption[];
  const codes = languages.map(language => language.code);
  const identifiers = new Set(codes.map(code => code.toLowerCase()));
  if (identifiers.size !== codes.length || typeof value.fallback !== 'string' || !codes.includes(value.fallback)) return false;
  for (const { alias } of languages) {
    if (alias === undefined) continue;
    const identifier = alias.toLowerCase();
    if (identifiers.has(identifier)) return false;
    identifiers.add(identifier);
  }
  if (!object(value.messages) || Object.keys(value.messages).length !== codes.length) return false;
  let keys: string[] | undefined;
  for (const code of codes) {
    if (!Object.hasOwn(value.messages, code)) return false;
    const messages = value.messages[code];
    if (!object(messages)) return false;
    const messageKeys = Object.keys(messages);
    keys ??= messageKeys;
    if (messageKeys.length !== keys.length || !keys.every(key => Object.hasOwn(messages, key)
      && typeof messages[key] === 'string' && messages[key].length > 0)) return false;
  }
  return true;
}
