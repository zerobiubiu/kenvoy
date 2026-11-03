# 产品版本日志

状态：参考；治理：持续维护的版本概要。详细执行事实保留在编号变更记录中，不以本页替代验证证据。

## 0.2.1

发布状态：待发布；仅 dev 本地提交，未合并 main、未推送。Kenvoy 为 0.2.1，KEngine 代码未变，保持 0.2.0。

### Kenvoy

- 补齐 `dev`/`run` 的优雅退出：启动构建可被终止信号取消；Unix 处理 SIGINT/SIGTERM，Windows 处理 Ctrl+C、Ctrl+Break、控制台关闭与注销。
- 首次信号后按期限排空 HTTP、停止开发 WebSocket、回收开发协调器/Vite、关闭 JavaScript worker 与 SQLite；第二次信号或 75 秒超期强制退出，避免关闭阶段永久挂起。
- Vite 控制与日志读取均有期限；正常关闭后删除本次实例独占的 `.kenvoy/client-dev` 镜像与缓存。
- 实施与验证见 [0025](0025-graceful-shutdown.md)。

### 工程与分发

- 工程流程切换为 dev/main 双分支：本地版本标签自动维护，main 只发布当前版本（含 PATCH），手动发布仅作 main 兜底，不再补发历史版本。
- 手动与正式分发共用单目标打包实现，修复根 LICENSE 输入及新归档名称对应的 binstall 元数据；既有 0.2.0 源包和 Release 资产不覆盖。
- 当时未递增产品版本；本次 0.2.1 统一包含该批工程改动，见 [0021](0021-dev-main-release-rules.md)。
- 修复根提交产品版本漏标：初始化 v0.0.0 与根声明的 v0.1.0 可共存，已有标签不移动；见 [0022](0022-root-version-tag.md)。
- 版本标签仅来自真实已提交版本，及时补到对应版本提交；退役 v0.0.0 初始化特例，增加切换、合并及推送前维护入口；见 [0023](0023-version-tag-rules.md)。
- 新增 npm / Homebrew / Scoop / WinGet / Chocolatey / AUR 与 APT/RPM/Alpine/Arch 仓库发布工作流，只消费已发布 Release 归档；见 [0024](0024-package-channels.md)。

## 0.2.0

发布状态：已发布，2026-11-02；两个 crates.io 源包及六目标预编译二进制均已上线，cargo-binstall 安装核验通过；记录：[0019](0019-crates-and-binary-release.md)。

### KEngine

- 从 0.1.0 的早期占位接口切换到通用 JS/TS/JSX/TSX 语言引擎，不沿用旧 `hello`/`VERSION` 用法。
- 提供 Engine、AnalysisSession、自有 Generic IR、位置与 UTF-16 保真字符串、表达式结构、模块图、作用域/符号/引用及跨模块绑定。
- 增量分析保留候选与最后有效结果，结构化报告语言和模块诊断，不执行用户代码或解释 Web 业务。

### Kenvoy

- 提供声明式 Application IR、资源/字段/页面/权限、真实 SQLite CRUD/迁移、管理员与 Session、受控同源 API。
- 默认原生 React/CSS 表单与后台，支持选项联动、跨资源选项、组件重载、自定义组件 SDK 和受限 JavaScript Action。
- 支持 SQLite 多文件字段、附件权限/绑定/清理、上传图片及 URL 图片展示。
- 提供 create/check/inspect/admin/migrate/build/dev/run、版本化产物、原生监听、受控 Vite/HMR 与错误恢复。
- 完成 DDD 内部分层及模块规模门禁；修复 Windows 密码中文提示和未声明根页的 Runtime 入口。

### 包与分发

- 两个产品版本均更新为 0.2.0，内部测试/工程成员不发布。
- 增加包级 README、MIT 许可证、元数据和精确打包边界；内置前端随源包发布，构建不依赖完整仓库布局。
- 增加 cargo-binstall 元数据、六目标交叉编译和 ZIP/SHA256 分发流程。
- 本次按用户要求不跑测试套件；实际编译、归档与安装核验范围以 0019 为准。

## 0.1.0

crates.io 已有的早期占位版本。归档代码与说明保留在线上版本，不覆盖重传。
