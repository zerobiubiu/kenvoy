# 产品版本日志

状态：参考；治理：持续维护的版本概要。详细执行事实保留在编号变更记录中，不以本页替代验证证据。

## 0.2.0

发布状态：两个 crates.io 源包已发布，预编译二进制构建中；记录：[0019](0019-crates-and-binary-release.md)。

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
