# Rust 工程入口与跨 Crate 测试

状态：参考（现状文档）。工程成员依据见 [ADR 0011](../decisions/0011-rust-test-workspace-and-xtask.md)，内部代码组织依据见 [ADR 0012](../decisions/0012-ddd-layers-and-module-boundaries.md)，测试范围见[测试策略](testing-strategy.md)。

## Workspace 与入口

```text
Cargo.toml
.cargo/config.toml          # cargo xtask 别名
crates/
  kengine/                 # 产品 Library，保留独立 tests
  kenvoy/                  # 产品 Binary，保留独立 tests 和前端工程
  kenvoy-tests/             # 不发布的跨 Crate 测试与 Rust 夹具
xtask/                     # 不发布的 Rust 工程编排
```

产品只依赖产品层：kenvoy → kengine；测试和 xtask 不进入产品依赖闭包。默认成员为两个产品包，`--workspace` 包括四个成员。没有通过测试重组开放 Kenvoy 的私有业务模块。

## 工程命令

| 命令 | 执行范围 |
|---|---|
| `cargo xtask check` | Cargo/源码分层、目录与文件规模，以及文档校验 |
| `cargo xtask architecture` | 真实元数据、成员/目标、依赖闭包、发布限制、源码 AST 边界与 500 行硬上限 |
| `cargo xtask docs` | 文档 UTF-8、异常字符、空白、本地链接和索引可达性 |
| `cargo xtask build` | 构建默认前端，再构建 Kenvoy CLI |
| `cargo xtask test` | 先构建 CLI，再运行 Workspace 标准 Rust 测试 |
| `cargo xtask browser` | 先构建 CLI，再显式运行测试成员的浏览器用例 |
| `cargo xtask dist --binary PATH --target TRIPLE --version VERSION --output DIR` | 校验已有二进制的目标架构，生成 ZIP 与 SHA256；兼容 host 可做限时版本核验，不执行测试套件 |
| `cargo xtask version-tags` | 只读预览 dev 中已提交版本的缺失附注标签，不读取未提交版本、不推送 |
| `cargo xtask version-tags --apply` | 在 dev 将缺失标签补到对应版本首次出现的提交，保留既有合法标签；不生成初始化标签、不推送或改写历史 |

xtask 不自动安装工具或框架依赖。首次从源码构建仍需按[根 README](../../README.md)准备 npm 依赖；浏览器需要已有 Playwright/Chromium。内部 Cargo 命令使用 `--offline --locked`。CLI 路径从 Cargo 的 compiler-artifact 消息取得，并通过 `KENVOY_BINARY` 传给测试，支持不同 target 目录和构建 profile。

前端 `npm run build` 先生成 Vite 资产，再通过 `tsconfig.types.json` 从真实 SDK 与两个 JSX runtime 根生成 `dist/types` 声明闭包；Cargo 构建要求两类产物齐全，发布源包也包含这些声明。不要仅运行 Vite 或手写一份 SDK 声明替代该步骤。编辑器消费回归由 `frontend/tests/editor-types.test.mjs` 使用真实 TypeScript 执行正反向检查；项目生成与保护边界由 `kenvoy/tests/types_cli.rs` 验证。


dist 仅做已有程序的归档与验收，不隐式发布到 registry 或 GitHub；发布与交叉编译流程见[发布操作](../operations/releasing.md)。正式对外操作由发布操作者在对应门禁完成后执行。

开发统一使用 dev，只有用户批准合并 main 才进入发布。[标签维护入口](../../.githooks/maintain-version-tags)由提交、切换回 dev、合并完成及推送前钩子复用；首次克隆需按[Git 与发布操作](../operations/releasing.md)启用本仓库 hooksPath。轻量 CI 协调脚本位于 [.github/scripts](../../.github/scripts/)，与本地 Rust 工程入口分工，不恢复 tools 或额外产品 Workspace 成员。

### npm 分发安装器

安装器的工程依赖与锁文件独立位于 `.github/scripts/channels/npm_runtime/`，不进入 React Runtime 或 Cargo 产品依赖。准备依赖时只用 `npm ci --prefix .github/scripts/channels/npm_runtime --ignore-scripts --no-audit --no-fund`，不运行未审查的依赖脚本。

