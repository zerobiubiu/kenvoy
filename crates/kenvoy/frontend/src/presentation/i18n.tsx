import { createContext, useCallback, useContext, useEffect, useMemo, useState } from 'react';
import type { ReactNode } from 'react';
import { LOCALE_STORAGE_KEY, localeDirection, localeLabel, selectLocale } from '../domain/i18n/locale';
import { LEGACY_I18N } from '../domain/i18n/model';
import type { I18n, I18nBundle } from '../domain/i18n/model';
import { createI18n } from '../domain/i18n/translate';

export interface I18nContextValue extends I18n { setLocale: (locale: string) => void }
const I18nContext = createContext<I18nContextValue | null>(null);

/** 框架自动装配；语言偏好不访问 API，也不参与 Session、权限和 Schema 代际。 */
export function I18nProvider({ bundle = LEGACY_I18N, children }: { bundle?: I18nBundle; children: ReactNode }) {
  const [preference, setPreference] = useState<string | null>(() => {
    try { return window.localStorage.getItem(LOCALE_STORAGE_KEY); } catch { return null; }
  });
  const languages = typeof navigator === 'undefined' ? [] : navigator.languages;
  const locale = selectLocale(bundle, preference, languages);
  const i18n = useMemo(() => createI18n(bundle, locale), [bundle, locale]);
  const setLocale = useCallback((next: string) => {
    if (!bundle.locales.includes(next)) return;
    setPreference(next);
    try { window.localStorage.setItem(LOCALE_STORAGE_KEY, next); } catch { /* 禁用存储不阻止本页切换。 */ }
  }, [bundle.locales]);
  useEffect(() => {
    document.documentElement.lang = locale;
    document.documentElement.dir = localeDirection(locale);
  }, [locale]);
  const value = useMemo(() => ({ ...i18n, setLocale }), [i18n, setLocale]);
  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n(): I18nContextValue {
  const value = useContext(I18nContext);
  if (!value) throw new Error('useI18n 必须在 Kenvoy Runtime 内使用');
  return value;
}

export function LocaleSelector() {
  const { locale, locales, setLocale, t } = useI18n();
  if (locales.length < 2) return null;
  return <label className="kv-locale"><span>{t('ui.language')}</span>
    <select value={locale} onChange={event => setLocale(event.target.value)}>
      {locales.map(language => <option key={language} value={language} lang={language}>{localeLabel(language)}</option>)}
    </select>
  </label>;
}
