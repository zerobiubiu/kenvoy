# 0015：补齐文件上传与图片可运行示例

状态：已执行；治理：不可变变更记录；执行日期：2026-11-02。影响范围：`examples/uploads-app` 及使用入口；前置：[0014 文件上传与图片实现](0014-file-uploads-and-images.md)。本轮没有修改 Kenvoy/KEngine 核心、依赖、产品版本或设计系统，没有迁移用户数据库。

## 背景与方案

0014 交付包含契约和临时验收项目，但没有正式 `examples/` 项目。本轮按用户要求补齐可运行示例，不重写框架上传逻辑，不将临时数据或构建缓存纳入仓库。

- `/uploads`：原生多文件表单，必填图片最多 4 张/每张 5 MiB，补充 PDF/文本最多 3 个/每个 2 MiB。
- `/images`：DSL `image` 使用已核验的 Rust 官方 PNG URL，替换地址只需修改一个常量；外部图片需要联网。
- `/gallery`：管理员页面，用 `useResource/Image/fileUrl/AttachmentList` 读取真实资料、展示和下载文件；包含加载、错误、空状态及刷新，不提供预置业务记录。仅展示最新 12 条，完整管理进入原生后台。
- 附带两张自生成 PNG 与文本样例，不添加第三方 UI 库或用户 npm 依赖，不设置默认账号密码。运行目录有独立忽略规则，数据库路径仍为项目 `data/app.sqlite`。

使用步骤与外部图片许可见[示例 README](../../examples/uploads-app/README.md)，接口边界仍由[附件与图片契约](../contracts/file-uploads.md)定义。

## 实施文件

| 文件 | 内容 |
|---|---|
| `examples/uploads-app/kenvoy.toml`、`.gitignore` | 项目入口、回环 3000 端口、独立数据库及缓存/产物忽略 |
| `examples/uploads-app/src/app.tsx` | 一份资料模型、两个文件字段、公开表单与三个页面 |
| `examples/uploads-app/src/gallery.tsx`、`gallery.css` | 同一 SDK 数据模型、真实图片/附件、刷新/空错状态和窄屏换行；仅使用局部布局样式 |
| `examples/uploads-app/src/image-source.ts` | 外部示例 URL 单一来源 |
| `examples/uploads-app/samples/sample-blue.png`、`sample-green.png`、`sample-note.txt` | 可手动选择的真实样例文件，均为本项目自生成或自写内容 |
| `examples/uploads-app/README.md` | Windows 运行命令、权限、操作流程、源码入口、数据与来源说明 |
| 根 `README.md`、`examples/README.md`、`docs/README.md`、`docs/contracts/file-uploads.md` | 可发现的示例入口 |
| `docs/changes/README.md`、`docs/verification/README.md`、本文 | 实施与专项验证索引 |

## 后续验证建议

外部 URL 依赖 GitHub 网络与对方服务，失败会显示真实错误，不承诺离线可见；上传与本地图库在工具链准备后不依赖该外部图片。需要换图时修改 `image-source.ts` 并核对新来源许可。

本轮仅验证 Windows/Chromium 和本示例，不扩大为跨平台、辅助技术、海量图库或全框架回归通过。框架代码未变，因此没有机械重跑全部 Rust/前端测试；0014 的历史测试计数不写作本轮结果。

## 执行验证记录

### 环境与隔离

使用本机 Windows x64、现有 `target/debug/kenvoy.exe`、现有 Node/npm 与 omp 默认浏览器配置。示例源目录只执行无数据库写入的 `check`；实际账号、上传、删除及生产重启均在独占副本 `artifacts/uploads-example-4MOKFs`，仅将该副本端口改为 0。

副本由源码原样复制；随机临时管理员密码通过 stdin 初始化，未写进示例源码或命令参数。自定义客户端以 `--install-deps` 显式准备锁定工具链；没有新增 npm 用户依赖或修改锁文件。

### 实际命令与结果

