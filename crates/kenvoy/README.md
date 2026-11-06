# Kenvoy

将 TypeScript/JSX 声明解释为 React 页面、受控 API、管理后台和 SQLite 持久化应用的 Rust 原生 CLI。

本源码版本为 **0.5.0**，与 **`@kenvoy/types@0.5.0`** 准备同步正式发行，KEngine 保持 **0.2.0**；0.5.0 尚未实际上传或完成该版本的安装验收。当前已发布 CLI 为 **0.4.4**，下列默认安装命令获取实际已发布版本，不将源码版本或 Git 标签当作线上可安装证据；完整发行状态见仓库的渠道进度与版本日志。

## 安装

从源码安装：

```text
cargo install kenvoy --locked
```

需要 Rust 1.98 或以上及对应平台的原生 C 构建环境。发布包已经包含通用前端资产；安装 CLI 和运行基础 DSL 应用不需要先执行 npm 构建。

使用预编译二进制：

```text
cargo binstall kenvoy
```

预编译归档来自 [GitHub Releases](https://github.com/zerobiubiu/kenvoy/releases)，正式发布提供 Windows MSVC、Linux musl、macOS 的 x86_64/aarch64 目标，归档旁附 SHA-256 校验文件。以对应版本的正式 Release 为准；交叉编译产物不代表在全部架构完成运行测试。

## 创建并运行

```text
kenvoy create my-app
cd my-app
kenvoy admin init --username admin
kenvoy dev
```

管理员密码由你自行输入，没有默认账号或默认密码。隐藏输入不显示字符或星号，输入完成后按回车并再次确认。

访问启动日志中的应用地址；未显式声明根页时自动进入当前身份首个可访问页面。默认客户示例的登记页为 `/customers`，后台为 `/admin/customers`。

国际化始终启用，create 生成 languages 的默认中文/英文/韩文、根 language.schema.json 和 translations.json。语言使用代码、可选唯一别名及必填显示名；设备默认语言不支持时使用 fallback。`kenvoy i18n` 更新 Schema、只在缺失时创建带引用的译文文件，不覆盖已有翻译；dev/run 缺少或校验失败时阻断，不能关闭。DSL/内置界面、`useI18n()` 与 0.5.0 新增的 `useLocale()` 共用上下文，业务值及数据库结构不变。0.5.0 的 check/build/dev/run 还在源码可用时预检显式翻译调用，缺键或无法静态确定的动态键会定位源码并阻断。旧可选配置与分语言文件须显式迁移；无源码部署也必须携带匹配的 translations.json。

停止 dev 后可运行：

```text
kenvoy run
```

dev/run 使用同一持久化数据库；不要通过删除 data 目录修复前端或更新问题。`kenvoy build` 只构建产物，不改业务数据库。

`create` 同时固化初始迁移。后续 Schema 变化须先 `kenvoy migrate generate --description "变更说明"`、审查并 `kenvoy migrate seal`；构建产物携带完整链，run 对空库也按顺序逐步执行，不会临时生成迁移。危险候选与执行计划分别要求精确校验值确认，必要时先备份；失败只回滚当前步骤。

配置中的应用版本、最低 CLI 版本和项目格式相互独立；0.5.0 create 将新项目的最低 CLI 写为 **0.5.0**，项目格式仍为 **0.4.0**，不自动提高既有项目的声明。缺少兼容字段的旧配置明确诊断。旧库通过 `migrate baseline --at <步骤ID>` 预览和确认接入，原历史保留。一次性 `dev --snapshot dev-snapshots/try.sqlite` 使用隔离空路径，正式 run 不接受快照。部署必须停旧服务；生命周期锁不能协调未实现该协议的旧 CLI。

`dev`/`run` 支持 Ctrl+C、SIGTERM 与 Windows 控制台关闭事件：先停止接收和开发连接，再按期限回收 watcher/Vite、JavaScript 与 SQLite；重复发送终止信号会立即退出。已接受但结果不明的写入仍需核对。

## 自定义组件与依赖

基础界面使用 React、TypeScript 与原生 CSS。自定义客户端组件首次构建需要 Node 24.22.0 或以上及 npm；只有显式 `--install-deps` 才准备应用声明的用户依赖，不会静默修改应用的 package.json。

0.5.0 将编辑器声明独立交付为同版本的 `@kenvoy/types@0.5.0` npm 开发依赖。新项目自带 TypeScript 配置与精确版本依赖，已有项目加入类型包和 `types: ["@kenvoy/types"]`；`kenvoy types` 已删除，不再生成 `.kenvoy/types`。无需安装重复 React 运行包或占位 `@kenvoy/client`。该类型包尚未首次发布；发行前的源码开发可构建并安装同版真实 tarball，详见仓库 dev 文档。


0.5.0 的默认入口还支持静态 React 函数/箭头函数返回 JSX、局部常量与静态重导出；这不是执行任意 React 代码生成 Schema，动态 UI 与 Hooks 仍属于普通客户端组件。

支持多文件字段、上传图片/URL 图片展示、系统组件重载、联动与跨资源选项、受限服务端 JavaScript。服务端始终执行认证、授权、字段约束和附件归属规则，不能以隐藏 UI 替代权限检查。

## 文档

- [仓库与完整使用说明](https://github.com/zerobiubiu/kenvoy)
- [文档中心](https://github.com/zerobiubiu/kenvoy/tree/dev/docs)
- [示例项目](https://github.com/zerobiubiu/kenvoy/tree/dev/examples)

## 许可证

MIT，见随包提供的 LICENSE。
