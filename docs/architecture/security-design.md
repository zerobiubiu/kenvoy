# 认证、授权与 HTTP 安全

状态：参考（现状文档）；核对日期：2026-11-01。本文维护认证、授权、请求、产物及开发代理的安全契约。正式生命周期与浏览器回归见[最终验收](../verification/0012-final-acceptance.md)；功能验证不表示完成独立安全审计或部署认证。

## 边界与威胁模型

Kenvoy 是可信项目代码的本机宿主。KEngine 不处理账户、Session、HTTP 或权限。业务 IR 先经 DSL 和模块边界检查，HTTP 只消费有效模型；数据库只访问注册资源，内部账户表不能通过通用资源 API 访问。直接修改数据库、宿主代码或运行账户权限的操作者属于部署管理边界，不能靠应用 Cookie 阻止；仍会检查结构漂移、会话孤立外键和不合法密码成本，避免默默接受损坏状态。

当前授权粒度为资源操作、表单字段和 Action Execute，支持 anonymous/user/admin。尚无行级所有权、组织隔离、MFA、找回密码、自助注册或不可信插件沙箱；不能将 user 角色当成已实现多租户隔离。QuickJS 仅允许可信项目纯计算；runtime.trust=untrusted 明确拒绝，执行限额不能代替操作系统隔离。

## 账户初始化与密码

初始管理员通过本机 CLI admin init 创建，不提供公开注册接口，也不硬编码默认密码。已有管理员时 init 不覆盖；追加账户使用显式 admin add-user。终端输入隐藏并二次确认；非交互输入须显式 --password-stdin，不支持命令行密码参数。CLI 的本机操作者具有数据库访问能力，账户命令不借用浏览器管理员 Session。

用户名为 3—64 个 ASCII 字符，以字母/数字开头，允许字母、数字、下划线、点、@、连字符，统一小写。密码为 12—128 个 UTF-8 字节，禁止控制字符及全空白。Argon2id 参数为 version 19、m=19456 KiB、t=2、p=1、输出 32 字节，随机 salt 16 字节；数据库保存 PHC 哈希，不保存明文。固定格式和成本校验避免被篡改 PHC 任意提高运行成本。

密码工作在 spawn_blocking 执行，信号量最多同时 2 个任务，超载返回受控错误。不存在或非法用户名也执行同参数 dummy hash 验证，不直接返回账号存在性；过长输入在进入密码运算前拒绝。日志和普通 JSON 不输出密码、PHC 或原始 Session token。

## Session 与 Cookie

Session token 由系统随机源生成 32 字节，以 64 个小写十六进制字符传输；数据库只存 SHA-256 摘要。匿名上下文有效期 900 秒，认证上下文 28800 秒，不按每次读取自动延长。表内全局最多 4096 条会话，每用户最多 32 个活跃会话；新建时有界清理过期项，过期用户会话不占活跃上限。

登录和退出在同一事务中消费旧 token 并生成新 token；同 token 并发旋转只有一个能成功，容量或 SQL 失败回滚，旧 token 不因失败丢失。有效期和账号状态以服务端为准。孤立的非空 user_id 不会被当成匿名会话，认证初始化和查询均拒绝损坏关系。

Cookie 名称 kenvoy_session，Path=/、HttpOnly、SameSite=Lax，Max-Age 不超过服务端到期时间；HTTPS public_origin 添加 Secure，不使用 LocalStorage 作为认证依据。会话响应返回用户视图、CSRF 值、到期时间和 generation；原始 token 只通过 Set-Cookie 下发，不放入响应 JSON。

CSRF 值为以 Session token 作 key、固定域标识 kenvoy.csrf.v1 为消息的 HMAC-SHA256，比较使用 MAC 验证接口。旧会话 CSRF 不能用于新会话。Cookie 为 HttpOnly 不意味着页面无 XSS 风险，前端仍须安全渲染文本和构建边界。

## 请求与授权

