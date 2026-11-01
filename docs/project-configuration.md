# 项目配置与路径规则

状态：现行。本文维护配置默认值、路径和重载规则，类型见 `crates/kenvoy/src/project.rs`；基础示例见 [customer-app 配置](../examples/customer-app/kenvoy.toml)，配置错误恢复与生命周期结果见[最终验收](verification/0012-final-acceptance.md)。

CLI 从 --project 指定位置或当前目录向上寻找 kenvoy.toml。路径以项目根解析；未知表/键、错误类型和不支持的技术选项立即失败，不退回默认值。未填写的配置使用以下默认值。

| 配置 | 默认值 |
|---|---|
| project.name / version | kenvoy-app / 0.1.0 |
| application.entry | src/app.tsx |
| server.host / port | 127.0.0.1 / 3000 |
| server.public_origin | 未配置；回环监听按实际端口推导 |
| database.provider / path | sqlite / data/app.sqlite |
| frontend.framework / render | react / csr |
| runtime.javascript | quickjs |
| runtime.trust | project |
| build.out_dir | dist |

host 为 IP 地址，port 为 u16，0 允许系统分配空闲端口。runtime.trust 仅接受 project 或 untrusted；当前只执行可信项目代码，untrusted 在 JavaScript 准备/运行时明确拒绝，不会降级为同进程不可信执行。实际验收进度见 [路线](development-roadmap.md)。

阶段 05 增加 public_origin。非回环监听必须配置固定 HTTPS Origin；HTTP 只允许回环监听和 localhost/回环 IP 目标。禁止凭据、非根路径、查询、片段和显式零端口。服务端不根据 Host 或任意 Forwarded 头猜测可信站点；该配置用于 Origin 校验和 Secure Cookie。TLS 可由外部可信入口终止，Axum 自身仍提供 HTTP。

路径必须在项目内，不含 ..、绝对路径、控制字符、Windows ADS、设备名或尾点/空格。已存在祖先经 canonicalize 检查，禁止链接越根。源码、数据和输出目录彼此独立；数据库不能直接位于项目根，数据目录不能与源码目录重叠。输出不能是项目根、.git、.kenvoy 或 node_modules。Windows 上也检查尚未创建路径的大小写别名。

框架内建 npm 包及 package.json 明确声明的包可成为 KEngine 的精确 External 引用；这不证明依赖已安装，实际打包另验。相对 CSS 必须对每个 importer 重新定位，不能借另一目录同名文件放行。普通 UI 修改只重新解析变化源码；CSS 不触发全部 TSX 重解析。

dev 重新读取配置，但 database.path、build.out_dir、server.host、server.port、server.public_origin 的改动需要重新启动；服务继续使用原有配置，不在监听期间搬移数据库或更换公开端口。其他受支持配置随完整候选重新验证，失败保留旧状态。

开发 Vite 地址、代理密钥和镜像目录由宿主管理，不是用户可配置的任意上游 URL。普通项目不需要自行编写 Vite 配置。用户 package.json 的直接依赖作为明确输入，缺少依赖时通过 dev/build/run --install-deps 显式安装；不会静默修改依赖声明。CLI 使用独立框架工具链缓存，生产加载已构建产物时不启动 Node。

构建输入指纹覆盖源码、配置、包清单/锁文件及支持的静态资产，包括 TS/TSX/JS/JSX/MTS/CTS、CSS、常见图片和字体；数据库、输出目录、.git、.kenvoy、node_modules、target 与 .env 文件不作为客户端输入。修改受支持输入会使 run 重新构建，并触发 dev 候选检查。具体白名单由 artifact.rs 维护，未知文件类型不自动成为公开资产。

没有源码的生产部署保留 kenvoy.toml、完整 dist/manifest.json 及版本目录、独立 data 目录；run 验证格式、框架指纹、文件路径/大小/哈希和数据库结构。dev/run 使用相同数据库路径规则，构建输出不包含业务数据库。

敏感信息不能进入客户端投影或开发错误响应；没有默认管理员密码。初始化使用 admin init，部署 Origin 和 Cookie 规则见 [安全设计](security-design.md)。
