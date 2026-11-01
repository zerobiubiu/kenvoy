# ADR 0009：版本化产物与 Rust 管理的开发适配

状态：已采用；影响范围：Kenvoy build/run/dev；日期：2026-11-01；前置：[ADR 0006](0006-react-csr-runtime.md)、[ADR 0007](0007-rust-http-runtime.md)。

## 背景与问题

通用 React Runtime 已能解释 Application IR，自定义组件仍需要 TSX 转换、依赖解析和 React Refresh。CLI 必须独立管理服务，同时保护 SQLite、最后有效模型和旧构建产物。单独使用 Rolldown API 不提供完整开发服务器。

## 可选方案

1. 把用户项目交给外部 Vite Server 托管全部 HTTP：增加第二个公开服务，并分散认证、API 与生命周期管理。
2. Rust 内重写完整 npm/TSX/HMR 工具链：维护成本过高，重复成熟工具能力。
3. Axum 始终作为公开宿主；生产服务静态产物，开发模式由受控内部 Vite 提供客户端转换与 HMR。

## 选择与原因

采用方案 3。基础 DSL Runtime 编入 CLI，基础项目不需要 Node。存在客户端扩展时，Kenvoy 使用项目内锁定的框架工具缓存；只有显式 --install-deps 才安装必要依赖，不改写用户 package.json，不加载用户 Vite/PostCSS 配置或 .env。

生产输出放入 `dist/versions/<build_id>/`，含 Application IR、client 和 server modules。manifest 记录格式版本、框架指纹、输入指纹、文件大小/SHA-256 及依赖版本。先写独立 staging，校验后封存，再原子切换 manifest。产物加载核验路径、哈希、限额与格式；不清空未知输出目录，不碰独立数据目录。没有源码时可只用配置、产物、CLI 和 SQLite 启动。

开发模式仍由 Rust notify 和 AnalysisSession 决定更新。源码与 Schema 错误不会进入客户端镜像。完整候选准备后，在 HttpRuntime 发布屏障内完成安全迁移、模型、静态资产和 JavaScript 的切换，再提交 Vite 镜像并激活清单。数据库提交之后的收尾失败明确报告；开发代理失败可退回已验证静态资源。

Vite 只监听随机 loopback 端口，访问必须具有内部随机密钥，浏览器通过 Axum 同源 HTTP/WebSocket 代理访问。代理不转发用户 Cookie/Authorization，不允许任意目标地址；原始 Schema、server 与项目私有文件不进入镜像。React preamble 使用外部脚本以符合 CSP。

## 已知代价与限制

- 自定义 UI 的开发和重新构建需要 Node/npm；生产已构建产物不需要。
- 普通函数组件可通过 React Refresh 保留状态，结构性 DSL 更新可重新挂载，混合导出不保证状态保留。
- 候选更新执行完整客户端验证构建；语言解析缓存是增量的，尚不追求完整构建图的最小重打包。
- 不可变历史版本暂不自动删除，部署方需管理保留与磁盘容量。
- SHA-256 是完整性校验，不是对不可信发布者的签名认证；项目代码与产物仍属于可信部署输入。
- 本轮平台验收为 Windows；其他平台需独立验证文件替换、native binding 与退出行为。

## 架构影响与验证

以上能力全部在 Kenvoy 内实现，不增加 Crate 或 KEngine 业务依赖。实现位置为 `build.rs`、`artifact.rs`、`lifecycle.rs`、`client_development.rs` 与 `http/development.rs`。正式命令、浏览器更新和退出结果由[验证索引](../verification/README.md)记录；本决策不以适配器单测代替完整浏览器验收。
