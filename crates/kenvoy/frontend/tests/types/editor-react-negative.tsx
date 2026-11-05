/** @jsxImportSource react */
import type { FieldBinding } from '@kenvoy/client';

// 即使已经导入 SDK，普通 React 的 JSX 命名空间也不应获得 DSL 标签。
const field = <number name="amount" />; // @error react-namespace-isolation
export type Binding = FieldBinding;
