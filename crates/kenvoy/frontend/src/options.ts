import type { FieldDefinition, FieldError, FieldValue, JsonScalar, ResourceDefinition, SelectOption } from './types.ts';
import { normalizeInput, validateField } from './validation.ts';

export interface BindingValue {
  value: FieldValue;
  valid: boolean;
  errors: FieldError[];
}

/** 草稿只参与显式转换；非法草稿留在编辑状态，不能冒充有效 JSON 字段值。 */
export function normalizeBindingValue(field: FieldDefinition, draft: unknown): BindingValue {
  const result = validateField(field, normalizeInput(field, draft));
  return result.ok
    ? { value: result.value as FieldValue, valid: true, errors: [] }
    : { value: null, valid: false, errors: result.errors };
}

/** 选项依赖只取声明的本资源字段，不允许组件拼接任意源表条件。 */
export function optionDependencies(field: FieldDefinition): string[] {
  return [...new Set([
    ...Object.values(field.options_source?.filters ?? {}),
    ...field.options.flatMap(option => Object.keys(option.when ?? {})),
  ])];
}

export function dependencyValues(field: FieldDefinition, values: Readonly<Record<string, FieldValue>>): Record<string, JsonScalar> {
  return Object.fromEntries(optionDependencies(field).map(name => [name, Array.isArray(values[name]) ? null : values[name] ?? null]));
}

export function dependenciesReady(field: FieldDefinition, values: Readonly<Record<string, FieldValue>>): boolean {
  return optionDependencies(field).every(name => values[name] !== undefined && values[name] !== null && !Array.isArray(values[name]));
}

export function staticOptions(field: FieldDefinition, values: Readonly<Record<string, FieldValue>>, resource?: ResourceDefinition, validity?: Readonly<Record<string, boolean>>): SelectOption[] {
  return field.options.filter(option => Object.entries(option.when ?? {}).every(([name, value]) => {
    if (Array.isArray(values[name]) || resource?.fields.some(item => item.name === name && item.kind === 'file')) return false;
    if (validity?.[name] === false) return false;
    const dependent = resource?.fields.find(item => item.name === name);
    if (!dependent) return values[name] === value;
    const expected = validateField(dependent, value);
    const actual = validateField(dependent, values[name]);
    return expected.ok && actual.ok && expected.value === actual.value;
  }));
}

/** 传递清空下游字段，不能仅刷新选项后仍提交上一父值对应的子值。 */
export function dependentFields(resource: ResourceDefinition, changed: string): string[] {
  const affected = new Set<string>([changed]);
  let added = true;
  while (added) {
    added = false;
    for (const field of resource.fields) if (!affected.has(field.name) && optionDependencies(field).some(name => affected.has(name))) {
      affected.add(field.name); added = true;
    }
  }
  affected.delete(changed);
  return [...affected];
}

/** 每次运行模型独立冻结；用户组件不能修改框架持有的原始 Schema 或选项。 */
export function immutable<T>(value: T): T {
  if (value === null || typeof value !== 'object') return value;
  const copy = Array.isArray(value)
    ? value.map(item => immutable(item))
    : Object.fromEntries(Object.entries(value).map(([key, item]) => [key, immutable(item)]));
  return Object.freeze(copy) as T;
}
