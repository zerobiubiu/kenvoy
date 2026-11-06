# 0040：五个示例的开发类型安装与检查

状态：已执行；范围：examples 的开发依赖、锁文件、CSS 类型环境及静态 JSX 组合。前置记录：[0039：示例类型生成](0039-example-editor-types.md)。用户已明确授权为全部五个示例安装开发类型并验证。本轮未启动应用、未执行数据库迁移，不更改产品版本或既有标签。

## 背景与方案

0039 只生成了声明及缺失配置，没有安装 npm 依赖，因此用户仍看到找不到 React 类型的诊断。按用户确认，为全部示例安装与框架一致的 TypeScript 7.0.2、@types/react 19.3.0、@types/react-dom 19.3.0，并为 extensions-app、options-app 补 typecheck 脚本，保留原第三方依赖。

安装使用 include=dev、omit=peer、ignore-scripts；省略自动 peer 运行包但不跳过版本约束解析，不使用 legacy-peer-deps。依赖来自已有 npm 缓存，未执行依赖安装脚本。extensions-app、options-app 原先自动安装的 React peer 副本被 npm 移除；React 运行时继续由 Kenvoy 统一提供。

## 真实类型诊断与修复

安装后 customer-app、i18n-app 直接通过。其余项目出现此前被缺失类型遮住的真实错误：

- 三个带全局样式的示例出现 TS2882：CSS 副作用导入没有类型声明。
- extensions-app 的 CustomerFields 默认导出静态 JSX 值，但 `<CustomerFields />` 要求可调用的 React 组件，出现 TS2604/TS2786。

在三个示例分别加入独立的 `src/env.d.ts`，精确声明全局 CSS 仅有副作用、没有 JavaScript 导出；不引入 any、不关闭检查。反向检查确认不存在的 CSS 导出仍会报错。

extensions-app 改用 `{CustomerFields}` 插入已有静态 JSX 值，不把它伪装成函数，不新增类型断言或执行 Schema 函数。改动前后对真实 CLI inspect 产生的客户资源模型去除 source 定位字段后做深比较，结果完全一致；check 仍通过，包含一个资源、一个页面、一个动作。

## 实施文件

- extensions-app、options-app 的 package.json：增加 typecheck 脚本与精确开发类型依赖，原 dependencies 不变。
- 五个示例的 package-lock.json：保留已有锁文件并更新，新建此前缺失的三份锁文件。
- `examples/extensions-app/src/env.d.ts`、`examples/options-app/src/env.d.ts`、`examples/uploads-app/src/env.d.ts`：CSS 副作用类型环境。
- `examples/extensions-app/src/app.tsx`：静态 JSX 值改用插值。
- 示例总览、扩展示例 README、文档中心及变更/验证索引：同步安装、检查及复现命令。

开工时 i18n-app 的 `src/I18nDemo.tsx` 和 `src/app.tsx` 已有用户暂存改动。两文件未被修改、覆盖或纳入本次提交；i18n 的检查针对用户当前工作区代码，不能将其暂存内容冒充本次源码修改。

## 执行验证记录

环境：Windows x64，Node v24.22.0，npm 12.2.0。五个目录分别执行：

```text
npm install --offline --save-dev --save-exact --include=dev --omit=peer --ignore-scripts --no-audit --no-fund typescript@7.0.2 @types/react@19.3.0 @types/react-dom@19.3.0
```

五次均退出 0；每例新增五个开发相关包，extensions-app、options-app 另各移除一个自动 peer 包。没有修改全局 npm 设置。

最终逐例执行：

```text
npm --ignore-scripts run typecheck -- --pretty false
```

| 示例 | 最终 typecheck 退出码 | 错误对照退出码 | 根 node_modules/react |
|---|---:|---:|---|
| customer-app | 0 | 1 | 不存在 |
| extensions-app | 0 | 1 | 不存在 |
| options-app | 0 | 1 | 不存在 |
| uploads-app | 0 | 1 | 不存在 |
| i18n-app | 0 | 1 | 不存在 |

正反向检查过程：

1. 在每例实际 include 范围内加入普通文件名 `src/tmp-kenvoy-typecheck-control.ts`，分别引用不存在的 SDK 类型并把字符串赋给 number。
2. 用该例安装的真实 TypeScript 及原 tsconfig 检查，五例均退出 1，均出现 TS2305 与 TS2322，证明不是空检查或 any 消红。
3. 三个 CSS 示例另引用不存在的 stylesheet 导出，均得到额外 TS2305，证明 CSS 声明没有开放虚假的 JavaScript API。
4. 删除所有临时控制文件后，五次 npm typecheck 均再次退出 0。

额外执行 `kenvoy --project examples/extensions-app check`：退出 0，输出一个资源、一个页面、一个动作。资源 inspect 的前后 JSON 比较通过；这是实际 DSL/模型烟测，不是源码字符串断言。

SHA-256 核对确认：原第三方 dependencies 相同；除本轮 package/lock 与 extensions-app 的预期插值修改外，原有源码、配置、语言文件、迁移和数据字节未变；用户 i18n 暂存源码也保持。未更改 strict 等编译选项，没有新增诊断抑制或任意 JSX 标签索引。

本地证据保存在忽略目录 `artifacts/examples-typecheck/`：五例真实错误对照输出、最终核对汇总、资源模型前后 JSON。node_modules 保留供编辑器使用，临时错误文件已删除。

## 限制与后续建议

- 本轮未直接操作 VS Code。若仍显示旧诊断，可重启 TypeScript Server 或重新打开对应应用目录；实际 TypeScript 检查已通过。
- SDK 声明仍是 `.kenvoy/types` 派生缓存，重新克隆或清理后先执行 types；随后可用 `npm ci --include=dev --omit=peer --ignore-scripts` 按锁文件安装。
- 本轮不启动这五个已有应用、不写业务库，不将类型检查当作浏览器或数据库运行验收。SDK 与运行时能力的先前验证见 [0038](0038-editor-types-and-jsx-runtime.md)。
