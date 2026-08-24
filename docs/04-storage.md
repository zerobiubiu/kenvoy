# 存储架构：PostgreSQL + S3

> 文档类型：📋 接口契约
> 状态：✅ 已执行
> 影响范围：数据持久化（模型数据）、文件上传、用户/权限数据
> 前置文档：[03-project-structure.md](./03-project-structure.md)
> 替代文档：无
> 生效版本：0.1

## 1. 架构总览

项目采用 **PostgreSQL + S3 Object Storage** 的组合架构：

- **PostgreSQL**：结构化数据、业务关系、文件元数据、用户与权限
- **S3**：文件本体（图片、视频、文档、压缩包等大对象）
- **应用层**：维护 PostgreSQL 对象记录与 S3 实际对象之间的一致性

```text
业务数据
    ↓
storage_object（文件元数据：文件 ID / Bucket / Object Key / 文件名 / Content-Type / 大小 / ETag / 创建时间）
    ↓
Bucket + Object Key
    ↓
S3 Object Storage（实际文件内容）
```

## 2. PostgreSQL 职责

| 关注点 | 说明 |
| :--- | :--- |
| 业务数据 | 数据模型（`<Models>`）对应的业务表，事务性读写 |
| 文件元数据 | `storage_object` 表：文件 ID、Bucket、Object Key、原始文件名、Content-Type、文件大小、ETag、创建时间 |
| 关联关系 | 业务数据通过文件 ID / 对象记录与 S3 文件关联 |
| 用户与权限 | `users` / `roles` / `user_roles` / `departments` / `sessions` 等系统表 |

**基准版本**：PostgreSQL 18+（数据库结构与 SQL 以 PG 18 为准）。

## 3. S3 职责与抽象

S3 被视为**统一的对象存储协议与访问方式**，不与任何一家云厂商绑定：

- 项目只需配置符合 S3 API 的存储服务端点、访问凭证、默认 Bucket
- 任意兼容 S3 API 的服务均可作为存储后端（本地 MinIO、私有部署、云环境同接口）
- 更换存储服务只需改端点 / 凭证 / 区域 / Bucket 配置，不改业务代码
- 项目只定义「如何连接和使用 S3」，不关心「S3 由谁提供」

```text
PostgreSQL
    ├── 业务数据
    ├── 文件元数据
    └── 文件与业务对象的关联关系

S3 Object Storage
    ├── 文件
    ├── 图片
    ├── 视频
    └── 其他对象
```

## 4. 约束

- `public/` 静态资源目录**不可在运行时修改**；上传文件必须走 S3（或 SQL 存储），不走 `public/`
- 存储接口基于 S3 标准抽象，不依赖具体厂商 SDK 之外的专有能力
- 本地开发、私有部署、云环境使用相同的存储接口

## 5. 落地范围（当前状态）

- 表达式引擎、审批流状态机、表单校验：已实现（纯逻辑，见 `kengine` 的 `expr` / `workflow` / `validation` 模块）
- PostgreSQL 持久化：已实现（模型数据 → PG 18 表；用户/角色/通讯录/会话/存储元数据系统表，见 `kengine-db`）
- S3 文件存储：已实现（`kengine-s3` 对象存储抽象 + `storage_object` 元数据 + dev 上传/下载端点）

## 6. HTTP 端点（dev 服务器）

| 端点 | 方法 | 说明 |
| :--- | :--- | :--- |
| `/__kenvoy/file` | POST | multipart 上传（字段名 `file`），需登录；文件写入 S3，元数据写入 `storage_object`，返回 `{ id, objectKey, fileName }` |
| `/__kenvoy/file/{id}` | GET | 下载文件本体，需登录；按元数据 Content-Type 返回，未知 ID 返回 404 |

配置：`kenvoy.toml` 的 `[storage]`（endpoint / region / bucket / access_key / secret_key）；`[database]` 未配置或 `[storage]` 未配置时上传/下载返回 503。

## 执行验证记录（2026-08-25，S3 存储收尾）

- 环境：本地 Windows，Rust 1.98；Docker 容器 `kenvoy-minio`（MinIO，127.0.0.1:9000，minioadmin/minioadmin）
- 命令：`cargo fmt --all -- --check`、`cargo check --workspace`、`cargo test --workspace`（含 `KENVOY_TEST_DATABASE_URL`）、`cargo clippy --workspace --all-targets --all-features -- -D warnings`
- 结果：全部通过（kengine-s3 3 个 MinIO 集成测试；kenvoy 19 个测试含未配置存储 503 用例）
- 实现：`[storage]` 配置段；`POST /__kenvoy/file` / `GET /__kenvoy/file/{id}`（登录鉴权、元数据写入失败回滚删对象）；`runtime.js` 文件控件上传 → payload 携带文件 ID；ETag 取自上传响应头
- 冒烟：真实 MinIO + PG 容器上传/下载/重启持久化验证通过
- 验证结论：✅ 通过
- 已知限制：下载不设 Content-Disposition（前端按需处理文件名）；未做大小上限/类型白名单；`storage_object.bucket` 记录实际桶名，下载固定走默认桶
