# 示例：企业请假管理（完整功能模板）

`examples/leave-app` 是 Kenvoy 的完整可运行示例，展示：

- **多应用结构**：工作台 + 请假应用（申请 / 审批列表 / 详情）+ 人事中心（通讯录 / 人员台账）+ 系统管理（用户 / 角色 / 部门 / 通讯录）
- **多角色权限**：普通员工 / 部门主管 / 人事总监 / HR / 管理员，`DataFilter` 行级过滤（普通员工仅见本人）
- **审批流**：请假 ≤3 天 → 部门主管 → 人事备案；>3 天 → 人事总监 → 人事备案；驳回回到发起人
- **用户 / 部门 / 通讯录**：`kv user` / `kv role` / `kv dept` / `kv contact` CLI + 管理端点 + 管理页面
- **系统数据源**：`<Table source="users|roles|departments|contacts">`（管理页，需管理员）
- **详情页**：列表行点击跳转 `/leave/detail?id=N`，展示字段 + 流程状态
- **module 复用**：`module/进度卡.kenvoy.xml` 全局组件被工作台 / 人事中心引用
- **未登录引导**：打开工作台看到"登录 / 注册"条；navbar 有登录态徽章与退出按钮
- **表格操作列**：审批列表每行可撤回 / 通过 / 驳回；用户管理可审核 / 禁用（权限由后端校验）

## 快速开始

前置：PostgreSQL 18（示例默认 `127.0.0.1:5433`，容器名 `kenvoy-pg18`）、可选 MinIO。

```bash
# 1. 引导角色 / 部门 / 演示账户（幂等，可重复执行）
bash seed.sh          # Git Bash / WSL / Linux
# 或 Windows cmd：
seed.bat

# 2. 启动
cd examples/leave-app
cargo run -p kenvoy -- dev

# 3. 打开
# http://127.0.0.1:5173
```

## 演示账户

密码均为 `demo123`（`[auth.password].strength = "none"`）：

| 账户 | 等级 | 角色 | 你能演示什么 |
| :--- | :--- | :--- | :--- |
| `admin` | 100 | 管理员 | 全部管理端点（用户/角色/部门/通讯录）、任意数据 |
| `hr` | 80 | HR | 通讯录 / 部门管理、人事备案节点 |
| `ceo` | 70 | 人事总监 | >3 天请假审批 |
| `alice` | 30 | 部门主管 | ≤3 天请假审批 |
| `bob` | 10 | 普通员工 | 提交请假，列表仅见本人记录 |

通讯录另有 `Carol 王` / `Dan 赵`（无账号人员，用于演示"人事录入无账号人员"）。

## 端到端演示步骤

1. 用 `bob` 登录 → 提交请假（>3 天）→ 列表可见"审批状态=未提交/待审"
2. 用 `alice` 登录 → 审批列表看到该单 → 通过 →（>3 天）流转到人事总监
3. 用 `ceo` 登录 → 审批 → 通过 → 流转到 HR 备案
4. 用 `hr` 登录 → 完成备案 → 状态变"已通过"，申请人收到站内消息
5. 任一时刻用 `admin` 登录可管理用户/角色/部门/通讯录

## 目录说明

```
apps/
├── layout.kenvoy.xml          # 全局布局：navbar（工作台/请假/人事/系统管理）+ 页脚
├── dashboard/page.kenvoy.xml  # 工作台：登录引导 + 统计卡 + 图表 + 流程查看器
├── leave/                     # 请假应用
│   ├── layout.kenvoy.xml
│   ├── models.kenvoy.xml      # 请假单模型（含流程状态字段）
│   ├── apply/                 # 申请表单（Validation + 附件 + 提交按钮）
│   ├── list/                  # 审批列表（操作列 + 行点击详情）
│   └── detail/                # 请假详情（字段 + 流程状态）
├── hr/                        # 人事中心（通讯录 / 人员台账）
└── admin/                     # 系统管理（用户 / 角色 / 部门 / 通讯录）
module/
└── 进度卡.kenvoy.xml          # 全局复用组件（工作台 + 人事中心引用）
I8n/zh/  I8n/en/               # 中文 / English 翻译
seed.sh  seed.bat              # 引导脚本（幂等）
```

## 注册审核（review 模式）

默认 `[auth].mode = "open"`（公开注册）。切换 `review` 后：

```toml
[auth]
mode = "review"
```

新注册账号为 `pending`，无法登录；管理员在「系统管理 → 用户」页点「通过」启用（或 `POST /__kenvoy/auth/users/{id}/approve`）。
切换后 `seed.sh` 创建的演示账户仍可直接登录（`active`）。

## 重置

```bash
bash seed.sh --reset           # 删除 demo 用户 + 通讯录后重建
bash seed.sh --reset-users     # 仅删 demo 用户
```

> 注意：`--reset` 需要 `psql` 客户端；也可手动在管理端点删除。