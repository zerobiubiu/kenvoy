# 0021：dev/main 双分支、自动标签与当前版本发布

状态：已执行；治理：不可变变更记录；日期：2026-11-03。范围：本仓库 Git 规则、标签维护、手动打包及产品发布；前置：[0019](0019-crates-and-binary-release.md)、[0020](0020-license-single-source.md)；现行规则：[发布操作](../operations/releasing.md)。本轮仅在本地 dev 实施，不推送、不合并 main、不发布新版本。

## 背景与确认

用户改为常驻 dev/main 双分支：开发统一在 dev，每次已提交的产品版本变化自动打本地 tag，用户决定发布时才合并 main。确认删除旧 `dev.x.y.z` 开发发布通道和全历史补发；main 当前版本包括 PATCH，手动发布只作 main 兜底。工作流文件两分支共享，但发布执行权限严格限制 main。

核对发现 dev 落后 main 45 个提交且可快进，工作树干净。用户明确允许本地同步/切换、不推送；已执行 `git switch dev`、`git merge --ff-only main`，从 `3e870f4` 开始实施，不重写任何提交历史。

共同根为 `7d3b5c2860d6e3c64948612a53d0df74ddf47883`。本轮补本地附注标签 `v0.0.0`，消息为 `0.0.0 init`；原提交消息、内容与 SHA 保持不变，既有 `v0.2.0` 标签不移动。

## 方案与实施

### Git 规则与自动标签

- `AGENTS.md`、README 入口及发布文档统一新模型、自动本地提交/打 tag 与需另行授权的推送/合并边界。
- 新增 Rust `cargo xtask version-tags [--apply] [--repo PATH]`：从 Git 已提交对象解析 TOML，默认只读预览；dev、完整历史、既有标签一致性检查通过后，以引用事务补缺失附注标签。
- 根旧 manifest 版本只作比较基线；已有合法同版本标签保留。未提交版本、非 dev、浅克隆、轻量/错版本/不在历史中的冲突标签不能触发静默修补。
- 新增 `.githooks/post-commit`，只在 dev 成功提交后补本地 tag；`.gitattributes` 固定钩子 LF。原仓库未配置 hooksPath，只有默认 sample 钩子，本轮将本地 `core.hooksPath` 设为 `.githooks`；不影响其他仓库，克隆后需另行启用。

### main 当前版本发布

- 删除 `distribution.yml`；新 `release.yml` 只监听 main 推送，`patch-release.yml` 改为 main 的 `confirm=yes` 手动兜底，两者调用同一 `release-one-version.yml`。
- 只检查当前 main SHA 和两个产品的精确版本，允许 PATCH；不扫描历史 tag 来发布。已被新 main 替代的运行或发布已齐的版本跳过。
- 先确认远端 tag、版本和 ancestry，再执行任何上传；存在新源包时要求 tag 源码树与 main 一致。缺 tag 在上传前失败，而不是到 Release 阶段才失败。
- 从仓库外精确 Git 快照构建前端、正常 Cargo dry-run/编译校验，按 KEngine → Kenvoy CLI 发布缺失源包；发布后的只读可见性与官方 checksum 核验不重试上传写请求。
- 二进制从真实 registry 源包构建。Release 缺资产时只补当前版本缺项；已有 ZIP 和标签不覆盖，缺侧车使用原 ZIP 的摘要。新 Release 在六目标资产完整后公开，标题为 `Kenvoy <version>`。
- GitHub/registry 查询分离认证域。调用方授予被调方所需权限；撤销旧说明中“reusable Secret 不可用”“job 级 write 不支持”的无充分依据结论，按 GitHub 官方权限继承契约处理。

### 单目标构建与分发契约

- `manual-package.yml` 从统一 JSON 选择一个系统/架构，调用共享 `.github/actions/package`；不带发布凭据、不创建 tag/Release，只保留 14 天 artifact。
- 正式与手动构建复用同一实现。Windows 在 Windows runner 用 MSVC/PowerShell，Linux 和 macOS 各用本系统 runner 与 Zig，保留同系统内跨架构；不在 Linux 上统一编译所有系统。
- 修复已删除 crate LICENSE 后残留的默认路径：checkout 使用根 LICENSE，registry 使用包内 LICENSE。
- `xtask dist` 新增可选 `--archive-name`，侧车采用实际 ZIP 名，拒绝路径、设备名和覆盖。更新未来版本 binstall 模板，使 `windows/linux/macos × x86_64/ARM64` 下载名与归档一致；不更名或重传线上 0.2.0 资产。

## 改动文件清单

