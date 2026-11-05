import type { JSX as ReactJSX } from 'react';
import type { DslIntrinsicElements } from './dsl-types';

/** 同名标签择一使用原生 React 或 DSL 属性，不能混入另一分支的专用属性。 */
type AlternateProps<Native, Dsl> =
  | (Native & { [Name in Exclude<keyof Dsl, keyof Native>]?: never })
  | (Dsl & { [Name in Exclude<keyof Native, keyof Dsl>]?: never });

type KenvoyIntrinsicElements = Omit<ReactJSX.IntrinsicElements, keyof DslIntrinsicElements> & {
  [Name in keyof DslIntrinsicElements]: Name extends keyof ReactJSX.IntrinsicElements
    ? AlternateProps<ReactJSX.IntrinsicElements[Name], DslIntrinsicElements[Name]>
    : DslIntrinsicElements[Name];
};

/** 仅供 @kenvoy/client 的自动 JSX runtime 使用，不扩充全局或 React.JSX。 */
export declare namespace JSX {
  type ElementType = ReactJSX.ElementType;
  type Element = ReactJSX.Element;
  type ElementClass = ReactJSX.ElementClass;
  type ElementAttributesProperty = ReactJSX.ElementAttributesProperty;
  type ElementChildrenAttribute = ReactJSX.ElementChildrenAttribute;
  type LibraryManagedAttributes<Component, Props> = ReactJSX.LibraryManagedAttributes<Component, Props>;
  type IntrinsicAttributes = ReactJSX.IntrinsicAttributes;
  type IntrinsicClassAttributes<Element> = ReactJSX.IntrinsicClassAttributes<Element>;
  type IntrinsicElements = KenvoyIntrinsicElements;
}
