# Rust 工程入口与跨 Crate 测试

状态：参考（现状文档）。工程组织依据见 [ADR 0011](../decisions/0011-rust-test-workspace-and-xtask.md)，测试范围见[测试策略](testing-strategy.md)。

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
| `cargo xtask check` | Cargo 架构边界与文档校验 |
| `cargo xtask architecture` | 真实元数据、成员/目标、正常依赖闭包、发布限制 |
| `cargo xtask docs` | 文档 UTF-8、异常字符、空白、本地链接和索引可达性 |
| `cargo xtask build` | 构建默认前端，再构建 Kenvoy CLI |
| `cargo xtask test` | 先构建 CLI，再运行 Workspace 标准 Rust 测试 |
| `cargo xtask browser` | 先构建 CLI，再显式运行测试成员的浏览器用例 |

xtask 不自动安装工具或框架依赖。首次从源码构建仍需按[根 README](../../README.md)准备 npm 依赖；浏览器需要已有 Playwright/Chromium。内部 Cargo 命令使用 `--offline --locked`。CLI 路径从 Cargo 的 compiler-artifact 消息取得，并通过 `KENVOY_BINARY` 传给测试，支持不同 target 目录和构建 profile。

没有新增代码生成或发布命令：当前只实现已有实际需要的检查、构建与测试编排。

## 测试所有权与边界

`kenvoy-tests` 使用标准 `#[test]`/`#[tokio::test]`，通过 KEngine 公共 API 与真实 Kenvoy CLI 联合验证语言/业务模型，以及真实 HTTP、会话、权限、CRUD 和重启持久化。单独执行该成员测试时应先构建 CLI；可显式指定绝对 `KENVOY_BINARY`。

Rust 夹具拥有唯一临时目录、CLI 标准输入、监听端口和子进程；命令、启动、浏览器和关闭均有期限。清理只针对自己创建的目录与进程，不操作示例或用户数据库。Windows 上强制终止子进程只证明进程回收与重启持久化，不等同于真实 Ctrl+C 验收。

浏览器用例默认 ignored，只有显式命令才运行。前端 JavaScript/TypeScript 单元测试仍使用其原生测试工具；Playwright 脚本保留浏览器 API 和 DOM 断言，通过协议调用 Rust 夹具，不再在 Node 中管理 Kenvoy 进程。扩展和选项流程保留对临时项目已声明、锁定依赖的显式 `--install-deps` 验证；不会安装浏览器或修改用户项目。

## 已退役路径

原 tools 下的架构/文档检查已迁入 Rust xtask，HTTP 脚本已迁入 Rust 集成测试，Node 项目进程夹具由 Rust 替代。独立前端技术探针退出当前树，真实浏览器覆盖改用正式 Kenvoy 应用。

历史 ADR、变更与验证报告中的旧路径和旧命令保留为当时事实，不是现行执行入口。需要追溯旧源码时查看 Git 的 `1ccc2db` 及以前版本，当前维护入口统一为本页与[文档索引](../README.md)。
