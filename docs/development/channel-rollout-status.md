# 渠道发布进度与待办

状态：参考（现状文档）；治理：持续维护的进度页。当前仅保留 npm、Homebrew、Scoop；crates.io 与 GitHub Release 主发布流程继续保留。规则见[渠道操作](../operations/distribution-channels.md)，npm 单包切换及真实首发过程见 [0033](../changes/0033-single-npm-package-download.md)。

## 当前发布路径

| 项 | 当前状态 |
|---|---|
| npm 包形态 | 只生成和发布 `kenvoy`；安装时按系统/架构下载一个对应 ZIP，不再发布或依赖平台分包 |
| npm 首发版本 | `kenvoy@0.2.0` 已公开，latest 指向 0.2.0；注册表 SHA-1/SHA-512 与冻结包一致，Windows x64/Linux x64 匿名隔离安装及实际运行通过 |
| 旧 Windows 分包 | 仅 `kenvoy-win32-x64` 曾成功发布，含 0.0.0-stage、0.2.0；用户已授权撤销，两次撤销认证超时，最后实查仍存在 |
| 其它旧分包 | Windows ARM64 发布已停止，其余平台分包未上传 |
| GitHub / crates.io | 既有 0.2.0 正式发行保持不动；本轮不创建新 GitHub Release 或发布 crate |
| Homebrew / Scoop | 保留生成及自动发布路径，本轮未推送对应辅助仓库 |
| 已退役渠道 | APT、RPM、Alpine、Arch、WinGet、Chocolatey、AUR；不恢复专用实现或凭据 |
| 工作流 | prepare/npm/homebrew/scoop；npm job 只上传生成的 `kenvoy/` |
| 源码版本 | CLI 0.4.3，KEngine 0.2.0；项目格式仍 0.4.0，新建最低 CLI 取创建时版本 |
| 本轮 Git 操作 | 最终收口后维护本地 v0.4.3；不移动旧标签、不推送或合并 main |

取消平台分包和 Linux 软件源不等于取消平台支持：Windows、Linux、macOS 的 x64/ARM64 官方 ZIP 继续保留。安装包约定与具体测试证据以 0033 为准，不以 npm install 退出码 0 推定安装脚本已执行。

## npm 安装与脚本审批

npm 12 默认阻止未获准的依赖安装脚本。全局安装应显式仅批准本包：

```text
npm install -g kenvoy --allow-scripts=kenvoy
kenvoy --version
```

局部项目使用 `npm install-scripts approve kenvoy` 后执行 `npm rebuild kenvoy`；全局重建使用 `npm rebuild -g kenvoy --allow-scripts=kenvoy`。不修改用户全局策略，不放行所有依赖，也不让运行入口偷偷绕过脚本审批联网。

## 凭据与 OIDC

- 仓库仍保留 `CARGO_TOKEN`、`PACKAGES_TOKEN`；不再需要 WinGet、GPG/APK 或 AUR 凭据。旧 `kenvoy-packages` 仓库和本地恢复副本不动。
- 本次手动首发由用户本机终端完成 npm CLI 认证及提交，不使用 ego，不在聊天中收取密码或验证码；退出 0/HTTP 202 不等于正式版本已公开，须核验精确版本及摘要。
- 首发成功后只给 `kenvoy` 配置一个 Trusted Publisher：GitHub 用户 `zerobiubiu`、仓库 `kenvoy`、工作流文件 `channels.yml`，environment 留空；允许直接 `npm publish`，不能仅允许 staging。
- OIDC 由用户自行配置。本轮不替用户修改 npm 账号设置，也不把 npm whoami 成功当成 OIDC 已可用。

## 后续自动发布

完成并验证本轮后，先核对待推送 main 的树与 `v0.4.3^{tree}` 一致，再按发布规则推所需标签与 main。主发布成功后自动运行三个保留渠道，不扫描或补发历史源码版本。

标签之后如果又改变了树，需重新处理版本安排；不得静默移动标签或放宽整树门禁。0.2.0 npm 首发与尚未公开的 0.4.3 源码是不同状态，安装命令不会自动获得未发布功能。
