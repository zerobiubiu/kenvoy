# npm 单包按平台下载与 CLI 0.4.3

状态：已执行（单包实现、0.2.0 正式 npm 首发与安装核验、0.4.3 本地验证；旧分包撤销及 OIDC 待完成）。影响范围：npm 安装与发布流程、CLI 发布版本及渠道操作文档。前置：[0032](0032-retire-linux-and-winget-channels.md)。本记录替代 0024、0029 中的 npm 平台分包方案，不改写历史实施事实。

## 背景与授权范围

用户明确要求只发布 `kenvoy` 一个 npm 包，在安装时按平台下载官方二进制，而不是维护六个平台包及其主包。旧方案已发布 `kenvoy-win32-x64@0.2.0`，注册表另有 `0.0.0-stage` 占位；Windows ARM64 上传未完成，其余平台分包未上传。

本轮获得的授权分别为：

- 改为单包实现；撤销整个旧 `kenvoy-win32-x64` 包及其两个版本，接受已使用版本号不能重用的后果。
- 使用已经正式发行的 0.2.0 六平台 ZIP 手动建立 `kenvoy@0.2.0`，不将当前源码冒充旧版本。
- 当前源码 CLI 递增至 0.4.3，最终代码、测试和文档一次本地提交并维护新标签；不推送、不合并 main，不创建新的 GitHub Release 或发布 crate。
- npm OIDC 由用户稍后自行配置；认证链接交给用户，随后改为用户本人终端执行发布，不再使用浏览器工具，不收取密码、验证码或 token。

## 实施方案

### 一个包、一个平台下载

生成器只输出 `kenvoy/`。npm tarball 不含平台二进制，不含 `kenvoy-*` 依赖，包含安装器、运行入口、下载清单与原始许可证。

下载清单记录六组系统/架构的实际官方 URL、ZIP 摘要和大小、归档成员、二进制摘要和大小。`postinstall` 只选当前平台，下载一个 ZIP，依次核验 ZIP 和二进制，原子写入 `vendor/<binarySha256>/<executable>`。内容寻址防止升级时误运行前一版本残留文件；运行入口不联网，也不回退到旧扁平目录。

下载器仅允许 HTTPS 及指定 GitHub 下载主机，限制重定向、总超时和空闲超时，遵循代理与 NO_PROXY，保留 TLS 校验，不转发 npm 凭据。归档拒绝重复成员、路径穿越和不符的大小/摘要；目标目录和文件拒绝符号链接，失败不覆盖已有可用文件。Unix 设置可执行权限。

运行入口保留参数、标准流和退出码。Windows 控制台已广播 Ctrl+C，wrapper 不再对同一子进程额外 `kill(SIGINT)`，避免将优雅退出变为强制终止。

### 工程与自动发布

安装器工程依赖单独锁定为 `fflate@0.8.2`、`https-proxy-agent@7.0.6`、`proxy-from-env@1.1.0`，来源为 npm，均为 MIT；声明 Node.js >=18，不进入前端 Runtime 或 Cargo 产品依赖。

`publish_npm.py` 改为 `--package <输出目录/kenvoy>`，移除平台包枚举和旧参数；默认只读，显式发布仍受 main 门禁限制。工作流只上传一个包，Homebrew/Scoop 路径不变。手动首发直接使用 npm，不伪造 GitHub 分支变量绕过门禁。

源码 CLI 为 0.4.3，KEngine 为 0.2.0；项目格式仍为 0.4.0。新建项目最低 CLI 取创建时的 0.4.3，不自动修改已有项目的最低版本。

### npm 12 安装脚本审批

npm 12 安装退出码 0 可能仅表示包已解压，不能证明 postinstall 已运行。注册表全局安装应仅批准本包：

```text
npm install -g kenvoy --allow-scripts=kenvoy
kenvoy --version
```

局部项目使用 `npm install-scripts approve kenvoy` 后 `npm rebuild kenvoy`；局部安装不接受 `--allow-scripts` 参数。验证本地 tarball 时，审批键是精确的 `file:` 来源，而不是注册表包名；本轮通过独立临时前缀和精确来源审批验证，没有改动用户全局审批策略。

### 首发状态与 OIDC 边界

用户在本机终端执行冻结的 `kenvoy-0.2.0.tgz`。2026-11-04 20:41 UTC 的 npm 日志显示认证完成（done 返回 200）、上传受理（PUT 返回 202）、命令退出 0，并提示包仍在处理。随后注册表仅出现描述为 `Temporary package placeholder for staged publishing` 的 `kenvoy@0.0.0-stage`；精确 `0.2.0` 当时尚未公开。

