# 默认 React Runtime

状态：参考（现状文档）；阶段 06/07 登记与后台、阶段 08 自定义组件和 Server Function 已通过各自的真实浏览器验收；阶段 09 的组件状态保留、CSS 更新、语法错误恢复及带开发 WebSocket 的优雅退出已通过专项真实验证。最终统一验证范围以阶段记录为准。交互模式为 Operate，采用代码实现后真实浏览器验证。

用户已明确授权自主实施，产品事实取自 [PRODUCT.md](../../PRODUCT.md) 和原始工程规范，不重复请求用户要求的 React/原生 CSS、布局和业务流程批准。本轮设计决定只服务本项目，不写入跨项目偏好。

## 结构与构建

当前默认组件已改用原生 React 控件与 CSS；系统组件重载和自定义数据组件使用统一字段绑定。新增 select 联动、跨资源选项与 SDK 详见[组件与选项契约](../contracts/component-bindings.md)。下文阶段 06—10 的 MUI 测试结论仅为迁移前历史证据；新实现的验证以[原生组件与联动选项变更记录](../changes/0012-native-components-and-options.md#执行验证记录)为准。

多文件字段、表格/详情附件展示、Image 组件及 SDK 图片接口已经接入同一 Runtime。`FieldValue` 仅为原标量或严格附件数组；上传、逐项移除和取消由表单控制器维护，不把文件对象直接交给 JSON CRUD。用法与安全边界见[文件与图片契约](../contracts/file-uploads.md)，本轮证据见[0014 变更记录](../changes/0014-file-uploads-and-images.md)。

当前浏览器验收由 `kenvoy-tests` 的 Rust 测试入口运行，Rust 拥有项目夹具和 CLI 子进程；Playwright 脚本保留页面行为断言。原 tools 进程夹具已退役，执行方式见[工程与测试流程](../development/engineering-workflow.md)。

React + TypeScript + 原生 CSS + React Router 根据 `/_kenvoy/application` 的受控投影渲染通用组件；普通表单和后台使用同一资源字段、验证和 API。函数/自定义组件只通过注册 ID 连接，不能 JSON 序列化函数或把完整应用入口打包给浏览器。

框架源码位于 crates/kenvoy/frontend，Vite/Rolldown 先生成 dist，再由 crates/kenvoy/build.rs 将公开 index/assets 嵌入 Rust Binary。基础 DSL 使用内置资产，应用启动不要求 Node、不安装 npm 依赖；开发框架源码时先在 frontend 执行 npm ci --ignore-scripts、npm run check、npm run build，再编译 Rust。当前依赖锁定于该目录 package-lock.json，缺少 dist 会明确阻止发行构建。

`src/frontend.rs::export` 将内置资产写入调用方独占创建的空 staging，检查文件内容、链接和目录边界，不覆盖已有文件。统一构建层将已验证产物保存到默认 `dist/versions/<build_id>/client`；`dist/manifest.json` 标识当前激活版本。加载时重新检查清单、文件大小和 SHA-256，篡改时保留现场并报错。当前流程不再使用 `.kenvoy/runtime`。业务 SQLite 仍位于独立持久化目录，构建不会清理数据库或已有版本；不按页面生成 React 业务源码。

阶段 08 已实现自定义 UI 的独立打包；阶段 09 通过受控 Vite 适配实现开发资源与 Fast Refresh，并已验证普通组件和 CSS 修改后 Hooks 状态保留。用户不需要编写 Vite 配置。构建只负责模块注册和资产，不按页面生成 React 业务源码。HTTP 静态服务只开放该版本的 `client/`，同级 `modules/server.json` 不属于公开目录。

### 自定义客户端构建与扩展

`client_build::build` 接收已经通过语义校验的 ProgramIR/ApplicationIR，只写调用方独占创建的空 staging。没有客户端模块引用时直接 `frontend::export` 内置资产，不检查或调用 Node/npm；清单仍记录内嵌框架依赖版本和 npm 锁文件摘要。存在自定义引用时，Rust 调用受控 `client-builder.mjs`，完成真实 Vite/Rolldown 打包；产物文件摘要和已解析的框架/用户 npm 版本交给统一清单。

框架的 package.json、package-lock.json、Runtime 源码和构建脚本嵌入 CLI。锁定工具链位于项目 `.kenvoy/client-toolchain`，按锁文件、操作系统、架构、Node 主版本分开；源码缓存另按内容指纹分开。首次准备工具链使用 `npm ci --ignore-scripts --include=dev`，此后验证标记、固定源码和工具可加载性后复用，不在每次启动重新安装。用户项目的 package.json 不被生成或修改；只有显式 `--install-deps` 才准备用户锁文件并安装其已声明依赖。第一版自动安装仅接受 npm registry 版本，不接受 Git、文件或 workspace 依赖说明符。

普通 React 组件沿原始 import/barrel 入口打包，保留 CSS 副作用、Hooks、Props 与用户选择的第三方组件的 ESM 行为；仅 React、React DOM 和 React Router 统一解析到工具链，其他 UI 库解析到用户项目依赖，避免双 React 实例。声明入口及 `.diu` 文件不直接打包；可证明的函数、const 和客户端表达式由 Kenvoy 的同一词法闭包切片器提取，复杂可变捕获明确诊断并建议拆成独立客户端模块。切片表达式以 factory 注册，函数与组件以稳定引用 ID 注册，运行时不使用 `eval`。

浏览器通过 `@kenvoy/client` 的 `useAction(name)` 或 `callAction(name,args)` 调用声明的服务端动作。SDK 复用页面的 ApiClient、同源 Cookie、CSRF 与应用代际；JSON 参数和返回值不包含函数对象。动作写请求同样不自动重试；网络中断或超时保留结果待核对状态。`useAction` 共享通用 Runtime 的 React Context，不创建另一套认证状态。

构建入口禁用用户 Vite/PostCSS 配置和 `.env` 加载，拒绝 Node 内置模块、服务端私有闭包、声明入口原文、未分析的新源码、内部配置及越界文件；CSS 资源 URL 也经过文件类型/路径检查。分析后先核对源码字节，Vite load 再验证 SHA-256 并直接使用相同字节，构建完成后再次核对。子进程不经 Shell，标准输出与错误各最多保留 256 KiB；构建 120 秒、依赖准备 180 秒超时后停止并回收直接子进程。该工具链用于可信项目，不能作为不可信 npm 依赖的操作系统沙箱。

实际宿主测试覆盖初次工具链安装、未授权用户依赖安装拒绝、显式真实 clsx 2.1.1 安装、postinstall 不执行、原 package.json 不变、包含安全内联表达式的构建、服务端标记不进入客户端，以及第二次构建指纹一致。独立 Node 测试覆盖 Hooks/MUI/SDK/barrel CSS/第三方包/JSX 切片的真实打包，并验证服务端模块、动态未分析模块、源码变化及 CSS 私有资源被拒绝。浏览器扩展验收由阶段 08 统一记录承载；这些构建测试本身不声称已验证 Fast Refresh。

### 开发状态与恢复

仅具有客户端模块引用的开发应用启动 Vite 子进程；基础 DSL 继续使用内嵌资产。`client_development.rs` 在语义分析和受控构建通过后创建客户端镜像，Vite 不直接监听项目源码。候选准备不修改活动镜像；宿主发布数据库与应用模型后才提交镜像和更新通知。普通组件修改交给 Fast Refresh，结构变化允许重新挂载；提交开发镜像失败时停用代理、回退已验证的静态产物并显示需重载状态，不声称回滚已经提交的数据库迁移。

开发镜像位于 `.kenvoy/client-dev`，只包含允许的客户端源码和资源，不复制完整声明入口、Schema Component 或服务端私有模块。源代码和资源在镜像收集前后校验输入指纹；镜像最多 8192 个文件，单文件最多 32 MiB，总量最多 256 MiB，配额在复制或读取前检查，读取同时限制字节数。开发模式的用户文件必须位于镜像中，`@id`、`@fs` 和 `?raw` 不能绕过边界读取原项目资产；框架源码、锁定依赖和优化缓存分别校验。

Vite 仅监听随机 loopback 端口，HTTP 与 WebSocket 均要求父进程的随机密钥；浏览器通过 Axum 同源代理 `/_kenvoy/vite/` 访问，密钥不发给浏览器。React 开发 preamble 由受控 HTML 处理器外置，同源脚本策略不需要 `unsafe-inline`。优化依赖首次生成时允许 Vite 等待其完成，避免将尚未落盘的合法依赖误判为 404。SDK 与 Runtime 共用独立的 `context.ts`，避免 SDK、组件注册表和 Runtime 的循环导入导致 Fast Refresh 丢失 Context。

仅开发宿主在应用描述中增加 `development: true`，浏览器才请求 `GET /_kenvoy/development`；缺省或 `false` 均不轮询。生产宿主的该端点返回 404。状态协议为 `{revision,generation,state,error?,reload}`：`state` 为 `ready`、`building` 或 `error`，`revision` 随状态变化递增，`generation` 仅随有效应用发布变化；`error` 是宿主提供的脱敏摘要。

前端在一次请求完成后等待 1 秒再读取，避免慢请求累积。编译失败时以原生语义提示显示“开发更新未通过”，继续保留有效页面与填写内容；`building` 不提前消除已有错误，`ready` 才清除。断线使用独立警告，恢复连接、观察到修订号回退或有效代际变化后重新获取 Session 与应用描述。通知本身不升级 API 的写入代际，也不重试任何提交、修改、删除或 Action。

首次读取状态只建立刷新基线；后续新修订明确携带 `reload: true` 时才整页重载，同一代际也可表达从开发代理退回静态资产的必要重载。普通组件更新交给 Vite Fast Refresh，前端状态轮询不会自行调用整页刷新；结构更新可按开发适配器策略重新挂载。浏览器组件错误边界同时覆盖自定义组件和表达式 factory，避免局部异常替代整个应用。

当前状态协议、无写入重试和刷新决策已通过单元测试。真实扩展浏览器脚本已验证：计数为 2 时修改组件后仍为 2，继续点击为 4；CSS 修改后仍为 4；语法错误期间旧页面与计数保留，修复后清除诊断且计数仍为 4。带活动开发 WebSocket 的原生终端 Ctrl+C 验证已观察到端口与连接关闭，重启同一项目后 SQLite 记录仍存在。镜像配额补丁随后通过定向测试，最终同一二进制的六套浏览器统一复验亦全部通过，详见最终验收记录。

## Direction contract

THESIS：登记人完成准确提交，管理员快速定位和修改记录；所有状态围绕真实操作反馈组织。

OWN-WORLD：采用公共信息目录的清楚标识与一致索引，蓝色主操作、深色文字、白色工作区和浅灰背景；原生语义控件、系统中文字体、均匀间距。界面不使用地图、纸张纹理或代码装饰。

STORY：访问者读标题、填写字段、提交后看到真实结果。管理员登录后从资源导航进入列表，搜索定位记录，在可返回的编辑表单完成修改。

FIRST VIEWPORT：桌面后台左侧资源导航，上方应用名和账号/退出，主区标题、搜索及新增按钮、排序表格和分页。登记页中央单列表单，错误位于字段下；移动端导航收起，操作不挤到屏幕外。

FORM：七个候选分别取自服务登记、分类索引、文档目录、表格排印、公共标识、资料档案、公共信息目录；采用第七项，保留用户固定的企业后台结构。参考 seed:35f73ecf88170e69。轨道图挑战方案只保留当前位置清晰的纪律；唱片编码方案不适合直接业务标签；登机牌方案只保留修改记录仍可定位的反馈。标志性交互是编辑成功后更新该行并短暂显示温和底色，同时提供文字确认。

FINISH：unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance

## 状态与验证

必需状态：加载、空数据、搜索无结果、字段错误、服务器错误、提交中、成功、网络中断后的结果待核对、登录失败、会话过期、普通用户无管理权限、删除确认。禁止将未知提交结果显示为失败并自动重试。

前端已在 Node 24 的真实 TypeScript 模块上通过 Rust 共用的 68 个标量向量，包括 Unicode 空白、字符数、NUL、安全整数、严格日期/时区与空值。阶段 08 记录包含 86 项客户端测试；增加开发状态协议、恢复决策与镜像边界后，本轮 `npm test` 实际通过 90 项，`npm run check` 与 `npm run build` 均退出 0。这是后续定向验证，不改写阶段 08 的统一计数。测试直接读取同一 JSON 文件，不另写一份规则向量。DOM 转换保留文本原文、避免整数舍入误判，不猜测 datetime 时区；Create 默认值与 Patch 缺失语义分别验证。

API 客户端只发同源 Fetch，浏览器管理 HttpOnly Cookie，内存保存 CSRF/应用代际。写入不自动重试；网络中断、超时、损坏的成功响应标记结果待核对。30 秒期限覆盖请求和响应体；字段错误和核对编号保留。Session GET 与登录/退出串行，旧请求不会覆盖新登录；刷新 Session 不自行升级旧页面的 Schema 代际。页面、字段与组件响应形状不合法时明确报错，不静默替换为空列表。

阶段 07 已复验客户登记必填/长度/整数/min、真实 SQLite 提交、UI 管理员登录，及后台详情、增改删、搜索、筛选、排序和分页。未触碰的 null 保持、重启后的记录和 Session 持久化通过；桌面与 390 像素移动视口截图已检查。窄屏表格具有横向滑动提示、可聚焦滚动区域和固定在右侧的“管理”菜单，桌面保留直接操作按钮。独立视觉复审结论为 ready，具体范围与未覆盖状态见 [DESIGN.md](../../DESIGN.md)。

阶段 08 浏览器已验证 Hooks、Props、入口捕获表达式、MUI/clsx/barrel CSS、Schema Component、服务端 TS/ESM/Promise 与真实提交，服务端私有标记未进入公开资产。历史边界见[阶段 06 验证](../verification/0008-react-forms.md)、[阶段 07 验证](../verification/0009-admin-browser.md)与[阶段 08 验证](../verification/0010-typescript-extensions.md)。完整 production run、HMR 与阶段 10 综合验收仍以各自正式记录为准。

本次收尾实际执行已有工具 `impeccable detect --json crates/kenvoy/frontend/src crates/kenvoy/frontend/index.html`，退出 0，原始结果为 `[]`；未安装工具，未新增忽略规则，也未因扫描结果修改 UI。静态检查器结果不替代浏览器渲染或交互验收。
