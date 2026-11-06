# 独立编辑器类型包与同版发布

状态：已执行（本地实现与验证，未首次发布）；治理：不可变变更记录。影响范围：编辑器类型交付、CLI create、五个 Bun 示例与现有 main 发布接续。前置：[0038](0038-editor-types-and-jsx-runtime.md)、[0040](0040-example-typecheck-dependencies.md)、[0041](0041-examples-bun-package-manager.md)。替代 0038/0039 中 CLI 内嵌声明、types 命令和缓存 paths 的交付方式，不改写旧记录。

## 背景

此前应用要先执行 `kenvoy types`，把 CLI 内嵌的真实声明导出到 `.kenvoy/types`，再通过三个 paths 连接虚拟 SDK/JSX 模块。用户要求改为独立 npm 开发依赖，随 CLI 版本同步发布，不再依赖 CLI 生成编辑器声明；示例继续用 Bun，锁文件不提交。

用户选择合法包名 `@kenvoy/types`。`kenvoy@type` 是已有包名加 dist-tag 的语法，不作为另一个包名。

## 方案与实施

### 真实类型包与唯一版本源

- `@kenvoy/types` 只含声明、package.json、README 和 MIT LICENSE，不含 JavaScript、bin、生命周期脚本或 React 运行依赖。
- 声明由现有 `tsconfig.types.json` 从真实 SDK、生产/开发 JSX runtime 及传递依赖生成；包入口用环境模块连接 `@kenvoy/client` 和两个 JSX runtime，不手写另一套 SDK。
- 版本来自指定 CLI manifest 的真实 Cargo metadata，确认目标是 kenvoy Binary；不在模板另存一个版本号。React 类型 peer 范围来自现有前端精确开发依赖。
- 应用使用 `types: ["@kenvoy/types"]`、`jsx: "react-jsx"`、`jsxImportSource: "@kenvoy/client"`，不再配置三个缓存 paths。DSL、普通 React HTML/SVG、组件 props、事件和 SDK 参数的严格检查保留，普通 `React.JSX` 不受 DSL 污染。

### CLI 与五个示例切换

移除 `kenvoy types` 命令、输出报告、编辑器缓存管理模块、内嵌 EDITOR_TYPES 和旧专用缓存测试；不保留别名或 fallback。create 只在新目录生成 TypeScript 配置和私有开发清单，`@kenvoy/types` 精确版本取编译时 CLI 版本，不安装依赖、不创建声明缓存。

五个示例仍固定 Bun 1.4.3，包管理器、既有业务依赖与严格设置保持。源码开发先执行前端 `build:types`，生成实际 tarball 并同步五例该开发依赖；不改写 CLI 创建的独立用户项目。示例锁文件继续只在本地使用，不重新纳入 Git；旧本地 `.kenvoy/types` 可保留，但配置不再读取它。

本机实际遇到的 Bun 边界决定本地交付形式：

1. 目录型 file 依赖安装报 EPERM，同一文件的原生复制/硬链接操作成功；改用实际 npm tarball 安装通过，不修改系统权限或 Bun 配置。
2. 固定路径 tarball 从版本 0.0.1 换为 0.0.2 后，普通安装与 `--force` 都仍安装 0.0.1。内容寻址文件名、同步开发依赖后，普通安装实际更新到 0.0.2。
3. 因此本地产物为 `kenvoy-types-<CLI版本>-<SHA256前16位>.tgz`，`types-artifact.json` 记录完整摘要、版本与实际文件名。SDK 内容或 CLI 版本改变都换路径，避免复用旧 tarball；正式资产仍使用标准带版本文件名。
4. 并行首次安装同一 tarball 时，部分 Bun 进程报缓存目录 ENOENT；串行安装恢复成功，最终五例均串行检查通过。不把这个观察扩大为其他 Bun 版本或系统的结论。

### 正式发布同步与恢复

两个 main 正式入口和六个手动二进制打包入口不增加。CLI 源码包包含完整 `frontend/dist/types-package`；上传前检查实际 Cargo 归档中的类型目录和版本。