| 验证 | 实际命令/结果 | 结论 |
|---|---|---|
| 声明检查 | `target/debug/kenvoy.exe --project examples/uploads-app check`；退出 0；“1 个资源，3 个页面，0 个动作” | 声明及客户端引用边界通过 |
| 忽略保护 | `git check-ignore -v --no-index` 核对 `data/app.sqlite`、`.kenvoy/client-toolchain`、`dist/manifest.json`；均命中示例自身规则，退出 0 | 数据/缓存/产物不入库 |
| 开发运行 | 副本执行 `dev --install-deps`；真实客户端构建成功，`files=6`，监听 `http://127.0.0.1:57424` | 自定义 SDK/CSS 可构建运行 |
| 生产模式 | 停止 dev 后，不预先 build，副本直接 `run`；监听 `http://127.0.0.1:59641`，复用已有产物 | 生产真实运行，不依赖开发代理 |
| UI 静态扫描 | `impeccable detect --json` 检查 app/gallery/CSS，退出 0，输出 `[]` | 没有新增忽略项或修改主题 |
| 文档门禁 | `cargo run --package xtask --offline --locked -- docs`；退出 0，“79 篇 Markdown、790 个本地链接，通过” | 示例、契约与变更记录可由入口发现 |

运行中的服务由本轮拥有的进程句柄管理；PTY 外层退出 512 不作为原生 Ctrl+C 优雅退出证明。

验收结束后已关闭本轮自有浏览器标签与 dev/run 服务，并删除独占副本及其账号、数据库、构建缓存。正式 `examples/uploads-app` 保留源码与样例文件，没有写入业务数据库或预置密码。

### 浏览器、HTTP 与 SQLite

1. 公开 `/uploads` 通过原生输入选择两张样例 PNG 和文本，提交成功生成记录 1。上传图片实际解码尺寸为 640×360、320×240。
2. 匿名新增后，三个已保存文件及资料列表的读取均返回 401，表单不再显示无权限的下载入口；未登录直接打开 `/gallery` 也为 401，不误写成自动跳转登录。
3. `/images` 实际加载官方 URL，浏览器 naturalWidth/naturalHeight 为 512×512，不是占位图或拦截后的替代内容。
4. 管理员登录后，`/gallery` 通过真实 API 显示资料名称、说明、两张图片及文本。三个下载响应均 200/attachment，实际字节数为 4615、1767、193。
5. 图库管理链接实际进入 `/admin/materials?view=detail&id=1`，显示原生详情与全部文件；不是指向未实现的页面。
6. 直接读同一副本 SQLite：记录 1/`示例项目烟测`，`photos` 数组长度 2，`attachments` 长度 1。
7. 生产重启后，原记录、两图一附件和三个文件字节数保持一致，`/_kenvoy/development` 返回 404。
8. 仅删除该副本内本轮创建的记录 1（204），点击示例自身“刷新资料”后，出现真实空状态和返回上传入口；没有以 mock 清空界面。

表单和图库分别在 1440px、390px 截图并逐张查看，窄屏文档无横向溢出；名称、下载、刷新和管理入口可见。本机忽略目录保留 `uploads-example-form-desktop.png`、`uploads-example-form-mobile.png`、`uploads-example-gallery-desktop.png`、`uploads-example-gallery-mobile.png` 作为证据，不写入 Git。

窄屏导航默认收起，因此自动化等待“退出登录”文字曾超时；之后按实际 URL/可见页面确认已登录。另修正了泛化按钮选择器和假定详情标题的自动化条件，没有因此修改产品或绕过错误。现有设计系统和默认表单/后台保持不变；本轮只做示例局部布局与实际界面核对，不宣称独立全站设计审查。

### 已知限制

没有重新验证所有上传格式、错误注入、全部平台或既有七套浏览器流程。没有给 URL 失败设置假图兜底；没有新增完整 TypeScript 类型检查承诺。实际响应和运行证明只适用于上列场景。
