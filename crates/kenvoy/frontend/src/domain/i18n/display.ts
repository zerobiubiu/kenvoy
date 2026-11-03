import { isUploadedFiles } from '../files.ts';
import type { FieldDefinition } from '../types.ts';
import type { I18n } from './model.ts';

export type DisplayField = Pick<FieldDefinition, 'kind' | 'options'>;

/** 展示格式化不回写字段、草稿或 JSON；动态业务文本保持原文。 */
export function displayValue(value: unknown, field: DisplayField | undefined, i18n: I18n): string {
  if (value === null || value === undefined || value === '') return '—';
  if (typeof value === 'boolean') return i18n.t(value ? 'ui.yes' : 'ui.no');
  if (field?.kind === 'select') return field.options.find(option => option.value === value)?.label ?? String(value);
  if (field?.kind === 'file' && isUploadedFiles(value)) return value.length ? value.map(file => file.name).join(', ') : i18n.t('ui.files.empty');
  if (typeof value === 'number') return field?.kind === 'number' ? i18n.formatNumber(value, { maximumSignificantDigits: 21 }) : String(value);
  if (typeof value === 'string' && field?.kind === 'date') return i18n.formatDate(value);
  if (typeof value === 'string' && field?.kind === 'datetime') return i18n.formatDateTime(value);
  return String(value);
}

/** 约束提示与实际字段规则共用值，不在组件中维护另一份数值含义。 */
export function constraintHint(field: FieldDefinition, { t, formatNumber }: Pick<I18n, 't' | 'formatNumber'>): string {
  return field.kind === 'boolean' ? t('ui.field.boolean_hint')
    : field.kind === 'datetime' ? t('ui.field.datetime_hint')
    : field.kind === 'number' ? [t(field.integer ? 'ui.field.integer_hint' : 'ui.field.number_hint'),
      field.min != null ? t('ui.field.minimum', { min: formatNumber(field.min, { maximumSignificantDigits: 21 }) }) : '',
      field.max != null ? t('ui.field.maximum', { max: formatNumber(field.max, { maximumSignificantDigits: 21 }) }) : ''].filter(Boolean).join(t('ui.field.hint_separator'))
    : field.max_length != null ? t('ui.field.length', { max: formatNumber(field.max_length) }) : '';
}