主发布从同版本已发布 CLI crate 下载并核对官方 checksum，安全解包后只对包内声明目录执行隔离凭据的 npm pack；不读取新 HEAD 重建旧版。六个 ZIP、类型 `kenvoy-types-X.Y.Z.tgz` 及各自侧车共 14 项齐全才公开 Release。已有类型 tarball 逐文件对照同版 crate；缺侧车只补原归档摘要，冲突停止，不覆盖或重传。

channels 只下载正式 Release 的六目标和类型资产，核对 GitHub 官方 digest、类型侧车及纯声明身份。npm 先发布或确认 `@kenvoy/types` 精确版本可见，再调用原严格 CLI 安装器发布器；两包独立跳过已存在版本。只有精确 HTTP 404 表示缺失，鉴权/限流/网络错误停止，写请求不自动重试。

当前源码版本继续为 CLI 0.4.4、KEngine 0.2.0，不机械递增，也不移动旧标签。既有已发布 0.4.4 crate 不含新目录，不能用本轮源码回填旧版本；首次正式类型发行需未来 CLI 新版本完整打包。

## 改动文件清单

| 范围 | 文件与改动 |
|---|---|
| 类型生产 | `crates/kenvoy/frontend/scripts/build-type-package.mjs`、`type-package-npm.mjs`；`frontend/types-package/package.template.json`、`index.d.ts`、`README.md`；前端 package.json |
| CLI 包含与内嵌 | `crates/kenvoy/Cargo.toml`、`build.rs`：包含完整 npm 声明目录及构建来源，移除内嵌编辑器声明 |
| CLI 接线 | `src/infrastructure/project/create.rs`、`project/mod.rs`、`src/interfaces/cli/args.rs`、`output.rs`、`src/bootstrap/commands.rs` |
| 移除旧实现 | `src/infrastructure/project/editor/{mod,cache,config,files,tests}.rs`；`tests/types_cli.rs` 与 `tests/types_cli/{support,boundaries}.rs` |
| 消费回归 | `frontend/tests/editor-types.test.mjs`、`tests/types/editor-positive.tsx`、`editor-negative.tsx`；`crates/kenvoy/tests/cli.rs` |
| 五例配置 | customer-app、extensions-app、options-app、uploads-app、i18n-app 各自的 package.json、tsconfig.json、README.md，以及 examples/README.md |
| 正式资产 | `.github/scripts/type_package.py`、`package_types.py`、`registry.py`、`release_plan.py`、`publish_crates.py`、`publish_release.py` |
| 渠道 | `.github/scripts/channels/assets.py`、`type_assets.py`、`publish_types.py`、`publish_npm.py`；现有安装器严格身份校验不放宽 |
| 发布回归 | `.github/scripts/tests/test_type_package.py`、`type_package_fixture.py`、`test_release_plan.py`、`test_release_assets.py`、`test_publish_crates_auth.py`；channels/tests/test_publish_types.py、test_type_assets.py |
| 工作流与规则 | `.github/workflows/release-one-version.yml`、`channels.yml`；AGENTS.md 的 npm 双包和同版来源规则 |
| 现状与索引 | 根/CLI README，docs/README，前端架构、组件契约、CLI 操作、工程流程、发布/渠道操作与进度，changes/CHANGELOG 与两个分类索引 |

## 后续验证建议与发行前提

- 新版本经用户确认后按既有标签/主分支规则发布；不能重新上传 0.4.4 或移动其标签。
- 首次发布前确认有权使用 `@kenvoy` scope，并为 `@kenvoy/types` 单独配置 `zerobiubiu/kenvoy`、`channels.yml` 的 Trusted Publisher，environment 留空并允许 npm publish。已有 `kenvoy` 授权不自动覆盖新包。
- 远端正式发行后再验证 registry 安装同版本类型和 CLI，核对真实 provenance 与消费者类型检查；本轮本地证据不代替该项。
- 本轮不启动业务应用、浏览器或示例数据库；没有对 UI、HTTP、迁移和远端 Actions 作新的验收声明。

