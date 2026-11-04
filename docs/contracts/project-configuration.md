# 项目配置与路径规则

状态：参考（现状文档）。本文维护配置、兼容门禁、路径和重载规则；实现见 `crates/kenvoy/src/infrastructure/project/`，操作见 [CLI](../operations/cli-design.md) 与 [迁移执行协议](../operations/migration-design.md)。

CLI 从 --project 指定位置或当前目录向上寻找 kenvoy.toml。路径以项目根解析；未知表/键、错误类型和不支持的技术选项立即失败，不退回默认值。兼容字段必须显式声明，其他未填写配置使用下列默认值。

| 配置 | 默认值 |
|---|---|
| project.name / version | kenvoy-app / 0.1.0 |
| project.min_cli_version / format_version | 必须显式填写；不自动接入旧配置 |
| application.entry | src/app.tsx |
| server.host / port | 127.0.0.1 / 3000 |
| server.public_origin | 未配置；回环监听按实际端口推导 |
| database.provider / path | sqlite / data/app.sqlite |
| frontend.framework / render | react / csr |
| runtime.javascript | quickjs |
| runtime.trust | project |
| build.out_dir | dist |
| languages（完全省略时） | 默认启用 zh-CN/cn/简体中文、en-US/en/English、ko-KR/ko/한국어 |
| languages.fallback | zh-CN；必须是已声明代码，不能使用别名 |

host 为 IP 地址，port 为 u16，0 允许系统分配空闲端口。runtime.trust 仅接受 project 或 untrusted；当前只执行可信项目代码，untrusted 在 JavaScript 准备/运行时明确拒绝，不会降级为同进程不可信执行。实际验收进度见 [路线](../development/development-roadmap.md)。

阶段 05 增加 public_origin。非回环监听必须配置固定 HTTPS Origin；HTTP 只允许回环监听和 localhost/回环 IP 目标。禁止凭据、非根路径、查询、片段和显式零端口。服务端不根据 Host 或任意 Forwarded 头猜测可信站点；该配置用于 Origin 校验和 Secure Cookie。TLS 可由外部可信入口终止，Axum 自身仍提供 HTTP。

路径必须在项目内，不含 ..、绝对路径、控制字符、Windows ADS、设备名或尾点/空格。已存在祖先经 canonicalize 检查，禁止链接越根。源码、数据、固定 `migrations/` 与输出目录彼此独立；数据库不能直接位于项目根，不得把迁移目录配置成数据、源码或输出目录，避免指纹/监听的数据排除规则吞掉迁移输入。输出不能是项目根、.git、.kenvoy 或 node_modules。Windows 上也检查尚未创建路径的大小写别名。

框架内建 npm 包及 package.json 明确声明的包可成为 KEngine 的精确 External 引用；这不证明依赖已安装，实际打包另验。相对 CSS 必须对每个 importer 重新定位，不能借另一目录同名文件放行。普通 UI 修改只重新解析变化源码；CSS 不触发全部 TSX 重解析。

dev 重新读取配置，但 database.path、build.out_dir、server.host、server.port、server.public_origin 的改动需要重新启动；服务继续使用原有配置，不在监听期间搬移数据库或更换公开端口。兼容字段每次重新检查，普通 UI 候选仍按完整流程验证；正式迁移链或物理 Schema 变化要求停服后重启，不在热更新时迁移持久库。

开发 Vite 地址、代理密钥和镜像目录由宿主管理，不是用户可配置的任意上游 URL。普通项目不需要自行编写 Vite 配置。用户 package.json 的直接依赖作为明确输入，缺少依赖时通过 dev/build/run --install-deps 显式安装；不会静默修改依赖声明。CLI 使用独立框架工具链缓存，生产加载已构建产物时不启动 Node。

