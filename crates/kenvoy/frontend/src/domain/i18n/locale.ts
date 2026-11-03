import type { I18nBundle, LanguageOption } from './model.ts';

/** 页内手动选择接受配置代码或别名，查词始终使用配置中的原始代码。 */
export function resolveLocale(candidate: string | null | undefined, languages: readonly LanguageOption[]): string | undefined {
  if (typeof candidate !== 'string' || !candidate) return undefined;
  const normalized = candidate.toLowerCase();
  return languages.find(language => language.code.toLowerCase() === normalized || language.alias?.toLowerCase() === normalized)?.code;
}

/** 设备默认值仅精确匹配代码；不猜测基础语言、别名或浏览器的其他偏好。 */
export function selectLocale(bundle: I18nBundle, deviceLanguage: string | null | undefined): string {
  if (typeof deviceLanguage !== 'string') return bundle.fallback;
  const normalized = deviceLanguage.toLowerCase();
  return bundle.languages.find(language => language.code.toLowerCase() === normalized)?.code ?? bundle.fallback;
}

/** 语言方向不改变业务时区；脚本标记覆盖语言默认方向。 */
export function localeDirection(locale: string): 'ltr' | 'rtl' {
  try {
    const script = new Intl.Locale(locale).maximize().script;
    return script && ['Arab', 'Hebr', 'Thaa', 'Nkoo', 'Adlm', 'Rohg', 'Syrc', 'Samr', 'Mand'].includes(script) ? 'rtl' : 'ltr';
  } catch { return 'ltr'; }
}
