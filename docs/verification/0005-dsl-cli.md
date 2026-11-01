# 阶段 03：DSL、字段契约与基础 CLI

状态：已执行；环境：Windows MSVC、Rust 1.99.1、Node 24.22.0；日期：2026-11-01。

## 已验收能力

Kenvoy 通过 KEngine 公共 IR 解释入口默认导出的 app/page；构建独立 Application IR v1。支持 resource、七类字段、option、form、table、action、navigation、item、group；检查属性、嵌套、引用、资源推导、重复、路由和权限。显式资源优先，隐式资源不静默合并；页面 label 覆盖保留在 Form.labels，不改变资源 Schema。

静态求值支持字面量、const、跨模块绑定、已实现运算、静态数组/对象/spread、JSX/Fragment 和 .diu.tsx 组合。普通 UI、事件和动态 Props 保存模块/表达式引用，不扫描组件函数体生成字段。未知动态 Schema、循环和超预算均报错。Schema 参数/children、解构求值和未导出的运行时函数尚不支持，明确诊断。

注册 server 入口及 .server 模块的运行依赖向下标记私有，明确 .shared 模块或 shared 注册形成共享边界；客户端可达私有模块、动态 import、import.meta 和未绑定 require 均拒绝。type-only 边不作为运行依赖。匿名只得到声明 form 所需的 create 策略，不获得列表/详情/更新/删除；多个表单策略不拼接字段权限。

严格 TOML 配置、项目发现、安全路径、create/check/inspect 已接入真实 Binary。check 可附带 --resource/--input/--patch 离线验证记录，始终不写数据库。inspect 展示注册计划和策略，不宣称 CLI 当前身份已获管理权限。

## 实际验证

| 命令 | 结果 |
|---|---|
| cargo fetch | 退出 0；新增锁定 22 个传递包 |
| cargo check --workspace --offline --locked | 退出 0 |
| cargo test --workspace --offline --locked | 退出 0；121 项常规测试通过，1 项原有浏览器探针忽略 |
| cargo clippy --workspace --all-targets --offline --locked -- -D warnings | 退出 0，无关闭 lint |
| cargo fmt --all -- --check | 退出 0 |
| cargo run -p kenvoy --offline --locked -- create examples/customer-app | 退出 0，真实创建示例 |
| cargo run -p kenvoy --offline --locked -- --project examples/customer-app check | 退出 0；客户管理系统，1 资源/1 页面/0 动作 |
| node tools/verify-architecture.mjs | 修复读取 metadata 的缓冲上限后退出 0，仍严格两个 Crate |
| node tools/verify-docs.mjs | 执行时 35 个 Markdown、121 条本地链接通过 |
| git diff --check | 退出 0，仅提示已有 LF/CRLF 转换策略 |

121 项构成：上一阶段 81 项，加 Kenvoy Binary 单测 30 项、真实 CLI 集成 4 项、独立 DSL 安全 6 项。30 项含 analyzer 10、静态求值 10、project 7、validation 3；validation 的一个测试实际执行 65 个共享 JSON 向量。

字段规则覆盖默认值、未知字段、Create/Patch、重复 JSON 键、Unicode 标量长度、White_Space、有限数字与安全整数、严格日期、显式时区与 UTC 毫秒规范化。datetime 超过 3 位小数、闰秒或超出 0001..9999 年范围拒绝；不静默截断。

## 审查与修复

独立测试发现并修复：不同目录同名 CSS 被全局 external 放行、Windows 大小写/设备名/ADS/保留路径别名、已有空目录创建覆盖风险、已导入的小写 UI 冒充内置字段、服务端依赖闭包泄漏和动态模块加载绕过。未知属性测试的错误码预期也按实际契约纠正，未放松错误检查。

首次集成发生 analyzer 文件尚未落盘导致编译失败；完成后验证通过。早期未消费函数警告已通过接通真实用途或移除无用 helper 解决；大诊断结构改用 Box 来源位置，保持 JSON 格式不变。架构脚本曾达到 Node 默认输出缓冲限制，改为显式 16 MiB 上限并保留具体错误，复验通过。

## 边界和下一步

本次没有启动正式 HTTP、创建 SQLite 数据库或渲染真实客户应用；前端投影、资源 CRUD、Session、迁移、构建和 dev/run 在后续阶段实施。示例源码可 check，不代表完整网站完成。下一步从同一 Resource IR 推导真实 SQLite Schema、Diff、事务迁移和持久化。
