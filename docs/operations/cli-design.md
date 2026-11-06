# CLI 当前行为

状态：参考（现状文档）。正式迁移、兼容门禁、旧库接入和快照见 [迁移操作](migration-design.md)；早期生命周期证据见[原验收报告](../verification/0011-cli-lifecycle.md)，不将历史动态迁移行为作为当前契约。

本文能力对应已发布的 **Kenvoy CLI 0.5.0 与 `@kenvoy/types@0.5.0`**，KEngine 保持 0.2.0。真实注册表安装、CLI 创建及默认类型接线已通过，发行与验证边界见 [0044](../changes/0044-cli-0.5.0-and-types-first-publish.md)。

## 创建与检查

```text
kenvoy create customer-app
kenvoy --project customer-app check
kenvoy --project customer-app inspect customers
kenvoy --project customer-app inspect
kenvoy --project customer-app inspect --generic
```

create 创建配置、src/app.tsx、README、.gitignore、data 目录、已固化初始迁移及根 language.schema.json、translations.json；0.5.0 还创建编辑器 tsconfig.json 和声明同 CLI 精确版本 `@kenvoy/types` 的私有 package.json，不再生成 `.kenvoy/types`。项目格式为 0.4.0，最低 CLI 声明取创建时的 CLI 版本（本次源码为 0.5.0），languages 默认中文/英文/韩文，国际化始终启用。既有项目的最低 CLI 不自动提升。不初始化 Git、安装依赖或创建 SQLite。已有目录（包括空目录）拒绝覆盖；写入失败保留本次部分内容并报告，不删除未知文件。

check 验证语法、模块、DSL、资源、路由、字段、词典与客户端/服务端边界，并准备、验证服务端 JavaScript 模块包；不执行用户函数、不连接数据库。0.5.0 还校验源码显式翻译调用的静态键集合，缺键或无法证明完整集合的动态键都会阻断，传 fallback 不能豁免。错误给出 KE/KV 代码、路径、行列、建议并返回非零状态。也可离线验证数据：

```text
kenvoy check --resource customers --input record.json
kenvoy check --resource customers --input patch.json --patch
```

--resource/--input 必须配对。Create 补默认值和可选 null，Patch 缺失字段表示不修改；显式 null 仍受可空性限制。输出明确标记未写数据库。

## 编辑器类型

以下能力要求 **Kenvoy CLI 0.5.0 与同版 `@kenvoy/types`**。编辑器类型已由独立公开 npm 包提供；新项目自动写入精确版本开发依赖。CLI 不再内嵌或导出编辑器声明，旧 `kenvoy types` 命令已删除。

已有项目保留原配置，只做以下接线：

1. 在 `devDependencies` 加入与当前 CLI **相同精确版本**的 `@kenvoy/types`，保留框架匹配版本的 `typescript`、`@types/react`、`@types/react-dom`。
2. 删除原先指向 `.kenvoy/types` 的三个虚拟模块 paths，加入以下设置。
3. 由项目包管理器安装开发依赖，再执行 `typecheck`；CLI 升级时同步更新类型包版本。

```json
{
  "compilerOptions": {
    "types": ["@kenvoy/types"],
    "jsx": "react-jsx",
    "jsxImportSource": "@kenvoy/client"
  }
}
```

默认配置继续采用 strict、noEmit、Bundler，不设置 baseUrl、skipLibCheck 或任意标签 any 索引。类型包从真实 SDK 源码和两个 JSX runtime 根生成声明，用环境模块连接 `@kenvoy/client`、生产与开发 JSX runtime；不提供 JavaScript，也不污染普通 `React.JSX`。React 运行包仍由 Kenvoy 共享，不安装重复 React 或占位 npm `@kenvoy/client`。

**0.5.0 已完成首次正式发布。** 普通项目执行 `npm install --save-dev --save-exact @kenvoy/types@0.5.0`，或安装 CLI 已生成清单中的开发依赖，再运行 typecheck，不需要先执行 CLI 生成声明。已发布 0.4.4 不含新接口和类型目录，不能把新包当作其兼容声明或用 HEAD 回填；版本与实际安装记录见 [0044](../changes/0044-cli-0.5.0-and-types-first-publish.md)。

源码开发在仓库根执行：

```text
npm --prefix crates/kenvoy/frontend run build:types
```

该命令生成 `frontend/dist/types-package`、带 CLI 版本与内容摘要的本地 tarball 及侧车；`frontend/dist/types-artifact.json` 记录实际文件名、完整摘要和版本。构建自动同步仓库五个 Bun 示例的该开发依赖，SDK 或 CLI 版本变化换用新路径，避免 Bun 的同路径旧包缓存，不改写独立用户应用。独立应用将实际 tarball 放入项目后，可把 `@kenvoy/types` 开发依赖设为 `file:./实际文件名.tgz` 再安装。正式 Release 使用 `kenvoy-types-X.Y.Z.tgz`，来源只能是相同版本已发布的 CLI 源码包。

