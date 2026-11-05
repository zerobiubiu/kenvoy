# 完整发布与六个手动打包入口

状态：已执行（本地实现与验证，未推送、未触发远端工作流）。影响范围：GitHub Actions 入口、执行规则和操作文档。前置：[0035](0035-cargo-package-checksum-recovery.md)。CLI 保持 0.4.4，KEngine 保持 0.2.0；不创建、移动或推送版本标签。

## 背景与明确边界

用户重新确认：dev 不自动发布；main 推送自动发布全部平台和保留渠道；另有一个仅 main、只能手动触发且范围完全相同的正式发布入口；手动打包拆为六份，允许所有分支，每次只打一个目标，产物仅供下载，不对外发布。

原配置存在两处差异：手动正式发布完成后不会自动接续三渠道；手动打包只有一个平台/架构选择入口。本轮只修正这些边界，不新增发布源、PR/定时流程、版本输入或额外人工阶段。

## 实施

### 两个完整发布入口

- 自动入口保留 `release.yml`，仅 push main；dev 和 tag 推送不触发。
- `patch-release.yml` 改名为 `manual-release.yml`，只保留 workflow_dispatch，保留既有 main 与 confirm=yes 要求；不保留旧入口别名。确认步骤在 main 上对非 yes 明确失败，正式发布依赖该步骤，避免依赖“全部跳过”的汇总状态来决定渠道接续；不增加用户输入。
- 两个入口共用 `release-one-version.yml` 和正式发布并发组，继续使用既有当前版本、标签、源包、六平台归档与恢复逻辑。
- `channels.yml` 只作为内部 workflow_run 后续，同时监听「main 最新版本自动发布」「main 最新版本手动发布」。仅接受成功的 push/workflow_dispatch 父运行，并检查 main、本仓库来源。
- 删除渠道自身的 workflow_dispatch、版本输入与回退分支。固定 checkout 父运行 head_sha，版本从该源码的 Cargo.toml 读取。操作者不再需要单独启动渠道。
- 保留 `channels.yml` 文件名、npm id-token 权限和三渠道 job，不改现有 OIDC 绑定或凭据。

完整发布在 GitHub 上仍分为主发布与内部渠道接续两个运行；无需第二次人工操作，但完成状态须以主发布及三个渠道都成功为准。

### 六个独立手动打包入口

| 入口 | 固定目标 |
|---|---|
| `manual-package-windows-x86_64.yml` | Windows x86_64 |
| `manual-package-windows-arm64.yml` | Windows ARM64 |
| `manual-package-linux-x86_64.yml` | Linux x86_64 |
| `manual-package-linux-arm64.yml` | Linux ARM64 |
| `manual-package-macos-x86_64.yml` | macOS x86_64 |
| `manual-package-macos-arm64.yml` | macOS ARM64 |

每个入口仅 workflow_dispatch、contents:read，无自定义参数、分支限制或发布凭据。平台/架构由入口固定，源码由 GitHub 的分支选择决定。

旧 `manual-package.yml` 改名为 `package-one-target.yml`，仅 workflow_call，接受内部必填 platform/arch 字符串。六个入口共用已有 select/build、统一目标 JSON、`package_source.py` 与 composite action；继续 source-mode:checkout、单目标构建、ZIP/SHA256 artifact 和 14 天保留策略，不增加发布操作。

最终共 11 个 YAML，其中业务入口恰好 8 个；其余 3 个分别是共享主发布、内部渠道接续、共享单目标打包，不提供独立人工入口。

## 改动文件

- 工作流：`patch-release.yml` → `manual-release.yml`，`manual-package.yml` → `package-one-target.yml`，新增上述六个手动打包入口，修改 `channels.yml`；`release-one-version.yml` 仅更新入口说明注释。
- 执行规则：AGENTS.md 固定两个完整发布与六个打包入口的边界。
- 现状文档：发布操作、渠道操作、渠道进度、文档总入口及 CHANGELOG。
- 记录索引：changes、verification 与本文。

既有目标 JSON、composite action、产品源码、Cargo manifests/lock、发布 Python 业务实现未改动。历史 0021/0024 等记录保留原事实，由索引说明替代关系。

## 执行验证记录

环境：Windows x64；Bun YAML 解析器；已有 actionlint 1.7.12（项目 artifacts 内的 Windows amd64 二进制）；uv 管理的 Python 3.12；来源守卫在已有 WSL Ubuntu 24.04 中显式运行 Bash。未安装新依赖。

| 验证命令或场景 | 实际结果与结论 |
|---|---|
| `target/debug/xtask.exe version-tags --repo D:/kenvoy` | 只读预览，9 个既有标签保留，缺失 0，未作修改 |
| `artifacts/channel-rollout/tools/actionlint/actionlint.exe` | 全部工作流无诊断，退出 0；工具版本 1.7.12 |
| 临时 YAML 结构与入口谓词检查 | 11 文件、8 业务入口、6 独立打包入口；72 个正式入口事件/分支/确认上下文检查通过 |
| 临时渠道接续谓词检查 | 288 个父名称、事件、结论、分支组合通过；只允许两个成功正式入口，打包入口不进入渠道 |
| 实际执行 channels 首步 Bash 来源守卫 | 6 场景符合预期：本仓库 main 允许；dev ref、tag ref、非 main 父分支、外仓、缺失父信息拒绝 |
| 实际执行手动确认 Bash 守卫 | yes 返回 0，no 与空值返回 1；正式发布依赖确认步骤，非 main 不执行确认或发布 |
| 实际执行 `package_source.py select --platform ... --arch ...` | 6 目标 × main/dev/feature 上下文共 18 次，均只输出一个正确目标、对应 runner 与当前版本 0.4.4 |
| `uv run --no-project --python 3.12 python -B -m unittest discover -s .github/scripts/tests -v` | 14 项既有发布行为回归全部通过 |
| `cargo run --package xtask --offline --locked -- check` | 架构、规模及文档通过：437 个自有源码文件，最大 449 行；108 份 Markdown、1,177 条本地链接 |

来源守卫最初的本机探针使用了歧义 `bash` 命令，实际进入 WSL 转发器，测试环境变量未传入；该次失败不算工作流失败或通过证据。改为 `wsl --exec env ... bash -c` 显式注入后，六个实际守卫场景全部符合预期，未修改工作流门禁来迎合探针。

临时谓词校验仅验证当前简单布尔表达式及配置对应关系，不作为新增永久的源码/接线断言测试。18 次选择器验证没有切换 Git 分支或真正执行跨平台编译；它验证的是对应入口传参和实际选择器行为。

## 生效与后续验收边界

本轮没有推送、合并到 main、创建 Release、上传软件包或触发 GitHub Actions。新增入口须在获得授权后同步到 GitHub 才生效；既有 0.4.4 的发行成功不等于新入口已完成远端验收。

后续获准运行时，分别核验自动/手动 main 的成功接续、失败/取消/未确认不进入渠道、非 main 发布阻断，以及六个打包入口在选定分支仅产生自身 artifact。不要为验证入口而发布测试版本、恢复已取消渠道或增加额外工作流。
