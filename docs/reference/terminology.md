# 称呼与术语

状态：参考（现状文档，规则性约定）。本文规定 Kenvoy 相关名称的唯一含义，避免“项目”“产品”在对话、文档和代码注释中指代不同对象。返回[文档总索引](../README.md)或[参考目录](README.md)。

## 核心区分

| 称呼 | 唯一含义 | 不是什么 |
|---|---|---|
| **kenvoy 项目**（kenvoy project） | 使用者用本产品规定的 DSL（`jsx`/`tsx`）编写、含 `kenvoy.toml` 的应用项目 | 不是本仓库，不是 Workspace，不是 `crates/kenvoy` |
| **Kenvoy CLI** | `crates/kenvoy` 这个产品包与其二进制 `kenvoy` | 不是用户应用，不是 KEngine |
| **KEngine** | `crates/kengine` 这个 Library 包 | 不承载 Web 业务、数据库、HTTP、认证、React |
| **KenvoySpace**（`kspace`） | 规划中的企业数字化云工作空间（控制台/Dashboard），闭源 | 当前未实现，不是现有 Cargo 成员，没有接口或部署方式 |
| **Kenvoy-Runtime**（`krt`） | 规划中的生产调度中枢（路由分发、CI/CD、版本、日志、多服务协同），闭源 | 当前未实现，不等同 `kenvoy run` 或现有构建流水线 |
| **本仓库 / 本 Workspace** | 这个 Git 仓库及其四成员 Workspace（`kengine`、`kenvoy`、`kenvoy-tests`、`xtask`） | 不是用户的应用项目 |

## 使用规则

- 指用户应用时写 **kenvoy 项目**（首次出现可写“kenvoy 项目（用户应用）”），已有文档中的“用户项目”“应用项目”与它同义，可直接替换。
- 指产品二进制或 CLI 行为时写 **Kenvoy CLI**，需要精确指向源码时写 `crates/kenvoy`。
- 指语言引擎时写 **KEngine**，需要精确指向源码时写 `crates/kengine`。
- 指当前开发对象时写 **本仓库** 或 **本 Workspace**，不用“本项目”“Kenvoy 项目”等可能被误读为产品仓库之外用户的说法。
- 代码、配置与命令中的标识符保留英文原样（`kenvoy`、`kengine`、`kenvoy.toml`、`--project`），不受本文中文称呼约束。

## 典型歧义与正确写法

| 容易含混的说法 | 正确写法 |
|---|---|
| “Kenvoy 的配置” | kenvoy 项目的配置（`kenvoy.toml`）；若要指产品配置代码，写 `crates/kenvoy` 的配置模块 |
| “本项目的数据目录” | 若指用户应用数据：kenvoy 项目的数据目录；若指仓库内文件：本仓库的 `data/` |
| “Kenvoy 项目发布” | 发布 Kenvoy CLI（`crates/kenvoy`）与 KEngine（`crates/kengine`） |
| “把项目根传给 KEngine” | 把 kenvoy 项目根传给 KEngine；KEngine 不发现 kenvoy 项目、不读取 `kenvoy.toml` |
| “项目数据库” | kenvoy 项目的 SQLite 数据库；Kenvoy CLI 不删除、不重建该库 |

## 关联文档

- 仓库结构与成员职责见[整体架构](../architecture/architecture.md)、[Crate 职责](../architecture/crate-boundaries.md)。
- kenvoy 项目的配置、路径与 DSL 入口见[项目配置](../contracts/project-configuration.md)、[已实现声明语法](../contracts/dsl-specification.md)。
- 命令与运行方式见[CLI 使用](../operations/cli-design.md)。

## 维护

称呼变化时先改本文与 [AGENTS.md](../../AGENTS.md) 的规则条目，再逐步统一正文；历史变更与决策记录中的旧称呼保留为当时事实，不因改名重写。
