# 通用管理后台与真实浏览器闭环

状态：已执行；影响范围：Kenvoy React Runtime 与阶段 07 验收。

## 背景与方案

已有真实受保护 CRUD 与前台登记，管理员需要使用同一 Resource IR 管理记录。通过通用 Admin Runtime 增加资源导航、查询与 CRUD，不生成每个资源的独立页面源码。

## 实施

- 新增 `crates/kenvoy/frontend/src/Admin.tsx`：导航、详情、创建/编辑工作区与记录级删除确认。
- 修改 `Runtime.tsx`、`App.tsx`、`theme.ts`：复用表单和表格，编辑仅发送 dirty 字段，保持 null，分页归零、空状态、长标题与中文关闭标签。
- 新增 `frontend/scripts/admin-e2e.mjs`；调整 `forms-e2e.mjs` 使用字段可访问状态与实际登录跳转。
- 更新根 README、路线与文档索引，明确源码构建前需先构建真实前端资产。
- 无文件删除；没有改变 KEngine 公共契约，没有业务数据库迁移。

## 执行验证记录

环境、命令、退出码、首次失败原因及实际截图范围见[阶段 07 验证](../verification/0009-admin-browser.md)。最新 TypeScript/Vite/Rust 构建及前台、后台 Chromium 流程退出 0。

后续建议：完成自定义组件和服务端函数后，在完整生命周期中复验 Fast Refresh、错误恢复、Ctrl+C、自动生产构建与 artifact-only 部署；当前后台通过不替代这些尚未实现的能力。
