# 客户管理扩展示例

此示例保留真实客户登记与 SQLite 持久化，同时展示 Schema Component、React Hooks、Props、内联回调捕获、门面模块 CSS、第三方 `clsx`，以及 TypeScript ES Module 异步服务端动作。

首次运行需要 Kenvoy CLI 0.4.0 或以上兼容版本、Node.js 24.22 或更高的兼容版本及配套 npm；项目格式为 0.4.0。在本目录执行：

```text
kenvoy check
kenvoy dev --install-deps
```

`--install-deps` 明确允许安装本示例 package.json 已声明并由 package-lock.json 锁定的 `clsx 2.1.1` 和 `@radix-ui/react-slot 1.5.0`。React、SDK 与 Vite 由框架缓存管理，不必添加到用户依赖，也不用编写 Vite 配置。

访问 `http://127.0.0.1:3000/customers`。点击“增加数量”调用捕获 `step` 常量的客户端回调；点击“计算折扣”经同源 CSRF/Session 通道执行服务端函数，返回 0.8。表单提交写入项目 `data/app.sqlite`，与组件内的临时计数状态分开。

停止后可直接再次执行 `kenvoy dev`。生产模式执行 `kenvoy run`；存在源码而产物过期时自动构建，不启动文件监听。已经安装的 npm 依赖复用，不需要重复 `--install-deps`。

管理数据前先在本目录执行 `kenvoy admin init --username admin`，按隐藏交互提示设置密码，然后登录 `/admin/login` 并访问 `/admin/customers`。没有默认管理员密码。此示例仅明确开放登记表单提交与纯计算动作；匿名列表、详情、修改和删除仍然禁止。

`CustomerFields.diu.tsx` 参与 Schema 推导，普通 `CustomerCard.tsx` 不参与。`discount.server.ts` 和仅被它依赖的 `math.ts` 不进入浏览器产物；`app.tsx` 的内联回调通过受控词法切片注册，不把整个声明入口打包到浏览器。不要在客户端组件或其 Props 中填写服务端密钥。

本例通过 `[languages]` 显式启用 `zh-CN`，显示名称为“简体中文”。根目录 `translations.json` 仅保留必需的 `$schema` 引用，界面沿用源码默认文案和框架内置中文，原生国际化仍然启用。执行 `kenvoy i18n` 可按配置更新根目录 `language.schema.json` 并校验译文；不会覆盖已有词条或自动翻译。完整规则见[示例总览](../README.md)。

部署和接口边界见[前端 Runtime](../../docs/architecture/frontend-runtime.md)、[JavaScript Runtime](../../docs/architecture/javascript-runtime.md)和[配置规范](../../docs/contracts/project-configuration.md)。
