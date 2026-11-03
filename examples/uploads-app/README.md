# 多文件上传与图片展示示例

使用同一份资料模型演示公开上传、原生后台和管理员图片库。没有默认账号、预置业务记录或第三方组件库依赖；照片和附件实际保存在本示例独立的 `data/app.sqlite`。

## 运行

要求 Kenvoy CLI 0.4.0 或以上兼容版本，项目格式为 0.4.0。先按[项目说明](../../README.md#从源码构建与验证)构建 Kenvoy。从仓库根目录执行：

```powershell
.\target\debug\kenvoy.exe --project examples/uploads-app check
.\target\debug\kenvoy.exe --project examples/uploads-app admin init --username admin
.\target\debug\kenvoy.exe --project examples/uploads-app dev --install-deps
```

初始化管理员时输入你自己的密码，不把密码写入源码或命令参数。首次自定义客户端构建需要 Node/npm，`--install-deps` 显式准备框架锁定工具链；无需手工 `npm install`，示例没有额外用户包。之后复用缓存。

默认监听 `http://127.0.0.1:3000`，访问根地址 `/` 后自动进入上传页；和其他示例同时运行时，修改本示例 `kenvoy.toml` 的端口。

| 地址 | 用途与权限 |
|---|---|
| `/` | Runtime 根入口，按当前身份进入首个可访问页面；本示例为 `/uploads` |
| `/uploads` | 公开资料表单，多选上传图片及补充附件 |
| `/images` | 声明式 `<image>` 读取真实图片 URL；此页的外部图片需要联网 |
| `/gallery` | 管理员专用 SDK 图库，读取真实资料及图片，没有自动灌入的示例记录 |
| `/admin/materials` | 原生管理后台，查看、编辑、替换文件或删除资料 |

停止 dev 后，直接执行生产模式，无需预先 build：

```powershell
.\target\debug\kenvoy.exe --project examples/uploads-app run
```

两种模式使用同一数据库。`data/`、`.kenvoy/`、`dist/` 和 `node_modules/` 已被本示例的 `.gitignore` 排除；不要删除数据目录来重建前端。

本例通过 `[languages]` 显式启用 `zh-CN`，显示名称为“简体中文”。根目录 `translations.json` 仅含必需的 `$schema` 引用，界面沿用源码默认文案和框架内置中文，原生国际化仍然启用。在示例目录执行 `kenvoy i18n` 会按配置更新 `language.schema.json` 并校验译文，不覆盖已有词条或自动翻译。完整规则见[示例总览](../README.md)。

## 操作流程

1. 用未登录的浏览器访问 `/uploads`，填写资料名称，可选填写说明。
2. 在“图片”中同时选择 [sample-blue.png](samples/sample-blue.png) 和 [sample-green.png](samples/sample-green.png)，观察预览；也可选择自己的 PNG/JPEG/GIF/WebP。每个最多 5 MiB，最多 4 张，至少一张。
3. 在“补充附件”中选择 [sample-note.txt](samples/sample-note.txt) 或自己的 PDF。每个最多 2 MiB，最多 3 个，此字段可不填。
4. 上传完成前不能提交；可逐项移除，或取消本次填写清理已知暂存。提交后取得真实记录编号。匿名新增不授予已保存文件的读取权限，因此提交后只保留名称和大小，不再请求图片预览或提供下载入口。
5. 从导航进入管理员登录，再访问 `/gallery`，查看刚才的真实记录、图片及补充附件，点击图片或附件的下载链接。新上传或后台修改后使用“刷新资料”；图库仅展示最新 12 条，完整查询进入后台。
6. 在 `/admin/materials` 编辑资料，移除旧文件并选择新文件后保存；已保存附件只有随记录提交修改才删除。删除整条记录时，其附件一并清理。
7. 访问 `/images` 查看固定 URL 图片；修改 [image-source.ts](src/image-source.ts) 的常量可替换地址。断网、地址失效或权限不足时显示真实加载失败，不以本地假图片伪装成功。

## 源码入口

- [app.tsx](src/app.tsx)：`file`、`maxFiles/maxSize/accept`、公开表单、`image` 及管理员页面。
- [gallery.tsx](src/gallery.tsx)：`useResource('materials')` 从受控 API 读取真实列表，`Image/fileUrl` 展示及下载图片，`AttachmentList` 显示补充附件；包含加载、失败、刷新及空状态。
- [gallery.css](src/gallery.css)：仅限定图库的间距与换行，不替换框架主题或原生后台。
- [image-source.ts](src/image-source.ts)：外部图片 URL 的单一来源。

公开表单只授权新增，不开放列表、详情、修改或删除。图库页面声明 `permission="admin"`；服务端继续执行 Resource/文件 Read 权限，自定义组件不会绕过授权。示例未增加新的文件协议、上传 API、默认 UI 库或服务端函数。

## 样例文件与外部来源

`samples` 中两张 PNG 是本项目生成的合成色块图，文本附件为自写说明，仅作上传与尺寸验证，没有用户资料；沿用仓库许可证。它们不是网页占位图，也不会自动写入数据库。

URL 页面原样引用 [Rust 官方素材仓库的 rust-logo.png](https://github.com/rust-lang/rust-artwork/blob/main/logo/rust-logo.png)，不将第三方图片复制入库。作品依据 [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/) 提供，未修改；参见[来源许可与商标说明](https://github.com/rust-lang/rust-artwork/blob/main/logo/README.md)。此处仅演示图片 URL，不代表 Rust 项目为本示例背书；外部站点会收到浏览器图片请求。

完整字段与 SDK 契约见[多文件上传与图片展示](../../docs/contracts/file-uploads.md)，文档总入口见[文档中心](../../docs/README.md)。