构建输入指纹覆盖源码、配置、包清单/锁文件、`migrations/`、根 `translations.json` 及支持的静态资产；数据库、输出、.git、.kenvoy、node_modules、target 与 .env 不作为输入。根 `language.schema.json` 是 languages 配置的派生内容，连同原子更新临时文件排除，避免自动更新引发重复构建。算法与身份见 [Runtime](../architecture/runtime-design.md#build-与-run)；构建不会补齐正式迁移。

无源码生产部署保留匹配的 kenvoy.toml、根 translations.json、完整 dist/manifest.json 及版本目录、独立 data 目录。产物内含目标 Schema、完整迁移链和规范化私有 i18n.json，不要求源码或 migrations/ 存在；但不能用打包词典绕过根译文校验。run 更新 language.schema.json 并校验文件、配置/CLI、产物身份和数据库结构/历史；root 译文规范化后必须与产物一致。

敏感信息不能进入客户端投影或开发错误响应；没有默认管理员密码。初始化使用 admin init，部署 Origin 和 Cookie 规则见 [安全设计](../architecture/security-design.md)。

## 四类版本与兼容规则

```toml
[project]
name = "customer-app"
version = "1.2.0"
min_cli_version = "0.4.0"
format_version = "0.4.0"
```

- `project.version` 是业务应用发布版本；不代表数据库迁移位置，也不等于 Kenvoy CLI 的包版本。
- `min_cli_version` 是 CLI 下限；`format_version` 是配置、DSL 及行为契约。当前支持 `>=0.4.0, <0.5.0`，最低 CLI 同时须不低于 0.4.0；旧可选 i18n/分语言文件协议已移除，不能只改版本数字而不转换配置与译文。
- 上述 0.4.0 是合法的兼容下限示例，不是所有新项目的固定生成值。create 将 `min_cli_version` 写为创建时 CLI 的包版本（当前 0.4.2），`format_version` 仍为 0.4.0；这不改变旧项目已声明的下限，也不等于一次项目格式升级。
- 三个配置版本均为完整 SemVer，按 precedence 比较，不进行字符串比较；构建元数据不改变优先级。稳定 CLI 拒绝预发布的最低 CLI 和项目格式；应用版本可为预发布。预发布 CLI 按自然次序检查下限和格式区间，例如 `0.4.0-alpha < 0.4.0`；不把预发布 CLI 当作对应稳定版。
- 缺少新增字段的旧配置明确拒绝运行并给出接入建议，不默默取当前格式或自动提高数字。先核对实际格式、补充经确认的字段，再单独处理旧库基线；`migrate history` 保留无需源码、无需通过兼容门禁的只读诊断入口。
- 兼容检查在源码编译、用户代码执行、数据库创建和迁移前完成。产物格式与框架指纹是另一道独立门禁，源码格式兼容不表示旧产物可运行。
- 应用构建哈希标识交付内容；Schema 哈希只标识规范结构。数据库迁移使用追加式 ID/checksum 及实际提交记录，同结构的数据清洗也可以独立推进迁移位置。

## 原生国际化

国际化始终启用。create 生成 languages 默认三语言、根 language.schema.json 和 translations.json；显式语言表只启用实际声明项，单语言也不能跳过文件校验。每语言 display 必填，alias 可缺但不能空或与代码/其他别名冲突，fallback 只能指向已声明代码。

`kenvoy i18n` 根据配置更新 Schema，只在 translations.json 不存在时生成带 `$schema` 的空词条文件。dev/run 更新 Schema 但不创建译文，缺失文件、引用或不完整/不合法语言矩阵均在启动业务前失败。值根据代码或别名归入规范语言；设备默认语言不支持时使用 fallback。完整规则见[原生语言契约](i18n.md)。


## 一次性开发快照

`kenvoy dev --snapshot dev-snapshots/try.sqlite` 显式创建最新结构，不执行正式历史。目标必须是项目内尚不存在的文件，父目录与正式数据、源码、迁移和输出目录隔离；拒绝复用已有文件、WAL/SHM 或快照标记。

快照同时写入数据库内 `_kenvoy_dev_snapshot` 来源标记、初始化保护 sidecar 和开发产物身份；单独复制或改名 SQLite 文件也不能去掉快照身份。`run`、正式迁移及旧库接入拒绝使用它。dev/run 不会因模式切换重置数据库，也不会把快照动态历史伪装成正式迁移。需要正式运行时，选择独立正式数据库并执行完整链；需要新的快照 Schema 时停止开发服务并指定另一空路径。
