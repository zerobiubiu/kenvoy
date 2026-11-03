import type { I18nFormatters } from './model.ts';

/** 格式器仅作用于展示，日历日期固定 UTC，日期时间由调用方明确选择时区。 */
export function createFormatters(locale: string): I18nFormatters {
  const numbers = new Map<string, Intl.NumberFormat>();
  const dates = new Map<string, Intl.DateTimeFormat>();
  const dateFormatter = (options: Intl.DateTimeFormatOptions) => {
    const key = JSON.stringify(Object.entries(options).sort(([left], [right]) => left.localeCompare(right)));
    let formatter = dates.get(key);
    if (!formatter) { formatter = new Intl.DateTimeFormat(locale, options); dates.set(key, formatter); }
    return formatter;
  };
  const parseDate = (value: string | Date): Date | null => {
    if (value instanceof Date) return Number.isFinite(value.getTime()) ? value : null;
    if (!/^\d{4}-\d{2}-\d{2}$/.test(value) || value.startsWith('0000-')) return null;
    const date = new Date(`${value}T00:00:00.000Z`);
    return Number.isFinite(date.getTime()) && date.toISOString().slice(0, 10) === value ? date : null;
  };
  return {
    formatNumber: (value, options = {}) => {
      if (!Number.isFinite(value)) return String(value);
      const key = JSON.stringify(Object.entries(options).sort(([left], [right]) => left.localeCompare(right)));
      let formatter = numbers.get(key);
      if (!formatter) { formatter = new Intl.NumberFormat(locale, options); numbers.set(key, formatter); }
      return formatter.format(value);
    },
    formatDate: (value, options = {}) => {
      const date = parseDate(value);
      if (!date) return String(value);
      const formatting: Intl.DateTimeFormatOptions = options.dateStyle !== undefined || options.timeStyle !== undefined
        ? { ...options, timeZone: 'UTC' }
        : { year: 'numeric', month: '2-digit', day: '2-digit', ...options, timeZone: 'UTC' };
      return dateFormatter(formatting).format(date);
    },
    formatDateTime: (value, options = {}) => {
      // 无时区的文本不由显示层猜测浏览器时区。
      const date = value instanceof Date ? value : /(?:Z|[+-]\d{2}:\d{2})$/i.test(value) ? new Date(value) : null;
      if (!date || !Number.isFinite(date.getTime())) return String(value);
      const formatting: Intl.DateTimeFormatOptions = options.dateStyle !== undefined || options.timeStyle !== undefined
        ? { timeZone: 'UTC', ...options }
        : { year: 'numeric', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit', second: '2-digit', timeZone: 'UTC', ...options };
      return dateFormatter(formatting).format(date);
    },
  };
}