本轮随后确认 `kenvoy@0.2.0` 已公开，latest 为 0.2.0，注册表 SHA-1/SHA-512 与冻结候选完全一致。Windows x64 与 Linux x64 均使用隔离凭据配置，从正式注册表全新全局安装，真实执行输出 `kenvoy 0.2.0`，安装后二进制摘要与清单一致。npm 12 只批准 `kenvoy` 的安装脚本，没有全局放行其它依赖。此前的 202 及占位状态只代表受理，不等于正式发行或 OIDC 已配置。

旧 `kenvoy-win32-x64` 撤销两次均遇认证会话超时；最后实查仍含 `0.0.0-stage`、`0.2.0`，不宣称已删除。只有该旧包在撤销授权内，不能删除新建的 `kenvoy`。

后续只给 `kenvoy` 绑定一个 Trusted Publisher：GitHub `zerobiubiu/kenvoy`、工作流文件 `channels.yml`、environment 留空，允许直接 npm publish；不能仅允许 staging。是否完成绑定及首次 OIDC 发布须独立取证。

## 改动文件

| 文件或目录 | 变更 |
|---|---|
| `.github/scripts/channels/npm_packages.py`、`publish_npm.py` | 单包生成、六目标固定下载清单、单包只读计划及发布 |
| `.github/scripts/channels/npm_runtime/` | 安装器、下载/归档/原子文件处理、运行入口、工程依赖和锁文件 |
| `.github/scripts/channels/tests/test_npm_packages.py`、`test_publish_npm.py` | 单包产物、坏归档、错误处理、发布前提回归 |
| `.github/scripts/channels/tests/npm_runtime*.cjs`、`npm_runtime_console_helper.py` | 平台、摘要、真实本地 HTTPS/代理、文件保护及进程信号行为 |
| `.github/workflows/channels.yml` | 只生成并发布 `kenvoy/`，使用新参数 |
| `crates/kenvoy/Cargo.toml`、`Cargo.lock` | CLI 0.4.3 |
| `AGENTS.md`、根及 CLI README | 单包规则、版本与发行说明 |
| `docs/contracts/project-configuration.md`、`docs/operations/cli-design.md` | 新建项目最低 CLI 示例与独立格式版本 |
| `docs/operations/distribution-channels.md`、`docs/development/channel-rollout-status.md` | 单包操作、脚本审批、凭据、真实首发状态与 OIDC 后续 |
| `docs/development/engineering-workflow.md`、`docs/reference/technology-stack.md`、`docs/reference/README.md` | 安装器工程入口、依赖和来源 |
| `docs/changes/CHANGELOG.md`、本文及文档/变更/验证索引 | 0.4.3 说明、执行证据和替代关系 |

## 执行验证记录

执行环境：Windows 11 x64，Node.js 24.22.0/npm 12.2.0，uv 管理的 CPython 3.12；Linux 实测使用既有 WSL Ubuntu 24.04，临时准备官方 Node.js 24.22.0 Linux x64（附带 npm 11.19.1）。Linux Node 归档 SHA-256 为 `cc2f601cee82d9a5a233302fe307abc600727035fd7d2c3a3571755989e95008`，已与官方 SHASUMS 核对；不安装系统级 Node、不改用户网络配置。

