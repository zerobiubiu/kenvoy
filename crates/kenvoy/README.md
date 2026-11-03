# Kenvoy

将 TypeScript/JSX 声明解释为 React 页面、受控 API、管理后台和 SQLite 持久化应用的 Rust 原生 CLI。

## 安装

从源码安装：

```text
cargo install kenvoy --version 0.2.1 --locked
```

需要 Rust 1.98 或以上及对应平台的原生 C 构建环境。发布包已经包含通用前端资产；安装 CLI 和运行基础 DSL 应用不需要先执行 npm 构建。

使用预编译二进制：

```text
cargo binstall kenvoy --version 0.2.1
```

二进制归档来自 [GitHub Release v0.2.1](https://github.com/zerobiubiu/kenvoy/releases/tag/v0.2.1)，提供 Windows MSVC、Linux musl、macOS 的 x86_64/aarch64 目标，归档旁附 SHA-256 校验文件。交叉编译产物不代表在全部架构完成运行测试。

## 创建并运行

```text
kenvoy create my-app
cd my-app
kenvoy admin init --username admin
kenvoy dev
```

管理员密码由你自行输入，没有默认账号或默认密码。隐藏输入不显示字符或星号，输入完成后按回车并再次确认。

访问启动日志中的应用地址；未显式声明根页时自动进入当前身份首个可访问页面。默认客户示例的登记页为 `/customers`，后台为 `/admin/customers`。

停止 dev 后可运行：

```text
kenvoy run
```

dev/run 使用同一持久化数据库；不要通过删除 data 目录修复前端或更新问题。`kenvoy build` 只构建产物，不改业务数据库。

`dev`/`run` 支持 Ctrl+C、SIGTERM 与 Windows 控制台关闭事件：先停止接收和开发连接，再按期限回收 watcher/Vite、JavaScript 与 SQLite；重复发送终止信号会立即退出。已接受但结果不明的写入仍需核对。

## 自定义组件与依赖

基础界面使用 React、TypeScript 与原生 CSS。自定义客户端组件首次构建需要 Node 24.22.0 或以上及 npm；只有显式 `--install-deps` 才准备应用声明的用户依赖，不会静默修改应用的 package.json。

支持多文件字段、上传图片/URL 图片展示、系统组件重载、联动与跨资源选项、受限服务端 JavaScript。服务端始终执行认证、授权、字段约束和附件归属规则，不能以隐藏 UI 替代权限检查。

## 文档

- [仓库与完整使用说明](https://github.com/zerobiubiu/kenvoy)
- [文档中心](https://github.com/zerobiubiu/kenvoy/blob/v0.2.1/docs/README.md)
- [示例项目](https://github.com/zerobiubiu/kenvoy/tree/v0.2.1/examples)

## 许可证

MIT，见随包提供的 LICENSE。
