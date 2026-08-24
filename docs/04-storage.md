# 存储架构：PostgreSQL + S3

> 文档类型：📋 接口契约
> 状态：🔧 待执行
> 影响范围：数据持久化（模型数据）、文件上传、用户/权限数据
> 前置文档：[03-project-structure.md](./03-project-structure.md)
> 替代文档：无
> 生效版本：—

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
- PostgreSQL 持久化：待实现（模型数据 → PG 18 表；用户/权限系统表）
- S3 文件存储：待实现（`storage_object` 元数据 + S3 客户端抽象）