| 验证命令或场景 | 实际结果与结论 |
|---|---|
| `uv run --no-project --python 3.12 python -B -m unittest discover -s .github/scripts/channels/tests -v` | 61 项，OK，1 项因 Windows 符号链接权限跳过 |
| 设置 `KENVOY_TEST_PYTHON` 后执行 `node --test` 三个 npm_runtime 测试文件 | 114 项：110 通过、0 失败、4 跳过；跳过为 Windows 不适用的 Unix 执行/信号或符号链接场景 |
| Windows 原生隐藏控制台 Ctrl+C | 修复前直接子进程退出 37、记录完成，wrapper 强杀后退出 1且无完成记录；修复后原生控制台回归通过 |
| 旧二进制残留场景 | 修复前新版本清单可运行扁平目录中的旧 0.2.0；修复后行为测试拒绝旧目录及不匹配哈希路径 |
| HTTPS/代理/重定向/截断/坏摘要/归档/原子安装 | 使用自有 loopback HTTPS、CONNECT 与临时文件完成行为验证，不访问用户业务库 |
| 真实 0.2.0 六归档生成与 `npm pack --json` | 六份 ZIP 重新核对官方 digest；最终主包 9,356 字节、11 文件，无二进制和平台包依赖 |
| `publish_npm.py --package <生成目录/kenvoy>` | 退出 0，显示 `kenvoy@0.2.0 计划发布（只读，未上传）` |
| Windows 匿名隔离全局安装最终 tarball | 精确 file: 来源获准后 postinstall 执行；真实 `kenvoy --version` 输出 0.2.0，二进制摘要与清单一致 |
| Linux 匿名隔离全局安装同一最终 tarball | postinstall 执行；真实 Linux 二进制输出 `kenvoy 0.2.0`，摘要与清单一致 |
| Windows 初版候选本地安装后的 `create`、`check` | 创建临时应用并检查通过：1 资源、1 页面、0 动作；最终候选仅补齐 npm 12 提示说明 |
| `cargo build -p kenvoy --offline --locked` 与真实 `--version` | 编译成功，输出 `kenvoy 0.4.3` |
| 0.4.3 真实 `create`、`check`、`build` | 均退出 0；创建最低 CLI 为 0.4.3、项目格式为 0.4.0，检查为 1 资源/1 页面/0 动作 |
| `actionlint .github/workflows/channels.yml` | 退出 0，无诊断 |
| 用户终端 `npm publish <冻结tarball> --access public --tag latest --ignore-scripts --provenance=false --registry=https://registry.npmjs.org` | 认证成功、上传 202、退出 0；首轮查询仍为服务端处理中，不能算正式版本已公开 |
| `npm view kenvoy@0.2.0 ... --json --prefer-online` 与 `npm view kenvoy dist-tags ...` | 精确版本公开、latest=0.2.0；SHA-1/SHA-512 与冻结包相等，11 文件，无平台分包依赖 |
| Windows npm 12 注册表全新全局安装 `kenvoy@0.2.0 --allow-scripts=kenvoy` | 隔离 prefix/cache/空凭据配置，退出 0；实际输出 0.2.0，Windows 二进制摘要一致 |
| WSL/Linux npm 11 注册表全新全局安装 `kenvoy@0.2.0` | 隔离 prefix/cache/空凭据配置，退出 0；实际 Linux 程序输出 0.2.0、摘要一致；npm 11 有未来脚本审批提示，WSL 原有 localhost 代理警告不影响此次下载 |
| `cargo run --package xtask --offline --locked -- check` | 通过：四成员及依赖方向、437 个自有源码文件、最大 449 行；105 份 Markdown、1,138 条本地链接 |
| 安装器及测试源码 `wc -l` | 15 个本轮 Python/CJS 文件最大 401 行，未超过 500 行上限 |

最终 tarball SHA-1：`b0a6bc12ae958a8aa865b433cee20a6f7189889a`。

最终 tarball integrity：`sha512-b6XFYP0EfTCn4Vk0iFpXzH0jkk4zLeLVTN+kI4Rax2UjwbYLj//juCGQ0T5Vwi4dtaUucUufRNrleLBBuwlk5g==`。

Windows 最终安装二进制为 20,388,864 字节，SHA-256 `72e9ad7e78074401db54405944f50187daf0ba254c2071d5b0df098ca418bb5b`；Linux 为 17,658,736 字节，SHA-256 `b7d4e411f21652274232a3fd485cbbf8e01c9aeb560b713b173f62872b0a096f`。安装目录使用该二进制摘要，而不是 ZIP 摘要。

## 已知限制与后续验证建议

- 正式 npm 0.2.0 已核验；旧分包仍待用户终端完成撤销认证，OIDC 尚未配置，不能以此次手动发布替代。
- Windows x64、Linux x64 已真实安装执行；macOS、ARM64 仅验证平台选择、清单和归档，不宣称完成对应硬件运行；Node 18 下限未另做真实运行。
- 本轮不运行全量 Rust、前端和浏览器套件，不触碰真实业务数据库；不证明后续 main 自动发布已经执行。
- 首次 OIDC 发布后再核验 provenance、正式包摘要及六个平台安装；Homebrew/Scoop 首次远端更新和安装也须独立验收。
- 发布 0.4.3 前确认所需本地标签已按授权同步，main 与版本标签完整树一致；不以挪动旧标签解决新提交差异。
