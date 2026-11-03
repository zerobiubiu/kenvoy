import { createContext, useCallback, useContext, useEffect, useMemo, useState } from 'react';
import type { ReactNode } from 'react';
import { localeDirection, resolveLocale, selectLocale } from '../domain/i18n/locale';
import { LOADING_I18N } from '../domain/i18n/model';
import type { I18n, I18nBundle, LanguageOption } from '../domain/i18n/model';
import { createI18n } from '../domain/i18n/translate';

export interface I18nContextValue extends I18n { setLocale: (locale: string) => void }
type RuntimeI18n = I18nContextValue & { languages: readonly LanguageOption[] };
const I18nContext = createContext<RuntimeI18n | null>(null);

/** 每次装载只采集设备默认语言；手动选择仅在本页生效，不重载组件或访问存储。 */
export function I18nProvider({ bundle = LOADING_I18N, children }: { bundle?: I18nBundle; children: ReactNode }) {
  const [deviceLanguage] = useState<string | undefined>(() => {
    try { return typeof navigator === 'undefined' ? undefined : navigator.language; } catch { return undefined; }
  });
  const [preference, setPreference] = useState<string | null>(null);
  const locale = resolveLocale(preference, bundle.languages) ?? selectLocale(bundle, deviceLanguage);
  const i18n = useMemo(() => createI18n(bundle, locale), [bundle, locale]);
  const setLocale = useCallback((next: string) => {
    const code = resolveLocale(next, bundle.languages);
    if (code) setPreference(code);
  }, [bundle.languages]);
  useEffect(() => {
    document.documentElement.lang = locale;
    document.documentElement.dir = localeDirection(locale);
  }, [locale]);
  const value = useMemo(() => ({ ...i18n, languages: bundle.languages, setLocale }), [i18n, bundle.languages, setLocale]);
  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

function useRuntimeI18n(): RuntimeI18n {
  const value = useContext(I18nContext);
  if (!value) throw new Error('useI18n 必须在 Kenvoy Runtime 内使用');
  return value;
}

export function useI18n(): I18nContextValue {
  return useRuntimeI18n();
}

export function LocaleSelector() {
  const { locale, languages, setLocale, t } = useRuntimeI18n();
  if (languages.length < 2) return null;
  return <label className="kv-locale"><span>{t('ui.language')}</span>
    <select value={locale} onChange={event => setLocale(event.target.value)}>
      {languages.map(language => <option key={language.code} value={language.code} lang={language.code}>{language.display}</option>)}
    </select>
  </label>;
}
