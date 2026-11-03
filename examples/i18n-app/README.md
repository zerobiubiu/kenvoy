# 原生 i18n 报名示例

此示例展示 Kenvoy 原生国际化：DSL 默认文案、中英文业务词典、内置表单/后台/错误、框架语言选择器和 `useI18n()` 共用一套语言状态。没有第三方 i18n 库、应用 Provider 或自建语言路由。

要求 **Kenvoy CLI 0.3.0 或以上兼容版本**。0.3.0 当前尚未发布，请使用本仓库源码构建的 CLI；若未加入 PATH，将下文 `kenvoy` 替换为仓库 `target/debug/kenvoy`（Windows 为 `.exe`）的路径。

## 运行

本例包含自定义 React SDK 演示组件，首次构建需要 Node.js 24.22+ 与 npm。React、SDK 和锁定工具链由框架管理；本例没有项目 npm 依赖，不需要 `--install-deps`。原生 i18n 本身不要求 Node，额外要求来自这个可交互 SDK 组件。

```text
cd examples/i18n-app
kenvoy check
kenvoy i18n check
kenvoy dev
```

访问 `http://127.0.0.1:3000/register`。语言按已保存偏好、浏览器语言和项目默认语言选择，可从页头手动切换。其他示例同样默认使用 3000 端口，不能同时占用；需要并行时显式修改一个示例的端口。

正式迁移 `000001` 已固化，空库首次启动按完整链创建 `data/app.sqlite`；不需要再次生成迁移。停止 dev 后可执行 `kenvoy build` 和 `kenvoy run`，两种模式使用同一数据路径，不因切换而重置。

## 建议体验顺序

1. 点击“增加演示计数”，将计数从 1 加到 2；切换 English，观察 `1 person / 2 people` 的复数及计数保留。
2. 填写报名姓名，选择“下午场”，将通知设为“否”；切回中文再切英文，输入和选择保持原值。
3. 留空姓名提交，观察内置必填错误随语言变化；正确填写后提交一次，看到真实保存结果与记录编号。
4. 检查示例数字、日历日期和时间的展示。时间示例输入带 `+08:00`，显示默认 UTC；切换语言不切换时区。浏览器自身日期选择器的年月日占位和弹层仍由浏览器/操作系统本地化。
5. 停服后创建管理员，再进入后台验证记录：

   ```text
   kenvoy admin init --username admin
   kenvoy dev
   ```

   按隐藏提示自行设置密码，没有默认密码。登录 `/admin/login` 后访问 `/admin/registrations` 或受保护的 `/registrations`，表头、选项和布尔展示随语言变化，业务原文保持不变。匿名仅能通过明确声明的表单报名，不能读取、修改或删除报名记录。

演示计数只存在组件内存中，不是表单的“同行人数”，也不会写库。表单实际保存数值 `guests`、选项 `morning/afternoon`、布尔 `newsletter` 和 ISO 日期 `arrival`；不会保存翻译后的 Morning、下午场、Yes 或 No。

## 文件与原生接线

| 文件 | 职责 |
|---|---|
| `kenvoy.toml` | 声明最低 CLI/项目格式 0.3.0、默认语言、支持列表和词典目录 |
| `src/app.tsx` | 正常编写业务资源、默认标签、页面、分组、导航和权限，不在 Schema 中调用 t |
| `src/I18nDemo.tsx` | 直接从 `@kenvoy/client` 使用 useI18n，演示命名参数、复数、缓存 Intl 格式化和 Hooks 状态 |
| `locales/zh-CN.json`、`locales/en-US.json` | 平面业务词典及少量内置提交按钮覆盖；其余系统文案使用框架内置中英文 |
| `migrations/manifest.json`、`migrations/steps/000001.json` | 从空 Schema 建立报名资源的正式迁移，不依赖部署时业务函数 |
| `.gitignore` | 排除数据、构建输出、工具链缓存和依赖目录 |

例如字段 `name` 自动使用 `resources.registrations.fields.name.label`；选项 `afternoon` 使用 `resources.registrations.fields.session.options.afternoon.label`；页面使用 `pages./register.title`。分组/导航标签使用 `text:<原文>`，自定义组件使用 `demo.*`。不修改资源标识或选项值来翻译界面。

内置按钮可通过 `ui.form.submit`、`ui.form.another` 覆盖，本例分别显示“提交报名/Submit registration”和“再报一名/Register another attendee”，不需要重载整个表单。

## 维护词典与交付

```text
kenvoy i18n extract
kenvoy i18n extract --locale en-US
kenvoy i18n check
```

extract 只输出当前声明默认文本与内置系统词条，不自动翻译业务内容，也不会合并或覆盖本例已维护的词典。先审查输出，再按键合并；原始 JSX 文本可能包含空白，使用实际提取键，不手工猜测。

单条缺失按当前语言、默认语言及框架文案回退；`i18n check` 报告原生键覆盖，损坏词典、重复键和整份非默认文件缺失明确失败。词典不可存放密钥、私人说明或业务记录。

修改词典或标签不生成数据库迁移；dev 按候选校验后发布，损坏候选保留旧运行态。修改字段结构时使用正常 generate/seal 流程并停服重启，不删除数据库。完整 `dist` 包含私有词典和正式链，生产部署可不携带 `src/`、`locales/` 和源码迁移目录，但 `kenvoy.toml` 必须与产物身份匹配。

完整规则见[原生国际化契约](../../docs/contracts/i18n.md)、[SDK 与字段绑定](../../docs/contracts/component-bindings.md)、[配置](../../docs/contracts/project-configuration.md)和[迁移操作](../../docs/operations/migration-design.md)。本例的实际验证归入[0027 实施记录](../../docs/changes/0027-native-project-i18n.md)。