## 执行验证记录

执行环境：Windows x64，仓库现有 Cargo/Node/npm，Bun 1.4.3、TypeScript 7.0.2、uv 管理的 Python 3.12。未安装新的系统工具、修改全局包管理器配置或放行依赖脚本。

| 命令或场景 | 实际结果与结论 |
|---|---|
| `npm run build:types`，随后定向执行最终包构建脚本 | 真实 SDK 声明生成成功；最终生成 `@kenvoy/types@0.4.4`、完整声明目录、本地 `kenvoy-types-0.4.4-0e231512d45077e0.tgz`、侧车与产物元数据，五例开发依赖自动同步 |
| `node --test tests/editor-types.test.mjs` | 最终 1 项通过；在干净应用安装实际 tarball，生产/开发 JSX 正向通过，非法 SDK/DSL/React 用法在预期位置产生真实诊断，不依赖本地声明缓存或 paths |
| 五例各自串行 `bun install --ignore-scripts --omit=peer --offline`、`bun run typecheck` | 最终全部退出 0，均安装内容寻址路径的 `@kenvoy/types@0.4.4`；未运行应用或访问数据库 |
| 实际 CLI 创建临时项目，PATH 指向无程序目录 | create 退出 0，生成开发依赖精确为 0.4.4、types 包接线，无 `.kenvoy`；用同版本真实 tarball 替换临时清单的未公开 registry 引用后，Bun 离线安装及 typecheck 均退出 0 |
| 固定路径与内容寻址 tarball 的 Bun 更新场景 | 固定路径从 0.0.1 改成 0.0.2，普通/force 安装仍为 0.0.1；内容寻址并更新开发依赖后，普通安装依次实际为 0.0.1、0.0.2；仅使用自有临时目录，已清理 |
| `cargo build -p kenvoy --offline --locked --message-format=json-render-diagnostics` | 构建退出 0，取得真实 compiler-artifact 二进制并用于上述创建场景；KEngine 增量缓存 finalize 出现 Windows os error 5 note，不影响本次编译，未改系统设置或关闭增量 |
| `cargo package -p kenvoy --offline --locked --allow-dirty --list` | 仅只读列包清单，退出 0；归一化 Windows 路径后确认完整类型目录 28 文件及两个构建脚本进入清单；没有上传或把该命令冒充正式源包编译验证 |
| Python source-release unittest discovery | 最终 33 项全部通过；包括真实 npm pack、Cargo 本机 registry 鉴权/外部 target、旧源包阻断、类型版本/内容错配、路径安全、不可变资产与缺侧车恢复 |
| Python channels unittest discovery | 80 项，79 通过、1 因 Windows 符号链接权限跳过；类型优先、精确 404、双摘要、错误停止、同版与部分恢复边界通过 |
| 两个受影响工作流的 `Bun.YAML.parse` | 解析成功；主发布保持 workflow_call，channels 保持 workflow_run；不代表远端事件调度或 OIDC 已验收 |
| `cargo fmt --all` | 退出 0 |
| `cargo xtask docs` | 通过：114 个 Markdown、1240 个本地链接；编码、链接与索引门禁通过 |
| `cargo xtask version-tags --apply` | 新增 0、保留 9，未推送或移动既有标签 |

初次 npm pack 因本机 npm 返回包名索引 JSON 对象而非数组失败，现同时识别两种形态并校验身份、版本、文件名。发布回归首次还发现侧车夹具的转义换行和 channels 测试目录导入遮蔽；已修复输入/导入问题、移除文案断言与重复路径的编排测试，最终结果如上，不重钉错误文案。

已知限制：类型包首次 npm 上传、scope 权限及新 Trusted Publisher、本轮远端 Actions 和跨平台实际安装均未执行；现有 CLI 的历史正式发行不作为新类型包的远端证据。