- 规则和入口：`AGENTS.md`、`README.md`、`.gitattributes`、`.githooks/post-commit`。
- Git/归档工具：`xtask/src/commands/mod.rs`、`version_tags/`、`distribution/options.rs`、`distribution/archive.rs`、`xtask/Cargo.toml`、`Cargo.lock`。
- 打包元数据：`crates/kenvoy/Cargo.toml`、`.github/distribution-targets.json`。
- Actions：删除 `.github/workflows/distribution.yml`；新增 `release.yml` 与 `.github/actions/package/action.yml`；重写 `release-one-version.yml`、`patch-release.yml`、`manual-package.yml`。
- CI 协调：`.github/scripts/registry.py`、`package_source.py`、`release_common.py`、`release_plan.py`、`publish_crates.py`、`publish_release.py` 及发布策略行为回归。
- 文档：发布操作、工程入口、版本日志、文档/运维/变更/验证索引及本记录。历史已执行正文未回写。

## 执行验证记录

环境：Windows x64；本机 Rust 1.98；既有 actionlint 1.7.7。根据仓库工程依赖授权，以 uv 安装 Python 3.12.15 供本地标准库脚本验证，不更换系统 Python。

| 验证 | 实际结果与结论 |
|---|---|
| `cargo update --workspace --offline` | `Locking 0 packages`；仅同步 xtask 新增的现有 TOML 依赖关系 |
| `cargo fmt --package xtask`；`cargo test --package xtask --offline --locked commands::` | 退出 0，14 项通过；包含真实临时 Git 仓库的标签创建/保留/冲突/分支/浅历史回归及实际归档、侧车和拒绝覆盖 |
| `cargo run --package xtask --offline --locked -- version-tags` / `--apply` | 真实仓库保留 v0.0.0、v0.2.0，缺失 0；新增 0，未移动标签、未推送 |
| `uv run --no-project --python 3.12 python -B -m unittest discover -s .github/scripts/tests -v` | 退出 0，10 项通过：当前 PATCH 可发布、dev 拒绝、旧 main 跳过、缺 tag/源码漂移阻断、完整版本空跑、缺资产/侧车处理及校验错误 |
| actionlint 检查四个工作流 | 退出 0，无诊断；不能据此声称远端 Actions 已运行 |
| 临时 Python 驱动实际 `package_source.py select` 与 `xtask dist` | 六次选择均只有一个目标；复用既有 0.2.0 六目标真实二进制，生成新命名 ZIP，实际核对根 LICENSE、包内三文件和侧车 SHA256；x86_64 Windows 额外执行版本输出，其余只校验格式/架构 |
| checkout/registry 源码准备与恶意 tar 探针 | 两模式均找到正确 LICENSE；真实 .crate 官方 checksum 通过且含预构建前端；越界 tar 被拒绝 |
| 当前远端 main 的只读发布计划 | 本地模拟 main 事件，GitHub 通过已登录 gh 发 GET、registry 读取真实公开状态；版本 0.2.0、`active=false`、构建矩阵为空，无上传动作 |
| 精确提交 `3e870f4` 的独占源码快照与源包 dry-run | npm 安装 33 包，Vite 115 modules 构建成功；首次 Cargo `--offline` 因缺 registry 元数据失败，保留已完成前端，仅去掉 offline 重试失败步骤 |
| `cargo publish --dry-run --locked --package kengine --package kenvoy`（独占快照，复用本机 target 缓存） | 两包正常打包、独立编译校验通过；kengine 58 文件、kenvoy 242 文件；两次明确输出 `aborting upload due to dry run`，没有重新发布 0.2.0 |

LSP references 启动请求报 `server exited unexpectedly (code 0)`，已报告工具问题；使用定向调用搜索与 Rust 编译/行为测试替代，没有以失效的 LSP 当作验证通过。

## 已知限制与后续验证

- 本轮用户明确不推送；新规则与实现只提交在本地 dev，尚未进入远端 main，未触发新的 GitHub 发布运行。
- 六目标归档冒烟使用已存在的真实发行二进制，不是本轮重新完成六平台编译，更不代表跨架构运行验收；没有改动 kenvoy 项目或业务数据库。
- 源包 dry-run 使用已经提交的 `3e870f4` 快照验证快照/前端/打包路径；不将它声称为未提交产品版本的真实上传。
- 后续获得发布授权后，先推所需 tag，再合并并推 main；观察无缺项跳过、首次发布与中断后的手动兜底。若 main 已更新，旧运行应停止而不是补发历史版本。
- 本地 hooksPath 不随 Git 克隆迁移；新机器按发布文档启用后再验证自动标签。标签不自动推送。

最终执行 `cargo fmt --package xtask -- --check` 与 `cargo run --package xtask --offline --locked -- check` 均退出 0：四成员与依赖方向通过；350 个自有源码文件、286 个 Rust 文件、最大 449 行；90 个 Markdown、924 个本地链接通过。