清理 `.kenvoy` 不影响类型包。编辑器仍显示缓存诊断时，重新加载 TypeScript 项目或重启 TS Server，并选择项目安装的 TypeScript 版本。`typecheck` 检查编辑器/API 类型，`kenvoy check` 检查声明语义、权限、Schema 和模块边界，两者不能互相替代。切换及实际包消费记录见 [0042](../changes/0042-published-editor-types-package.md)。


## 查看解析结果

inspect 默认输出宿主 Application IR；指定资源输出字段、表名、API 注册计划、后台地址和权限策略。CLI 没有 Session，不声称当前用户有管理权限。--generic 含源码和宿主路径，只供本地检查，不能直接发给浏览器。

## 原生国际化工具

```text
kenvoy i18n
```

命令先检查 languages 与兼容配置，再更新根 language.schema.json；已有相同 Schema 不重写。translations.json 缺失时独占创建，仅包含 `"$schema":"./language.schema.json"`；已存在则不覆盖。随后按最新生成的 draft 2020-12 Schema 校验引用及每个词条完整的非空语言矩阵，代码与别名只能选一个。无需源码或产物，不编译应用或连接数据库；旧 extract/check 子命令已移除。

dev/run 启动时重复配置、Schema 和译文检查，但不创建 translations.json；缺失时提示该命令或手工带引用创建。坏文件在数据库创建、迁移和应用代码执行前阻断。0.5.0 的 check/build/dev/run 在源码可用时还预检显式 `t(...)` 引用；无源码 run 仍要求根译文与已验证产物匹配，不重新扫描不存在的源码。`useLocale()` 读取与 `useI18n().locale` 相同的规范语言代码，不建立第二套状态。详见[原生语言契约](../contracts/i18n.md)。


## 数据库迁移

迁移命令直接使用项目的持久化数据库路径：

```text
kenvoy migrate
kenvoy migrate status
kenvoy migrate generate --description "增加客户电话"
kenvoy migrate generate --description "清洗客户数据" --rules rules.json
kenvoy migrate seal
kenvoy migrate seal --confirm <候选完整checksum>
kenvoy migrate apply
kenvoy migrate apply --confirm <当前完整checksum>
kenvoy migrate history
kenvoy migrate baseline --at <基线步骤ID>
kenvoy migrate baseline --at <基线步骤ID> --rules rules.json --confirm <接入报告checksum>
```

默认 status 只读核对正式链与数据库；数据库不存在时不创建文件。generate 比较上一正式 Schema 与当前受检 IR，seal 验证并追加固化候选，两者不连接业务数据库。apply 只执行固化链，逐步事务提交，危险计划要求当前执行 checksum 和一致性备份；显式规则处理改名、回填、转换及断言，不猜测业务意图。baseline 单独报告与确认，不伪造历史。history 不编译源码、不要求版本门禁通过、不读取译文，但 kenvoy.toml 仍须可解析且路径/语言配置有效；输出 applied、baseline 和 legacy_history。旧配置语法或已移除字段须先按诊断转换，不能把“绕过版本门禁”理解为忽略配置解析。规则见[迁移执行协议](migration-design.md)。

## 账号管理

管理员通过本机受控命令创建，不提供默认密码或公开注册接口：

```text
kenvoy admin init --username admin
kenvoy admin add-user --username operator --role user
kenvoy admin add-user --username maintainer --role admin
```

交互终端隐藏输入并二次确认，输入时不显示字符或星号，输入完成后按回车。自动化可显式添加 `--password-stdin`，只读取一行有效 UTF-8，去掉行结尾而不裁剪密码空格；非交互但未指定此选项会拒绝。密码不得放在命令参数或源码中。用户名和密码限制见 [安全设计](../architecture/security-design.md)。重复 init 不覆盖已有管理员，add-user 要求已初始化管理员，账号和 Session 真实保存在项目 SQLite 中。

交互提示通过标准错误的 Rust Unicode 终端输出并立即刷新，Windows 不依赖当前控制台输出代码页，也不修改它。隐藏读取仍由 rpassword 完成。提示乱码修复不放宽密码限制：校验错误应按原规则处理，而不是重建数据库或改成明文输入；修复及原生控制台证据见 [0017](../changes/0017-windows-password-prompt-encoding.md)。

## 开发、构建与生产

当前启动与构建命令为：

```text
kenvoy dev
kenvoy build
kenvoy run
kenvoy run --confirm <status 的 confirmation_checksum>
kenvoy dev --snapshot dev-snapshots/try.sqlite
```

