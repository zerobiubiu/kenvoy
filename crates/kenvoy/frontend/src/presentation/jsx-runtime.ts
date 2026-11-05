/** 运行时仍复用 Kenvoy 提供的 React；只替换编辑器使用的 JSX 类型入口。 */
export { Fragment, jsx, jsxs } from 'react/jsx-runtime';
export type { JSX } from './jsx-types';
