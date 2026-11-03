import type { I18nBundle } from './model.ts';

export const LOCALE_STORAGE_KEY = 'kenvoy.locale';

/** 精确匹配优先于基础语言匹配，始终返回白名单中的原始语言标记。 */
export function matchLocale(candidate: string | null | undefined, locales: readonly string[]): string | undefined {
  if (!candidate) return undefined;
  let normalized: string;
  try { normalized = Intl.getCanonicalLocales(candidate)[0].toLowerCase(); } catch { return undefined; }
  const exact = locales.find(locale => locale.toLowerCase() === normalized);
  if (exact) return exact;
  const base = normalized.split('-')[0];
  return locales.find(locale => locale.toLowerCase().split('-')[0] === base);
}

export function selectLocale(bundle: I18nBundle, saved: string | null | undefined, languages: readonly string[]): string {
  const preferred = matchLocale(saved, bundle.locales);
  if (preferred) return preferred;
  for (const language of languages) {
    const matched = matchLocale(language, bundle.locales);
    if (matched) return matched;
  }
  return bundle.default_locale;
}

/** 语言方向不改变业务时区；脚本标记覆盖语言默认方向。 */
export function localeDirection(locale: string): 'ltr' | 'rtl' {
  try {
    const script = new Intl.Locale(locale).maximize().script;
    return script && ['Arab', 'Hebr', 'Thaa', 'Nkoo', 'Adlm', 'Rohg', 'Syrc', 'Samr', 'Mand'].includes(script) ? 'rtl' : 'ltr';
  } catch { return 'ltr'; }
}

export function localeLabel(locale: string): string {
  try { return new Intl.DisplayNames([locale], { type: 'language' }).of(locale) ?? locale; }
  catch { return locale; }
}
