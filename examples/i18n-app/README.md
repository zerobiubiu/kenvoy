# 原生 i18n 报名示例

此示例展示 Kenvoy 原生国际化：DSL 默认文案、根目录集中中英文译文、内置表单/后台/错误、框架语言选择器、`useI18n()` 和 `useLocale()` 共用一套语言状态。原生国际化始终启用，没有第三方 i18n 库、应用 Provider 或自建语言路由。

本例的函数模板入口和 `useLocale()` 要求使用本仓库 dev 源码构建的 CLI，尚未包含在已发布的 0.4.4 中；项目格式保持 **0.4.0**。若未加入 PATH，将下文 `kenvoy` 替换为仓库 `target/debug/kenvoy`（Windows 为 `.exe`）的路径。

## 运行

本例项目依赖统一使用 Bun 1.4.3，bun.lock 只在本地生成或使用、不提交 Git。编辑器声明由 `@kenvoy/types` 开发依赖提供，安装前按[示例总览](../README.md)构建同 CLI 版本的本地 tarball，不再执行 `kenvoy types`。React 运行时、SDK 运行值与框架内部工具链仍由 CLI 管理，自定义客户端首次构建仍需 Node.js 24.22+ 及配套 npm；不要用当前仍走 npm 的 `--install-deps` 准备本例依赖。

```text
cd examples/i18n-app
bun install --ignore-scripts --omit=peer
kenvoy i18n
kenvoy check
kenvoy dev
```

访问 `http://127.0.0.1:3000/register`。每次打开或刷新按设备默认语言 `navigator.language` 精确匹配配置代码，忽略大小写；没有匹配或不可读取时使用 `fallback = "zh-CN"`。不会把 `en-GB` 猜成 `en-US`，不会继续查找 `navigator.languages` 中的其他偏好，也不读取 localStorage 中的历史语言。页头可手动选择“简体中文”或“English”，标签来自配置的 `display`，当前页面内保留选择；刷新后重新按设备默认语言选择。其他示例同样默认使用 3000 端口，不能同时占用；需要并行时显式修改一个示例的端口。

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
| `kenvoy.toml` | 声明最低 CLI/项目格式 0.4.0，以及 `[languages]` 的回退代码、语言代码、别名和显示名称 |
| `src/app.tsx` | 用 `export default function App()` 直接返回静态业务声明，保留资源、标签、页面、导航及权限，不执行组件或在 Schema 中调用 Hook |
| `src/I18nDemo.tsx` | 从 `@kenvoy/client` 使用 useLocale 读取当前语言，结合 useI18n 演示命名参数、复数、缓存 Intl 格式化和 Hooks 状态 |
| `translations.json` | 集中保存每个词条的全部语言译文，包含必需的 `$schema` 引用 |
| `language.schema.json` | 按语言配置生成的 Draft 2020-12 JSON Schema，约束完整语言覆盖和代码/别名互斥 |
| `migrations/manifest.json`、`migrations/steps/000001.json` | 从空 Schema 建立报名资源的正式迁移，不依赖部署时业务函数 |
| `.gitignore` | 排除数据、构建输出、工具链缓存和依赖目录 |

例如字段 `name` 自动使用 `resources.registrations.fields.name.label`；选项 `afternoon` 使用 `resources.registrations.fields.session.options.afternoon.label`；页面使用 `pages./register.title`。分组/导航标签使用 `text:<原文>`，自定义组件使用 `demo.*`。不修改资源标识或选项值来翻译界面。

内置按钮可通过 `ui.form.submit`、`ui.form.another` 覆盖，本例分别显示“提交报名/Submit registration”和“再报一名/Register another attendee”，不需要重载整个表单。

## 维护词典与交付

```text
kenvoy i18n
kenvoy check
```

`kenvoy i18n` 先校验语言配置，再更新 `language.schema.json`，相同内容不会重复写入。仅当根目录 `translations.json` 不存在时创建它，初始内容只有 `{"$schema":"./language.schema.json"}`；已有文件绝不覆盖。最后使用生成的 JSON Schema 校验译文，不提取源码、不生成业务词条，也不自动翻译。

本例配置 `zh-CN` 的别名 `cn`、`en-US` 的别名 `en`，并在不同词条间混用代码与别名。例如 `app.name` 使用 `zh-CN` / `en-US`，`resources.registrations.label` 使用 `cn` / `en`，`resources.registrations.fields.name.label` 使用 `zh-CN` / `en`。每个词条都完整覆盖两种语言；同一词条中，同一种语言只能使用代码或别名之一，不能同时使用。手动调用 `setLocale('cn')` 或 `setLocale('en')` 时也会解析为规范语言代码。

词条名由业务自由定义；除了 `$schema`，顶层没有固定必填词条。`title`、`description`、`content` 不是保留必填字段。未写入集中译文的 DSL 文案继续使用源码默认文字，系统文案使用框架内置中文/英文回退；已存在的词条则必须为全部配置语言提供非空字符串，不允许缺语言、未知语言、重复 JSON 键或同时出现同一语言的代码与别名。本例保留英文 `demo.people.one`，并为它补齐中文“演示计数：{count} 人”；中文复数实际使用 `demo.people.other`。

`dev` / `run` 先更新 Schema，再严格校验根译文及源码显式 `t(...)` 引用；漏词条、无法证明完整集合的动态键、缺失译文文件、错误 `$schema` 或非法语言矩阵，都会在数据库迁移和用户代码运行前阻断。`check` / `build` 同样检查源码引用；传 fallback 也不能豁免漏词条。需要新增语言时，修改配置后执行 `kenvoy i18n`，并手工为所有已有词条补齐该语言；命令不会替你填入译文。词典不可存放密钥、私人说明或业务记录。

修改词典或标签不生成数据库迁移；dev 按候选校验后发布，损坏候选保留旧运行态。修改字段结构时使用正常 generate/seal 流程并停服重启，不删除数据库。完整 `dist` 包含私有规范化词典和正式链；无源码生产交付仍须同时携带匹配产物的 `kenvoy.toml`、根目录 `translations.json` 和完整 `dist/`，不能只依赖产物内词典跳过根译文校验。可不携带 `src/` 和源码迁移目录，`language.schema.json` 可由 `run` 按配置重建。根译文变动后应从源码重新构建匹配产物，不能修改根译文后继续冒用旧产物。

完整规则见[原生国际化契约](../../docs/contracts/i18n.md)、[SDK 与字段绑定](../../docs/contracts/component-bindings.md)、[配置](../../docs/contracts/project-configuration.md)和[迁移操作](../../docs/operations/migration-design.md)。