生成器与发布器运行 `uv run --no-project --python 3.12 python -B -m unittest discover -s .github/scripts/channels/tests -v`。Node 行为套件运行 `node --test .github/scripts/channels/tests/npm_runtime.test.cjs .github/scripts/channels/tests/npm_runtime_network.test.cjs .github/scripts/channels/tests/npm_runtime_runner.test.cjs`；Windows 原生 Ctrl+C 夹具可通过 `KENVOY_TEST_PYTHON` 指定已准备的 Python 3 路径。

发布前还需对真实生成 tarball 执行匿名隔离安装、CLI 运行与摘要校验；npm 12 注册表全局安装仅为本包添加 `--allow-scripts=kenvoy`，本地 tarball 则匹配精确 `file:` 来源，局部安装通过 `npm install-scripts approve` 授权。不能以安装退出码 0 代替 postinstall 已执行。实际首发不属于测试，必须单独授权。细节见[渠道操作](../operations/distribution-channels.md)。

### 源码包与 Release 发布回归

运行 `uv run --no-project --python 3.12 python -B -m unittest discover -s .github/scripts/tests -v`。真实 Cargo 用例需要已准备的 Cargo/Git，使用独占 Cargo Home、本机索引与本机 HTTP 服务，验证默认仓库鉴权、上传前/后摘要及外部 target 目录；不会使用真实令牌或向公网发布测试包。Release 用例覆盖草稿创建、分页定位、既有 ZIP 复用与完整侧车。依据和边界见 [0035](../changes/0035-cargo-package-checksum-recovery.md)。

## 代码层次与拆分

产品和前端按[DDD 目录规范](../architecture/ddd-structure.md)组织；xtask 使用 `checks/commands`，测试支撑使用 `process/fixture/protocol`，不为工程编排创建业务领域层。

新增/迁移模块必须同步调用方、测试过滤路径、Rust 内嵌前端清单及开发镜像。自有 Rust/TS/TSX/MJS/CSS（含测试）每文件硬上限 500 物理行，目标 400 行内；按职责拆分，不以旧路径 shim 或巨型 shared 文件绕过约束。

结构门禁使用 syn 2.0.119 的 `full/visit` 解析 Rust，前端 import/re-export/动态导入复用 KEngine 公共语法 API；字符串和注释不是依赖证据。syn 与 xtask 对 KEngine 的依赖只用于工程检查，不进入产品正常依赖闭包。本次迁移、规则负例及真实行为验证见 [0016](../changes/0016-ddd-structure-and-module-split.md)。

## 测试所有权与边界

`kenvoy-tests` 使用标准 `#[test]`/`#[tokio::test]`，通过 KEngine 公共 API 与真实 Kenvoy CLI 联合验证语言/业务模型，以及真实 HTTP、会话、权限、CRUD 和重启持久化。单独执行该成员测试时应先构建 CLI；可显式指定绝对 `KENVOY_BINARY`。

Rust 夹具拥有唯一临时目录、CLI 标准输入、监听端口和子进程；命令、启动、浏览器和关闭均有期限。清理只针对自己创建的目录与进程，不操作示例或用户数据库。Windows 上强制终止子进程只证明进程回收与重启持久化，不等同于真实 Ctrl+C 验收。

浏览器用例默认 ignored，只有显式命令才运行。前端 JavaScript/TypeScript 单元测试仍使用其原生测试工具；Playwright 脚本保留浏览器 API 和 DOM 断言，通过协议调用 Rust 夹具，不再在 Node 中管理 Kenvoy 进程。扩展和选项流程保留对临时项目已声明、锁定依赖的显式 `--install-deps` 验证；不会安装浏览器或修改用户项目。

## 已退役路径

原 tools 下的架构/文档检查已迁入 Rust xtask，HTTP 脚本已迁入 Rust 集成测试，Node 项目进程夹具由 Rust 替代。独立前端技术探针退出当前树，真实浏览器覆盖改用正式 Kenvoy 应用。

历史 ADR、变更与验证报告中的旧路径和旧命令保留为当时事实，不是现行执行入口。需要追溯旧源码时查看 Git 的 `1ccc2db` 及以前版本，当前维护入口统一为本页与[文档索引](../README.md)。