dev 先完成兼容性、languages 与根译文预检，再构建候选并按正式迁移链初始化 SQLite、Auth、QuickJS 和 Axum。notify 监听源码、配置、译文、迁移及受支持资产，派生语言 Schema 不自激重建。坏配置、译文、源码、构建或迁移候选保留最后有效页面和数据库。数据/输出/监听路径、正式链或物理 Schema 变更须停服重启；未固化结构不静默写库，隔离开发快照不能直接用于 run。

启动日志中的应用 Origin 可直接访问：未声明 `/` 页面时，根地址加载 Runtime 并进入当前身份的首个可访问页面，无可访问页面则显示登录页；显式根页仍按声明权限处理。未知页面、API 和私有文件不会因此开放或返回页面壳。

仅使用内置 DSL 的项目直接使用 CLI 携带的 React 原生 CSS Runtime，不启动 Node/Vite。自定义组件使用已验证源文件和资产的开发镜像，候选发布后才通知 Vite 更新；浏览器通过同一个 Kenvoy Origin 访问代理与 HMR。正式组件/CSS 状态保持与错误恢复已验收，详见 [运行时设计](../architecture/runtime-design.md)。

build 不连接业务数据库，也不补迁移。它检查完整项目、集中译文和正式链，构建客户端与服务端包，验证后激活格式 4 的 dist/manifest.json。输出包含 application.ir、client/、modules/server.json、migrations.json 与规范化私有 i18n.json；旧产物和持久化数据不自动清理，未知输出目录拒绝接管。

run 不要求事先 build。源码存在时校验输入与产物身份，缺失或过期才构建；失败不静默运行旧产物。无源码部署需要 CLI、匹配的 kenvoy.toml、根 translations.json、完整 dist 和独立数据目录，不需要源码或 migrations/。Schema 可在启动时重建，但缺根译文仍阻断，规范化内容不得与产物不同。空库执行完整迁移链；生产不启动 watcher/Vite/HMR，开发端点为 404。inspect、admin 和 migrate status/apply 在无源码时可使用产物；i18n 只需配置，check、inspect --generic、迁移生成/固化仍需要源码。

三个命令都支持显式 --install-deps，为已经声明但缺失的用户 npm 依赖安装提供入口；没有该参数则缺失依赖明确报错。框架使用隔离的锁定工具链缓存，不修改用户 package.json，不读取用户 Vite 配置，不运行用户 npm 生命周期脚本。基础 DSL 和已有完整生产产物不要求 Node。构建缓存命中不重复安装全部依赖。

## 关闭、部署与日志

`dev`/`run` 在启动构建前安装终止信号监听。Unix 同时处理 SIGINT 与 SIGTERM，Windows 处理 Ctrl+C、Ctrl+Break、控制台关闭与注销。首次信号取消启动构建（如仍在构建），或停止开发协调器并关闭开发 WebSocket；HTTP 停止接收后最多等待 10 秒排空连接，开发协调器、JavaScript 与 SQLite 各有 15 秒关闭期限，Vite 控制/进程/日志回收另有更短期限。正常关闭会删除本次实例独占的 `.kenvoy/client-dev` 目录。第二次信号立即以 130 退出；首次信号后 75 秒仍未完成也会强制结束，防止关闭阶段永久挂起。基础/自定义 CLI 的真实 Ctrl+C、端口/子进程/临时目录关闭已在当前构建复核；Unix SIGTERM、构建中途信号和超时强制路径仍未实测。早期 Windows 强制终止夹具的证据仍与真实信号测试分开记录。

默认 `server.host=127.0.0.1`、`port=3000`；测试可用端口 0，再按实际绑定端口生成 origin。`server.public_origin` 指定对外 Origin，非回环监听要求显式 HTTPS；不含凭据、业务路径、查询或片段。HTTPS 由部署代理终止，当前 Rust listener 本身没有 TLS，不信任 Forwarded 来推导安全来源。配置详情见 [项目配置](../contracts/project-configuration.md)。

全局 `--json-logs` 选择结构化 tracing 日志，密码、Cookie、token 和表单正文不入日志。账户子命令属于本机管理边界，不依赖浏览器登录状态。

迁移和服务全程持有数据库生命周期锁，另一个新 CLI 的服务、迁移写入和账号管理不能并行使用该库；未实现协议的旧 CLI 仍须通过部署流程停服。应用已完成迁移但激活失败会明确报告“数据库已迁移，应用未就绪”，不自动回滚早期步骤或覆盖备份。就绪/失败及应用版本、构建哈希、CLI 版本和迁移位置追加记录到 `<数据库>.kenvoy-startups.jsonl`，真实迁移位置仍以 SQLite 成功记录为准。

生命周期和发布失败边界见 [Runtime 设计](../architecture/runtime-design.md)。阶段 05 的历史 HTTP 验证保留于 [验证记录](../verification/0007-http-auth.md)，阶段 08 的组件、函数和模块隔离见 [扩展验证](../verification/0010-typescript-extensions.md)；这些历史记录不等同于阶段 09 最终验收。
