# CLI 当前行为

状态：参考（现状文档）；七个命令、产物运行、notify、HMR 与 CLI Ctrl+C 已通过正式验收，见[生命周期报告](../verification/0011-cli-lifecycle.md)。

## 创建与检查

```text
kenvoy create customer-app
kenvoy --project customer-app check
kenvoy --project customer-app inspect customers
kenvoy --project customer-app inspect
kenvoy --project customer-app inspect --generic
```

create 创建配置、src/app.tsx、README、.gitignore 和 data 目录；不初始化 Git、安装 npm 或创建 SQLite。已有目录（包括空目录）拒绝覆盖，采用原子 create_dir 及 create_new 文件写入；失败保留本次部分产物并报告路径，不删除未知内容。

check 验证语法、模块、DSL、资源、路由、字段与客户端/服务端边界，并准备、验证服务端 JavaScript 模块包；不执行用户函数、不连接数据库。错误给出 KE/KV 代码、路径、行列、建议并返回非零状态。也可离线验证数据：

```text
kenvoy check --resource customers --input record.json
kenvoy check --resource customers --input patch.json --patch
```

--resource/--input 必须配对。Create 补默认值和可选 null，Patch 缺失字段表示不修改；显式 null 仍受可空性限制。输出明确标记未写数据库。

## 查看解析结果

inspect 默认输出宿主 Application IR；指定资源输出字段、表名、API 注册计划、后台地址和权限策略。CLI 没有 Session，不声称当前用户有管理权限。--generic 含源码和宿主路径，只供本地检查，不能直接发给浏览器。

## 数据库迁移

迁移命令直接使用项目的持久化数据库路径：

```text
kenvoy migrate
kenvoy migrate status
kenvoy migrate apply
kenvoy migrate apply --confirm <当前完整checksum>
kenvoy migrate history
```

默认 status 只读预览；数据库不存在时不创建文件。apply 初始化连接，核验漂移并事务应用安全计划；破坏性计划要求绑定当前数据库状态的完整 checksum，先备份再执行。候选改名始终阻断，首版尚无显式映射执行能力。history 不编译源码，语法错误时仍可查询迁移记录。失败返回非零退出码，不静默忽略。

## 账号管理

管理员通过本机受控命令创建，不提供默认密码或公开注册接口：

```text
kenvoy admin init --username admin
kenvoy admin add-user --username operator --role user
kenvoy admin add-user --username maintainer --role admin
```

交互终端隐藏输入并二次确认。自动化可显式添加 `--password-stdin`，只读取一行有效 UTF-8，去掉行结尾而不裁剪密码空格；非交互但未指定此选项会拒绝。密码不得放在命令参数或源码中。用户名和密码限制见 [安全设计](../architecture/security-design.md)。重复 init 不覆盖已有管理员，add-user 要求已初始化管理员，账号和 Session 真实保存在项目 SQLite 中。

## 开发、构建与生产

当前启动与构建命令为：

```text
kenvoy dev
kenvoy build
kenvoy run
```

dev 先构建有效候选，再初始化 SQLite、Auth、QuickJS 和 Axum，输出真实应用与管理登录 Origin。notify 监听源码、配置和受支持资产；180ms 去抖后按内容指纹构建。语法、字段、边界、构建或迁移错误保留最后有效页面与数据库，浏览器显示脱敏开发错误。数据库、输出目录、监听地址或 public_origin 变更要求重启。

仅使用内置 DSL 的项目直接使用 CLI 携带的 React 原生 CSS Runtime，不启动 Node/Vite。自定义组件使用已验证源文件和资产的开发镜像，候选发布后才通知 Vite 更新；浏览器通过同一个 Kenvoy Origin 访问代理与 HMR。正式组件/CSS 状态保持与错误恢复已验收，详见 [运行时设计](../architecture/runtime-design.md)。

build 不连接业务数据库。它检查完整项目、构建客户端及服务端函数包，写入独立版本目录，验证文件清单、大小和哈希后激活 dist/manifest.json。输出包括 application.ir、client/ 和 modules/server.json；旧产物和持久化数据不自动清理。已有非框架管理的输出目录拒绝接管。

run 不要求事先执行 build。源码存在时核对输入指纹、框架兼容性及产物完整性，缺失或过期才构建；失败返回错误，不静默运行旧产物。源码不存在时加载已验证产物，部署只需要 CLI、kenvoy.toml、完整 dist 和独立数据目录。生产不启动 watcher、Vite 或 HMR，开发状态与代理路由返回 404。没有源码时 inspect、admin 和 migrate 可使用产物中的 Application IR；check 和 inspect --generic 仍需要源码。

三个命令都支持显式 --install-deps，为已经声明但缺失的用户 npm 依赖安装提供入口；没有该参数则缺失依赖明确报错。框架使用隔离的锁定工具链缓存，不修改用户 package.json，不读取用户 Vite 配置，不运行用户 npm 生命周期脚本。基础 DSL 和已有完整生产产物不要求 Node。构建缓存命中不重复安装全部依赖。

## 关闭、部署与日志

Ctrl+C 先通知开发协调器停止，在 Axum 等待退出前取消开发 WebSocket，再等待已接受发布任务，随后回收 watcher/Vite、关闭 QuickJS 与 SQLite。真实基础/自定义 CLI 信号、资源关闭、重启数据保留均已验收；早期 Windows 强制终止夹具的证据仍与真实信号测试分开记录。

默认 `server.host=127.0.0.1`、`port=3000`；测试可用端口 0，再按实际绑定端口生成 origin。`server.public_origin` 指定对外 Origin，非回环监听要求显式 HTTPS；不含凭据、业务路径、查询或片段。HTTPS 由部署代理终止，当前 Rust listener 本身没有 TLS，不信任 Forwarded 来推导安全来源。配置详情见 [项目配置](../contracts/project-configuration.md)。

全局 `--json-logs` 选择结构化 tracing 日志，密码、Cookie、token 和表单正文不入日志。账户子命令属于本机管理边界，不依赖浏览器登录状态。

生命周期和发布失败边界见 [Runtime 设计](../architecture/runtime-design.md)。阶段 05 的历史 HTTP 验证保留于 [验证记录](../verification/0007-http-auth.md)，阶段 08 的组件、函数和模块隔离见 [扩展验证](../verification/0010-typescript-extensions.md)；这些历史记录不等同于阶段 09 最终验收。
