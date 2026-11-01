# 多文件上传与图片展示

状态：参考；治理：现状文档。实现与实测边界见[附件与图片变更记录](../changes/0014-file-uploads-and-images.md)。返回[契约索引](README.md)。

## 声明与值

```tsx
export default <app name="资料系统">
  <resource name="documents">
    <text name="title" label="资料名称" required />
    <file name="attachments" label="附件与图片" required
      maxFiles={5} maxSize={10485760}
      accept={["image/*", "application/pdf"]} />
  </resource>
  <page path="/documents" title="资料登记">
    <form resource="documents" />
    <image src="https://example.com/photo.png" alt="资料图片"
      width={320} height={180} fit="contain" />
  </page>
</app>;
```

图片 URL 仅为用法示意，应用应替换为实际地址。`file` 是业务字段，`image` 是展示组件，不推导数据库列。

完整可运行项目见 [uploads-app](../../examples/uploads-app/README.md)：包括公开上传、管理员后台与 SDK 图库，以及可手动选择的 PNG/文本样例；默认没有预置业务记录或管理员密码。

| 文件属性 | 规则 |
|---|---|
| `maxFiles` | 默认 5，范围 1..20；同一字段始终使用附件数组，不在单项时变成对象 |
| `maxSize` | 单文件字节数，默认及硬上限 10 MiB（10485760），下限 1 |
| `accept` | 默认空数组；允许规范小写 MIME 和 `type/*`，不接受文件扩展名或 `*/*` |
| `required` | 拒绝 `null` 和空数组；可选字段可使用二者 |
| `default` | 仅可为 `null` 或 `[]`；不能声明预绑定文件，必填字段不能使用空默认值 |
| 其他约束 | 不支持 `index`、`unique`、数字/文本/选项约束，不可作为 select 的联动依赖或来源值/标签/筛选字段 |

字段值为 `UploadedFile[]`，元素严格只有 `id/name/content_type/size`：ID 为 64 位小写十六进制随机标识；名称为 1..255 个 Unicode 标量，不含控制字符、路径分隔符或单独的 `.`/`..`；大小必须为正整数。重复 ID、未知元数据属性和任意非附件对象拒绝。元数据不是访问凭据，服务器还会核对真实存储、会话、资源和字段归属。

除 `file` 外，既有字段仍使用 JSON 标量；PATCH 缺失表示保留，显式 `[]`/可选 `null` 表示清空。附件类型、数量及大小约束进入 Schema 快照；收紧约束须遵循现有预览、确认、备份及存量数据预检规则，不能静默删除不符合新规则的文件。

## 存储、绑定与生命周期

文件二进制保存在业务 SQLite 的 `_kenvoy_uploads` BLOB 列，结构与版本由 `_kenvoy_upload_schema` 独立核验，不创建公开磁盘目录，不以原始文件名构造宿主路径。业务列保存 JSON TEXT 元数据；dev/run 使用同一个配置数据库，已有 SQLite 一致性备份同时包含附件。

上传先成为当前有效 Session 的暂存文件，归属只存 Cookie 的摘要。有效期与上传会话一致：匿名会话通常 15 分钟，认证会话通常 8 小时；登录/退出旋转后，旧暂存不能转移到新会话，应重新选择文件。启动与每次上传清理已过期暂存，过期文件立即不可读取或绑定。

- 暂存限额：每会话最多 20 个、总计 40 MiB；全局最多 1024 个、总计 256 MiB。已绑定附件不占暂存额度；上传还受每 IP 每分钟 120 次、每会话每分钟 60 次的进程内限流。
- Create/PATCH 在同一写事务核对元数据、当前约束、未过期 owner 和资源/字段，随后绑定记录。已有附件只可由原记录保留，不能挪用到其他记录或字段。
- 替换、清空及删除记录在同一事务删除被移除的 BLOB。已确认的资源/字段删除或改型也在迁移事务内清理相关附件；变更前备份保留旧内容。
- 校验、唯一冲突或事务失败不改变旧记录/附件；新暂存仍可修改后提交或主动取消。
- 取消只停止后续文件并隔离迟到响应，不谎称已撤回已发送请求。已知暂存 ID 会请求清理；响应丢失时不自动重传，无法识别的暂存由到期机制回收。提交进行中或结果未知时，离开页面不能擅自删除可能已经绑定的文件。

