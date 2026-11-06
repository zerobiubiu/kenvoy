# @kenvoy/types

Kenvoy CLI 的独立编辑器类型包，版本与 CLI 一致。包内包含真实客户端 SDK、React HTML/SVG 与 Kenvoy DSL 的声明，不包含 JavaScript、React 运行包、安装脚本或 CLI 二进制。

## 使用

在 kenvoy 项目中将 `@kenvoy/types` **精确锁定到 Kenvoy CLI 的相同版本**，并显式安装 `typescript`、`@types/react`、`@types/react-dom` 为开发依赖。React 类型须满足本包的 peer 范围；React 运行时继续由 Kenvoy CLI 提供。

在 `tsconfig.json` 中配置：

```json
{
  "compilerOptions": {
    "module": "ESNext",
    "moduleResolution": "Bundler",
    "target": "ES2022",
    "lib": ["ES2022", "DOM", "DOM.Iterable"],
    "strict": true,
    "noEmit": true,
    "jsx": "react-jsx",
    "jsxImportSource": "@kenvoy/client",
    "types": ["@kenvoy/types", "react", "react-dom"]
  },
  "include": ["src"]
}
```

`react-jsxdev` 同样可用。不要为这三个虚拟模块添加 `paths`；不需要生成或保留 `.kenvoy/types`。

业务代码仍从 `@kenvoy/client` 导入 API：

```tsx
import { useField } from '@kenvoy/client';
import type { FieldBinding } from '@kenvoy/client';

export function Title(binding: FieldBinding) {
  const field = useField(binding.field.name);
  return <input value={field.draft} onChange={event => field.onDraftChange(event.currentTarget.value)} />;
}
```

`@kenvoy/client`、`@kenvoy/client/jsx-runtime` 与 `@kenvoy/client/jsx-dev-runtime` 由本包提供类型，由 Kenvoy CLI 提供运行值。`@kenvoy/types/sdk`、`@kenvoy/types/jsx-runtime` 和 `@kenvoy/types/jsx-dev-runtime` 仅是声明入口，不能作为 JavaScript 运行依赖。内部声明文件不是公开子路径；DSL 标签仅存在于 Kenvoy JSX 命名空间，不修改普通 `React.JSX`。
