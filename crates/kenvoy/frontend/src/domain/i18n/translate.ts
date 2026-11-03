import { createFormatters } from './format.ts';
import { resolveLocale } from './locale.ts';
import type { I18n, I18nBundle, MessageParams } from './model.ts';
import { SYSTEM_MESSAGES } from './messages.ts';

function systemLocale(locale: string): 'zh-CN' | 'en-US' | undefined {
  const base = locale.toLowerCase().split('-')[0];
  return base === 'zh' ? 'zh-CN' : base === 'en' ? 'en-US' : undefined;
}

export function createI18n(bundle: I18nBundle, requestedLocale: string): I18n {
  const locale = resolveLocale(requestedLocale, bundle.languages) ?? bundle.fallback;
  const language = systemLocale(locale) ?? systemLocale(bundle.fallback) ?? 'zh-CN';
  const defaultLanguage = systemLocale(bundle.fallback) ?? 'zh-CN';
  const dictionaries = [[bundle.messages[locale], locale], [bundle.messages[bundle.fallback], bundle.fallback],
    [SYSTEM_MESSAGES[language], language], [SYSTEM_MESSAGES[defaultLanguage], defaultLanguage]] as const;
  const plurals = new Map<string, Intl.PluralRules>();
  const t = (key: string, params: MessageParams = {}, fallback?: string): string => {
    const count = typeof params.count === 'number' && Number.isFinite(params.count) ? params.count : undefined;
    let message: string | undefined;
    for (const [dictionary, dictionaryLocale] of dictionaries) {
      if (!dictionary) continue;
      let category: string | undefined;
      if (count !== undefined) {
        let rules = plurals.get(dictionaryLocale);
        if (!rules) { rules = new Intl.PluralRules(dictionaryLocale); plurals.set(dictionaryLocale, rules); }
        category = rules.select(count);
      }
      for (const variant of category ? [`${key}.${category}`, `${key}.other`, key] : [key]) {
        if (Object.hasOwn(dictionary, variant)) { message = dictionary[variant]; break; }
      }
      if (message !== undefined) break;
    }
    return (message ?? fallback ?? key).replace(/\{([A-Za-z_][A-Za-z0-9_]*)\}/g, (placeholder, name: string) =>
      Object.hasOwn(params, name) ? String(params[name] ?? '') : placeholder);
  };
  return { locale, locales: bundle.languages.map(language => language.code), t, ...createFormatters(locale) };
}