## HTTP 与访问授权

| 接口 | 请求与结果 |
|---|---|
| `POST /_kenvoy/uploads/{resource}/{field}` | 每请求一个文件原始二进制；201 返回 `{data: UploadedFile, generation}` |
| `GET/HEAD /_kenvoy/files/{id}` | 读取内容或响应头；`?download=1` 强制下载，不支持其他查询参数 |
| `DELETE /_kenvoy/files/{id}` | 仅取消当前会话未绑定暂存；请求体为空，成功 204；已绑定文件只能随业务记录修改/删除 |

上传携带规范文件 `Content-Type`、`x-kenvoy-filename: encodeURIComponent(name)`、`x-kenvoy-upload-mode: create|update`（默认 create）。Create 继承 `x-kenvoy-form`，必须单独匹配包含目标字段的同一条表单策略；Update 使用资源 Update 字段策略。所有写操作沿用 Origin、Session、CSRF 和 generation 校验。读取大请求体前先授权，接收期限 30 秒；仅上传路由按字段大小放宽，普通 JSON 仍最多 1 MiB。

暂存只对当前有效上传会话可读；已保存文件必须满足该资源与字段的 Read 策略。公开表单新增不授予文件读取权，提交后原生表单只显示名称与大小，不继续请求无权读取的图片。没有附件列表接口；未知 ID、他人暂存与已移除附件不返回内容。文件和下载响应均 `no-store`、`nosniff`，下载名称通过安全的 `filename*` 编码发送。

仅 PNG/JPEG/GIF/WebP 允许图片展示，服务器核对文件签名与声明 MIME；伪造或不支持的 image MIME 拒绝。其他 MIME 作为附件下载，不内联 HTML 等主动内容。签名识别不等于完整图片解码、病毒扫描或恶意内容审计。

## 图片组件与 SDK

`<image>` 必须显式声明 `src`、`alt`（装饰图允许空 alt）；可选 `width/height` 为 1..4096 的整数，`fit` 为 `contain`（默认）或 `cover`。支持无凭据 HTTP(S) URL 或单斜杠开头的站内路径，拒绝协议相对、反斜杠、控制字符及 javascript/data/blob URL。提供加载、空来源及失败状态，图片不拉伸，使用延迟加载、异步解码和 `no-referrer`。

URL 图片由浏览器直接请求，不由 Rust 代理或下载到数据库；站外服务能收到图片请求。CSP 仅放宽图片来源，脚本、连接及其他来源策略保持原有边界。

```tsx
import { Image, fileUrl, useField, useResource } from '@kenvoy/client';
import type { UploadedFile } from '@kenvoy/client';

export function UploadedPicture({ file }: { file: UploadedFile }) {
  return <Image src={fileUrl(file)} alt={file.name}
    width={320} height={180} fit="contain" />;
}
```

`fileUrl(fileOrId, true)` 生成强制下载地址；`AttachmentList` 复用多文件名称、大小、下载和图片展示。`useResource(name)` 提供 `upload(field, file, formId?, mode?)` 与 `removeUpload(id)`，仍由服务端最终授权。字段重载通过 `useField()` 的 `upload(files)`、`removeUpload(fileOrId)`、`cancelUpload()` 和 `uploadState` 使用同一表单控制器；不另建文件模型或自行重试。`Image`、`ImageProps/ImageBinding`、`UploadedFile/FileConstraints/FieldValue` 均从 SDK 导出。

默认表单与后台详情/表格共享附件显示；`override name="file"` 替换字段外观，`override name="image"` 替换图片展示，不改变上传验证、权限或值类型。上传或删除未完成、写入结果未知时阻止提交。

## 明确限制

目前是有界小文件、整文件上传和 SQLite 本地存储；没有对象存储、分块续传、服务端转码/缩略图、完整图片解码、病毒扫描或任意图片格式支持。大文件与更多存储后端需明确需求后另行设计，不使用伪造百分比进度或自动重试掩盖这些边界。证据与平台限制以[变更记录](../changes/0014-file-uploads-and-images.md)为准。
