# 0039：五个示例的编辑器类型生成

状态：已执行；范围：examples 下五个 kenvoy 项目的编辑器配置与本地派生声明。前置记录：[0038：编辑器 SDK 类型与 JSX Runtime](0038-editor-types-and-jsx-runtime.md)。未安装 npm 依赖、未启动应用、未执行迁移或类型检查；产品版本和已有标签保持不变。

## 背景与方案

按用户要求，为 customer-app、extensions-app、options-app、uploads-app、i18n-app 执行现有 `kenvoy types`。复用本仓库已有 debug CLI，不改生成器、不重编译产品，也不手写 SDK 声明。

缺失配置由命令创建；已有 package.json 原字节保留。`.kenvoy/types` 为受摘要清单保护的本机派生缓存，不提交到 Git；重新克隆或清理缓存后需再次生成。

## 实施文件与结果

| 示例 | 本机声明文件 | 新增 tsconfig.json | package.json |
|---|---:|---|---|
| customer-app | 24 | 是 | 新建私有开发类型依赖清单 |
| extensions-app | 24 | 是 | 保留原清单 |
| options-app | 24 | 是 | 保留原清单 |
| uploads-app | 24 | 是 | 新建私有开发类型依赖清单 |
| i18n-app | 24 | 是 | 新建私有开发类型依赖清单 |

入库内容为五份 `examples/<示例>/tsconfig.json`、customer-app/uploads-app/i18n-app 的 package.json，以及示例 README 和文档索引。本轮未修改业务源码、kenvoy.toml、语言文件、既有锁文件、迁移或数据。

当前配置使用原生 SDK 的 JSX 类型入口，示例入口说明同步明确要求 dev 源码构建的 CLI；已发布 0.4.4 尚不含 types 命令及对应 JSX 子入口。不为此擅自递增产品版本或改写应用兼容字段。

## 执行验证记录

环境：Windows x64，使用本仓库已有 `target/debug/kenvoy.exe`。通过进程参数直接逐个调用以下命令：

```text
D:/kenvoy/target/debug/kenvoy.exe --project D:/kenvoy/examples/customer-app types
D:/kenvoy/target/debug/kenvoy.exe --project D:/kenvoy/examples/extensions-app types
D:/kenvoy/target/debug/kenvoy.exe --project D:/kenvoy/examples/options-app types
D:/kenvoy/target/debug/kenvoy.exe --project D:/kenvoy/examples/uploads-app types
D:/kenvoy/target/debug/kenvoy.exe --project D:/kenvoy/examples/i18n-app types
```

五次均退出 0，状态均为 `declarations_generated`；每次 total/created 均为 24，updated/removed 均为 0，`dependency_installation_performed` 均为 false。

执行前后以 SHA-256 对照各示例已有 src、migrations、data、配置、语言文件、.gitignore、package.json、package-lock.json 和已有 tsconfig。customer-app、extensions-app、options-app、uploads-app、i18n-app 分别核对 7、14、11、11、11 个已有文件，原字节均保持，无新增业务/迁移/数据文件。

逐例解析新 tsconfig，确认 strict 和原生 JSX 入口；按生成 manifest 核对每份声明的实际 SHA-256，每例 24 份均匹配。以上仅证明生成与文件保护，不冒充 TypeScript 编译通过。

完整命令报告及核对摘要保存在忽略目录 `artifacts/examples-types-generation/`，派生声明保留在五个示例的 `.kenvoy/types/`。

## 限制与后续建议

- 本轮没有安装依赖，没有执行 npm run typecheck，不声称编辑器依赖已齐备或应用全部类型检查通过。
- extensions-app、options-app 的原 package.json 没有被自动补写；后续按 types 报告合并开发类型依赖与 typecheck 脚本，保留原第三方依赖，再显式安装和检查。
- 其余三个示例已具备新建的开发依赖清单，可由使用者显式执行 npm install --ignore-scripts 后再运行类型检查。
- 不需要额外安装占位 @kenvoy/client 或重复 React 运行包；操作契约见[编辑器类型](../operations/cli-design.md#编辑器类型)。
