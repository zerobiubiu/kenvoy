import { createContext, useCallback, useContext, useMemo } from 'react';
import type { FormEvent, ReactNode } from 'react';
import { ApiError } from './api';
import { useRuntime } from './context';
import { immutable } from './options';
import { validateRecord } from './validation';
import type { FieldDefinition, JsonScalar, JsonValue, ListQuery, OptionsQuery, PageDefinition, RecordData, ResourceDefinition, SelectOption } from './types';

/** 所有字段重载遵循同一控制协议。draft 是 DOM 编辑文本，value 是 JSON 标量。 */
export interface FieldBinding {
  field: Readonly<FieldDefinition>;
  resource?: Readonly<ResourceDefinition>;
  formId?: string;
  value: JsonScalar;
  /** false 时 value 为 null，编辑原文仍在 draft；这不表示已清空或可以提交。 */
  valid: boolean;
  draft: string;
  values: Readonly<Record<string, JsonScalar>>;
  valuesValidity: Readonly<Record<string, boolean>>;
  error?: string;
  disabled: boolean;
  options: readonly SelectOption[];
  optionsLoading: boolean;
  optionsError?: string;
  optionsReady: boolean;
  optionsPage: number;
  optionsTotal: number;
  optionsPageSize: number;
  optionsSearch: string;
  onChange: (value: JsonScalar) => void;
  onDraftChange: (value: string) => void;
  searchOptions: (value: string) => void;
  loadOptionsPage: (page: number) => void;
  reloadOptions: () => void;
  children?: ReactNode;
}

export interface FormBinding {
  resource: Readonly<ResourceDefinition>;
  mode: 'create' | 'edit';
  formId?: string;
  fields: readonly string[];
  values: Readonly<Record<string, JsonScalar>>;
  validity: Readonly<Record<string, boolean>>;
  errors: Readonly<Record<string, string>>;
  busy: boolean;
  saved: Readonly<RecordData> | null;
  submit: (event: FormEvent) => Promise<void>;
  children: ReactNode;
}
export interface TableBinding {
  resource: Readonly<ResourceDefinition>;
  records: readonly Readonly<RecordData>[];
  columns: readonly string[];
  busy: boolean;
  page: number;
  pageSize: number;
  total: number;
  refresh: () => void;
  children: ReactNode;
}
export interface GroupBinding { label: string | null; children: ReactNode }
export interface PageBinding { page: Readonly<PageDefinition>; children: ReactNode }

export const FieldContext = createContext<FieldBinding | null>(null);
export const FormContext = createContext<FormBinding | null>(null);

/** 在系统字段重载中取得原生字段绑定，不创建第二份数据模型。 */
export function useField(name?: string): FieldBinding {
  const binding = useContext(FieldContext);
  if (!binding || (name !== undefined && binding.field.name !== name)) throw new Error('useField 必须在对应字段重载内使用');
  return binding;
}
export function useForm(): FormBinding {
  const binding = useContext(FormContext);
  if (!binding) throw new Error('useForm 必须在资源表单内使用');
  return binding;
}

/** 自定义组件使用应用公开投影与同一 API；服务端仍执行最终授权和校验。 */
export function useResource(name: string) {
  const { application, client } = useRuntime();
  const resource = application.resources.find(item => item.name === name);
  if (!resource) throw new Error(`资源 ${name} 不在当前用户的应用描述中`);
  const definition = useMemo(() => immutable(resource), [resource]);
  const validated = useCallback((data: Record<string, JsonValue>, mode: 'create' | 'patch') => {
    const result = validateRecord(definition, data, mode);
    if (!result.ok) throw new ApiError('字段校验未通过', { status: 422, fields: result.errors });
    return data;
  }, [definition]);
  return useMemo(() => ({
    definition,
    permissions: immutable(application.permissions.filter(policy => policy.target.kind === 'resource' && policy.target.name === name)),
    list: (query?: ListQuery) => client.list(name, query),
    get: (id: number) => client.get(name, id),
    create: (data: Record<string, JsonValue>, formId?: string) => client.create(name, validated(data, 'create'), formId),
    update: (id: number, data: Record<string, JsonValue>) => client.update(name, id, validated(data, 'patch')),
    delete: (id: number) => client.delete(name, id),
    options: (field: string, query: OptionsQuery, formId?: string) => client.options(name, field, query, formId),
  }), [definition, application.permissions, name, client, validated]);
}
