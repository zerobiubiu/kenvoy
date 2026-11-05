import type { FunctionComponent, ReactNode } from 'react';
import type { ActionDefinition, FieldKind, JsonScalar, OptionsSource, Role } from '../domain/types';
import type { ImageProps } from './images';
import type { FieldBinding, FormBinding, GroupBinding, ImageBinding, PageBinding, TableBinding } from './runtime/bindings';

interface LeafProps { children?: never }
interface ContainerProps { children?: ReactNode }
interface NamedProps { name: string }

/** 属性与编译器的静态 DSL 一致；值域和跨资源约束仍由 Kenvoy 编译器校验。 */
interface FieldProps<Value> extends LeafProps, NamedProps {
  label?: string;
  required?: boolean;
  default?: Value | null;
}
interface IndexedFieldProps<Value> extends FieldProps<Value> { index?: boolean }
interface TextFieldProps extends IndexedFieldProps<string> { maxLength?: number; unique?: boolean }
interface NumberFieldProps extends IndexedFieldProps<number> {
  integer?: boolean;
  min?: number;
  max?: number;
  unique?: boolean;
}
interface SelectFieldProps extends Omit<IndexedFieldProps<string>, 'children'>, ContainerProps {
  optionsSource?: Readonly<Omit<OptionsSource, 'filters'>> & { filters?: Readonly<OptionsSource['filters']> };
}
interface FileFieldProps extends FieldProps<readonly []> {
  accept?: readonly string[];
  maxSize?: number;
  maxFiles?: number;
}
interface OptionProps extends LeafProps {
  value: string;
  label?: string;
  when?: Readonly<Record<string, JsonScalar>>;
}
interface PageProps extends ContainerProps {
  path: string;
  title?: string;
  resource?: string;
  permission?: Role;
}
interface FormProps extends LeafProps {
  resource: string;
  mode?: 'create' | 'edit';
  fields?: readonly string[];
}
interface TableProps extends LeafProps { resource: string; columns?: readonly string[] }
interface GroupProps extends ContainerProps { label?: string }
interface ItemProps extends LeafProps { label: string; path: string }
interface ActionProps extends LeafProps, NamedProps {
  // handler 可引用不同签名的导出函数；参数和结果由函数自身及调用方的类型负责。
  handler: (...args: never[]) => unknown;
  runtime: ActionDefinition['runtime'];
  permission: Role;
}

type OverrideBindings = Record<FieldKind, FieldBinding> & {
  image: ImageBinding;
  form: FormBinding;
  table: TableBinding;
  group: GroupBinding;
  page: PageBinding;
};
/** 重载名决定实际收到的绑定，不把各系统组件的 props 混为一个宽泛对象。 */
type OverrideProps = {
  [Name in keyof OverrideBindings]: LeafProps & {
    name: Name;
    component: FunctionComponent<OverrideBindings[Name]>;
  };
}[keyof OverrideBindings];

export interface DslIntrinsicElements {
  app: NamedProps & ContainerProps;
  page: PageProps;
  resource: NamedProps & ContainerProps;
  text: TextFieldProps;
  textarea: TextFieldProps;
  number: NumberFieldProps;
  boolean: IndexedFieldProps<boolean>;
  date: IndexedFieldProps<string>;
  datetime: IndexedFieldProps<string>;
  select: SelectFieldProps;
  file: FileFieldProps;
  option: OptionProps;
  form: FormProps;
  table: TableProps;
  image: ImageProps & LeafProps;
  action: ActionProps;
  navigation: GroupProps;
  item: ItemProps;
  group: GroupProps;
  override: OverrideProps;
}