| 接口 | 当前规则 |
|---|---|
| GET /_kenvoy/session | 获取/复用上下文；创建匿名 Session 受限流 |
| POST /_kenvoy/login | 已有上下文、同源、CSRF、JSON 用户名/密码；登录后旋转 |
| POST /_kenvoy/logout | 同源、CSRF、空 JSON 对象；旋转为匿名上下文 |
| GET /_kenvoy/application | 按当前身份投影页面、字段及模块注册 ID |
| GET /api/:resource、GET /api/:resource/:id | 服务端匹配 List/Read 策略，默认仅管理员 |
| POST /api/:resource | 匹配单一 Create 策略和表单 ID、字段子集，再二次验证 |
| PATCH/DELETE /api/:resource/:id | 服务端匹配修改/删除策略，默认仅管理员 |
| POST /api/actions/:name | 只接受 args 数组，验证 Server/Shared 登记及单条 Execute 策略；同源、Session、CSRF 与代际检查 |
| /admin/* | 登录入口可访问；有前端产物时匿名已注册管理导航 303 跳转登录，普通用户 403；管理数据仍需 admin |

匿名受控提交不能获得列表、详情、更新或删除权限。多个表单策略不合并字段授权：每个请求须完整匹配其中一项策略。非管理员 Create 响应只包含该策略允许字段及系统 ID/时间，隐藏字段默认值不会随完整数据库记录泄漏。API 权限在服务端执行，前端隐藏按钮不能代替授权。

浏览器写请求必须携带与配置完全一致的 Origin、有效 Session Cookie 和 x-kenvoy-csrf。公开表单另外携带 x-kenvoy-form，使用 encodeURIComponent 编码表单 ID，宿主严格解码一次；ASCII 未编码值保持兼容，非法 UTF-8 和重复编码不能绕过策略。可附带 x-kenvoy-generation，过期代际拒绝并要求重新获取页面描述。未知资源、内部表、非法字段/ID/查询和未支持方法返回结构化错误；未知 API 不落入 SPA 页面。

请求 Host 必须匹配 public_origin，绝不根据任意 Host 或 X-Forwarded-For 推导信任边界；Sec-Fetch-Site: cross-site 的会话获取和写操作拒绝，公开页面的正常跨站 GET 导航允许。重复 Host/Origin/CSRF 等安全头、同名会话 Cookie、JSON 任意层重复键、重复解码后查询参数均拒绝。请求体最多 1 MiB，接收期限 30 秒；写入 JSON 要求正确 Content-Type。查询长度、分页、搜索和筛选另受数据库契约限制；搜索和 Contains 也拒绝 NUL，避免 SQLite LIKE 截断模式。

当前限流为进程内固定 60 秒窗口、最多 4096 个计数键：登录每来源 IP 及用户名各 5 次；匿名会话创建每 IP 20 次、全局 120 次；非管理员 Create 每 IP 60 次、每 Session 20 次；Action 每 IP 60 次、每 Session 20 次。已有会话读取不消耗新建额度。实际连接地址来自 ConnectInfo，不信任代理转发头；多实例、代理后的细粒度来源识别和配置化额度尚未实现。

## 元数据、静态资源与错误

浏览器 DTO 显式构造，不序列化内部 Application IR 整体。只提供可见页面和这些页面需要的字段；管理员可见全部资源。普通组件、客户端函数和表达式只发送注册 ID，不发送宿主路径、SourceSpan、原始函数或 server handler 实现。页面文本与静态 Props 由 React 文本/属性渲染，不使用未经验证的 HTML；用户自定义 UI 代码仍属于可信项目边界。

静态资源仅从宿主指定且规范化的 client 目录读取；仅注册页面使用 index.html，资源路径拒绝转义/父路径，并 canonicalize 后确认仍在根目录内。私有管理页先授权再读入口文件。已封存的 client 目录与 IR、generation、server bundle 同代发布。此边界假定构建目录由可信宿主管理，不抵抗具有相同文件权限的本机恶意操作者并发篡改文件。

响应统一 no-store、nosniff、same-origin Referrer-Policy、DENY Frame。CSP 默认 self，脚本 self，样式保留 unsafe-inline 以兼容用户组件库的运行时样式（内置控件使用外部 CSS），图片允许 self/data；frame-ancestors none、base-uri self。仅 dev 的 connect-src 追加与 public_origin 对应的明确 ws/wss Origin；React Refresh preamble 外置为同源模块，script-src 不允许 inline script。

错误响应有错误码和 request_id，字段校验仅包含字段/原因，不含数据库路径、SQL 或认证秘密。数据库队列拒绝与已接受写入结果不确定分别处理；结果不确定时应联系有权限的管理员核对，不能让匿名提交者查询全部数据，也不能自动重试 Create。

## 存储、发布和部署

认证使用固定 _kenvoy_users/_kenvoy_sessions/_kenvoy_auth_schema 表，对三者 owner 的完整 sqlite_master 对象（含自动/显式索引和触发器）与受控 DDL 比较，并要求版本记录严格为 [(1,1)]。部分结构、额外触发器、索引变更、损坏版本或孤立会话外键拒绝。业务迁移仅精确预留这些名字，不泛化忽略 _kenvoy_ 前缀。

HTTP 请求使用同代模型读锁。发布任务被宿主跟踪，在模型写锁内先验证代际、执行迁移，提交后不再发新的可失败数据库命令，直接替换模型及资产指针；调用者取消不会取消已接受的发布任务。服务退出等待已接受发布任务后，宿主再关闭数据库。

构建产物有格式、框架兼容性、输入及逐文件 SHA-256/大小校验；先封存独立版本，再激活 manifest。文件锁避免同项目构建并发写入，路径拒绝越根/链接/大小写冲突；外来输出目录拒绝接管，旧版本和数据库不自动清理。哈希用于完整性检查，不是对不可信产物的签名认证；能够修改整个部署目录的人仍在可信管理边界。SQLite 提交与磁盘清单激活不是跨介质事务，提交后的收尾失败必须明确报告，不能声称数据库已回滚；断电/崩溃故障注入尚未全面验收。

服务端函数包也属于同一代际。QuickJS 每次调用使用全新上下文、1 秒期限、32 MiB 内存与有界队列；没有文件/网络/环境变量/数据库宿主 API。参数和返回值检查大小、深度、节点数及安全整数；输出拒绝隐式损失类型。异常响应不含私有消息或堆栈。取消和关闭能终止 Promise 等待，CPU 和微任务循环也受 interrupt 限制。

客户端白名单不加载整个 app/diu 声明入口；表达式只追踪实际词法捕获，普通组件保留原始 import 门面及依赖。ServerFunction 根不能借 Shared 标记进入浏览器；私有依赖闭包在 DSL 与客户端构建两次检查。构建产物中的服务端代码只由宿主加载，HTTP 不公开 modules/server.json；真实浏览器验收还扫描了客户端全部登记文件，确认私有标记没有被打包。该检查不意味着可发现项目作者主动复制的所有业务秘密。

默认仅监听回环 HTTP；非回环监听必须配置 HTTPS public_origin。当前 Axum 自身提供 HTTP，外部 HTTPS 由部署层终止 TLS，并保留原始 Host；不能把配置为 https 等同于 Rust listener 已经启用 TLS。反向代理 IP 信任、多实例限流、证书自动化不在当前实现范围。

## 开发代理与候选隔离

dev 监听只唤醒有界构建协调队列，原始用户源码不直接交给 Vite watch。Rust 先完成分析、边界检查、构建和输入指纹复核，再准备受控镜像。候选失败不更新有效模型或 Schema；已发布运行态后的 Vite 更新失败会停用代理并回到已验证静态产物，不能把该失败描述为数据库回滚。

Vite 监听随机回环端口，每个 HTTP 请求和 WebSocket upgrade 均验证只在宿主内部传递的随机密钥。浏览器只访问 Kenvoy 的 `/_kenvoy/vite/*`；代理目标固定，不接受用户指定 URL，只接受 GET/HEAD 或专用 HMR upgrade。入口 HTML 仍在注册页面及身份检查后读取；未知 API、资源路径和开发命名空间中的任意 HTML 不回退应用入口。生产不存在开发状态和代理路由。

HTTP 代理拒绝非法百分号、重复编码、路径穿越及编码分隔符；不转发 Cookie、Authorization、Origin、CSRF 或浏览器伪造的内部密钥。上游只收到有限缓存/内容头和宿主密钥，不将 Set-Cookie、Location 转回浏览器。reqwest 禁用环境代理与重定向，固定 2 秒连接、15 秒请求期限，最多 32 个并发请求和单响应 16 MiB；上游错误转换为脱敏 404/503。

HMR WebSocket 要求完全匹配的 Origin 和 vite-hmr 子协议，采用实际 socket IP，每 IP 每分钟最多 60 次升级，最多 64 条连接。单帧/消息最多 1 MiB、写缓冲 2 MiB，发送和关闭均有期限；优雅退出先取消升级连接，再等待 HTTP 停止。密钥不发送到浏览器或日志。

`/_kenvoy/development` 只在 dev 返回 revision/generation、ready/building/error、reload 与可选脱敏错误。首次页面导航遇到 building 时最多等待 10 秒，等待期间不持有模型读锁；超时为 503 和 Retry-After:1，完成后使用当前代际重新鉴权。已有界面可继续显示最后有效数据和错误，不能借错误响应泄漏私有源码、SQL、凭据或宿主绝对路径。

## 验证状态

认证和 HTTP 基线证据保留于 [阶段 05 验证](../verification/0007-http-auth.md)，后台登录/会话/真实 SQLite 管理见 [阶段 07](../verification/0009-admin-browser.md)。阶段 08 增加纯计算、限额、Promise 取消、模块边界、JSON 精度、Action 授权与脱敏错误测试，以及真实浏览器组件/Server Function 和客户端私有代码扫描，见 [阶段 08](../verification/0010-typescript-extensions.md)。未以这些功能回归替代独立安全审计或不可信代码隔离验证。

开发代理、候选隔离与资源关闭见[阶段 09](../verification/0011-cli-lifecycle.md)，统一二进制的最终回归见[阶段 10](../verification/0012-final-acceptance.md)。各记录的覆盖范围和历史失败原样保留。

相关文档：[数据库](database-design.md)、[Application IR](../contracts/application-ir.md)、[DSL](../contracts/dsl-specification.md)、[CLI](../operations/cli-design.md)、[配置](../contracts/project-configuration.md)、[运行时](runtime-design.md)。
