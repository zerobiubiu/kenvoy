export type MessageParams = Readonly<Record<string, string | number | boolean | null>>;
export interface I18nBundle {
  format_version: 1;
  default_locale: string;
  locales: string[];
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
export const LEGACY_I18N: I18nBundle = {
  format_version: 1, default_locale: 'zh-CN', locales: ['zh-CN'], messages: { 'zh-CN': {} },
};

/** 与服务端共用严格语言标记范围，拒绝损坏的描述而不是静默退回中文。 */
export function isI18nBundle(value: unknown): value is I18nBundle {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return false;
  const bundle = value as Record<string, unknown>;
  const canonical = (locale: unknown): locale is string => {
    if (typeof locale !== 'string' || locale.length > 64 || !/^[A-Za-z]{2,8}(?:-[A-Za-z0-9]{1,8})*$/.test(locale)) return false;
    try { return Intl.getCanonicalLocales(locale).length === 1; } catch { return false; }
  };
  if (bundle.format_version !== 1 || !canonical(bundle.default_locale) || !Array.isArray(bundle.locales)
    || !bundle.locales.length || !bundle.locales.every(canonical)) return false;
  const locales = bundle.locales as string[];
  if (new Set(locales.map(locale => locale.toLowerCase())).size !== locales.length || !locales.includes(bundle.default_locale)) return false;
  if (!bundle.messages || typeof bundle.messages !== 'object' || Array.isArray(bundle.messages)) return false;
  const dictionaries = bundle.messages as Record<string, unknown>;
  if (Object.keys(dictionaries).length !== locales.length || locales.some(locale => !Object.hasOwn(dictionaries, locale))) return false;
  return Object.entries(dictionaries).every(([locale, messages]) => locales.includes(locale) && messages !== null
    && typeof messages === 'object' && !Array.isArray(messages)
    && Object.entries(messages).every(([key, message]) => key.length > 0 && typeof message === 'string'));
}
