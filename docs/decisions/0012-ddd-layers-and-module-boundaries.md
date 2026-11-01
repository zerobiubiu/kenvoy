# ADR 0012：DDD 分层与可执行模块边界

状态：已执行；治理：架构决策定稿；日期：2026-11-02。影响范围：四成员 Workspace 内部代码组织；前置：[ADR 0011](0011-rust-test-workspace-and-xtask.md)；实施与验证：[0016](../changes/0016-ddd-structure-and-module-split.md)。本决策不改变两个产品层、默认原生 UI、数据库/HTTP/IR/CLI/SDK 契约，不新增产品 Crate。

## 背景

用户要求按 DDD 驱动划分目录、原子化与低耦合，及时拆分大文件。原结构主要按技术功能平铺，出现业务模型依赖 DSL 位置类型、认证服务直接绑定数据库、HTTP 同时承载业务授权与发布协调，以及 DSL/迁移/测试的千行文件。

这里的领域不是客户、订单等某个示例的业务，而是通用语言分析和声明式应用宿主。不能为目录看起来完整而增加 Customer 实体、空 Repository、事件总线或通用 Aggregate 基类。

## 选择

1. 保留四成员和 `kenvoy → kengine` 产品方向。KEngine 的语言模型与规则为语言域；Kenvoy 的资源定义、字段不变量、授权与声明快照为宿主域。
2. Kenvoy 使用 `domain/application/infrastructure/interfaces` 四层，外加 `bootstrap` 装配根。KEngine 使用对应语言域层次，公开根门面保留；前端使用实际 `app/domain/application/infrastructure/presentation` 分层，`main.tsx/sdk.ts` 是真实固定入口。
3. 领域层只含模型、值对象与纯规则；应用层通过当前真实需要的静态端口调用适配器，不引入具体基础设施或传输类型。SQLite/SWC/QuickJS/文件系统在基础设施，CLI/HTTP/React 展示在接口边缘。
4. 命令运行、原生监听、Vite 与进程关闭等系统装配位于 bootstrap，不伪装成业务实体。具体 HTTP 服务绑定集中在外层 wiring；控制器不直调数据库 actor。
5. 业务校验、选项验证和附件绑定保留在数据库原子事务内；抽取依赖不能改变错误时机、语义顺序、取消/提交边界或序列化字段顺序。DSL 的真实 SWC 转换通过静态端口注入，不能为了“纯净”将 check 校验推迟到 build。
6. 自有代码硬上限为每文件 500 物理行，目标 400 行内；测试同样拆分，业务数据/依赖/生成产物不在此上限内。按自然职责拆，不以一函数一文件、空层、宽可见性或旧路径 shim 应付门禁。
7. 在 xtask architecture/check 中加入目录、规模与层依赖门禁。Rust 使用工程依赖 syn 解析结构，前端复用 KEngine 的公开语法 API；不把关键词/字符串命中当成架构违规，也不把驱动依赖加进领域或产品依赖闭包。

## 备选与取舍

- 只重命名目录：改动小，但不能解除真正反向依赖，未采用。
- 直接拆成多个业务 Crate 或微服务：扩大部署/公共 API/依赖面，且违反已确认四成员设计，未采用。
- 每个文件一个接口、动态仓储或统一大 shared 模块：引入间接层、分配及耦合汇集点，没有当前需求，未采用。
- 当前方案会增加少量真实端口、局部模块和 import，但用静态泛型/impl Future 保持运行路径，不为单一实现引入 BoxFuture/虚拟调度。

## 兼容与落地要求

公开 KEngine 根导出、`@kenvoy/client`、CLI 参数/输出、Application/Generic IR、SQLite Schema/hash 与 HTTP 行为保持。内部根 namespace clean cutover，全部调用方、测试、嵌入源清单、开发镜像和文档链接同步迁移。

KEngine 的精确模块定位仍接受既有 CSS 等文件；语言域不能因分层收紧宿主已有输入。测试 helper 的 `--exact` 过滤名与 RPC 协议必须对应真实执行，不接受 0 tests 假成功。

源码布局和维护规则见[DDD 目录规范](../architecture/ddd-structure.md)。本次统一执行范围与限制见 [0016 变更记录](../changes/0016-ddd-structure-and-module-split.md)；后续路线变化另建决策，不重写本篇定稿。
