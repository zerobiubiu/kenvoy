# 0008：受控 JavaScript 执行与模块隔离

- 状态：参考；阶段 00 设计记录，rquickjs 依赖组合、构建条件与限制探针待验证。
- 影响范围：服务端函数、TypeScript 转换、Host API、安全边界。

## 背景

开发者需要以 TypeScript 编写纯计算和简单异步业务函数。服务端实现不能泄漏到浏览器，也不能直接获得宿主的全部能力。

## 问题

JavaScript 引擎不是 TypeScript 编译器，也不是完整操作系统沙箱；Promise、模块加载和资源限制都需要宿主协作。

## 可选方案

1. 只支持 Rust 扩展：无法满足用户 TypeScript 函数目标。
2. 依赖完整 Node/Bun 后端运行时：扩大兼容范围并改变默认宿主路线。
3. Kenvoy 内部使用 rquickjs 适配 QuickJS 系列引擎，提供受控的函数执行流水线。

## 最终选择

采用方案 3 作为待验证路线，第一版只承诺以探针验收同步纯计算、简单 Promise、ES Module 和 JSON 兼容参数/返回值。先由 Kenvoy 完成 TS 到 JS 转换、服务端模块图验证和函数注册，再调用引擎。KEngine 只提供通用引用信息。

具体 rquickjs、底层引擎版本和 feature 组合须通过锁文件、源码与平台编译探针核对；Bellard QuickJS 文档是概念依据，不代表 rquickjs 任意版本必然使用该文档所述同一源码版本。[rquickjs 文档](https://docs.rs/rquickjs/latest/rquickjs/)、[QuickJS 文档](https://bellard.org/quickjs/quickjs.html)

模块 resolver/loader 使用明确允许的构建模块清单，拒绝任意文件路径、网络 URL 和未批准包；默认不提供文件、网络、进程、数据库连接或内部指针。未来 Host API 通过带身份与权限的类型化上下文调用 Resource Runtime。

## 原因

轻量嵌入满足 Rust 宿主目标，同时把语言转换、权限和执行生命周期保留在 Kenvoy 内。

## 已知缺点

Promise 需要驱动 job queue 或相应异步 Runtime；同步 Runtime 的互斥锁不适合随意跨异步等待。模块生态也不等同于 Node API 兼容。[rquickjs Runtime/Context 说明](https://docs.rs/rquickjs/latest/rquickjs/)

超时采用引擎 interrupt、宿主 deadline 和任务取消共同控制；仅用 Tokio timeout 无法证明已停止同步无限循环。设置内存、栈、队列和输出上限，且必须实测生效：当前 rquickjs 文档指出自定义 allocator，包括 `rust-alloc`/`allocator` 配置下，`set_memory_limit` 可能是空操作；此时需受限 allocator 或其他强制限制方案。[Runtime 限制与 loader API](https://docs.rs/rquickjs/latest/rquickjs/struct.Runtime.html)

## 影响

可信项目代码按受限 Runtime 执行；不可信第三方代码需要独立进程或更强隔离，不能把内存/时间限制称为完整安全沙箱。客户端可达图触达 Server Function 实现时构建失败，浏览器只调用授权 action ID。异常、未处理 rejection 和超限释放上下文并记录脱敏日志。

## 验证与状态

本轮未引入 rquickjs、未执行函数或资源限制探针。阶段 08 前须在目标平台验证 TS 转换、ESM 导入、异步返回、异常、无限循环中断、内存耗尽、allocator 限制有效性、模块越界拒绝及失败后资源释放；客户端构建同时检查服务端源码隔离。任何限制未验证前，不向不可信代码开放执行。
