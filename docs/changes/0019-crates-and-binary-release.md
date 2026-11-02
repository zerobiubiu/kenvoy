# 0019：0.2.0 源包与跨平台二进制发布

状态：已执行；治理：不可变变更记录；日期：2026-11-02。结果：两个源包与六目标预编译二进制均已发布，cargo-binstall 基本安装核验通过。影响范围：kengine/kenvoy 发布包、构建元数据、分发工程与文档；前置：[0018](0018-runtime-root-entry.md)；现行流程：[发布操作](../operations/releasing.md)。

## 背景与授权

用户明确要求将两个产品更新为 0.2，推送并发布新版，再提供 cargo-binstall 二进制分发。版本归一为 SemVer `0.2.0`；现有线上 `0.1.0` 不覆盖。后续用户要求三大平台各架构使用交叉编译、Zig 替代 GCC、不做测试套件，并确认本机 Zig 由 Scoop 安装。

范围为 Windows MSVC、Linux musl、macOS 的 x86_64/aarch64 六目标。Windows MSVC 仍使用微软 SDK/工具链而不是 GCC；Linux/macOS 使用 cargo-zigbuild/Zig，macOS 仍需要 Apple SDK。没有加入移动平台或 WebAssembly。

发布、推送与版本更新已获授权；不读取/输出 Token，不修改用户项目数据。GitHub 和 crates.io 分别认证，不能复用 GitHub OAuth Token 冒充 registry Token。

## 方案与实施文件

- 两个产品 Cargo.toml 更新 `0.2.0`、发布描述、README/关键词/分类和明确 include 白名单；Kenvoy 的 KEngine 依赖改为 path+version，内部工程成员保持不发布。
- 两个 Crate 添加包级 README/LICENSE。Kenvoy 包含预构建前端发行资产及框架源码/构建脚本，不带 node_modules、用户数据库、凭据或运行缓存。
- `crates/kenvoy/build.rs` 明确区分 Workspace 和独立包布局；后者只读取包内锁文件，前者继续跟踪 KEngine 源码及 Workspace 内容。构建时生成实际锁文件 SHA256 和真实 KEngine 版本。
- `crates/kenvoy/src/bootstrap/build.rs` 使用构建期元数据，删除越包 Cargo.lock include 和将 Kenvoy 版本误记为 KEngine 版本的实现。
- `xtask/src/commands/distribution/` 增加二进制架构检查、ZIP/SHA256 归档；`.github/workflows/distribution.yml` 与 `.github/distribution-targets.json` 定义六目标及按平台补打包，区分 checkout `source` 和实际发布源包 `registry`，不偷偷上传 crate 或创建 Release。
- 发布入口及版本概要见本次新增的 operations/releasing.md、changes/CHANGELOG.md；索引与包级说明同步更新。

## 执行验证记录

### 环境与已确认事实

- 初始工作树干净，HEAD `1de3f7e`，main 相对本地 origin/main 领先六个提交；GitHub 公开仓库为 `zerobiubiu/kenvoy`，已登录对应账号。
- crates.io 的两个包均已有由 zerobiubiu 发布的 `0.1.0`，当前 owner 相同。
- 本机 Scoop shim `C:/Users/root/scoop/shims/zig.exe version` 实际返回 `0.17.0`。旧会话 PATH 未直接定位到，但没有据此重装 Zig 或修改全局 PATH。
- `cargo update --workspace --offline` 成功，产品锁文件版本为 0.2.0；新增工程 zip 2.4.2（default-features=false、deflate）及复用 sha2 0.11.1，产品 build-dependency 复用 toml 0.9.8。
- `npm run build --prefix crates/kenvoy/frontend` 退出 0，Vite 115 modules，产出通用 CSS/JS/index；没有运行 npm test。
- `cargo check -p kengine -p kenvoy --offline --locked` 退出 0，实际检查两个 0.2.0 产品；没有运行 cargo test 或浏览器测试。
- `cargo check --workspace --offline --locked` 与格式检查已通过；首次分发工具编译发现 sha2 0.11 输出数组不支持直接 LowerHex，已改为单次分配的十六进制编码。
- 工程源码目录使用 `xtask/src/commands/distribution/`，避免 `dist/` 被既有构建产物忽略规则排除；CLI 子命令仍为 dist。
- `cargo fetch --locked` 仅补齐缺失的 arbitrary/derive_arbitrary 元数据缓存，不执行测试；随后 `cargo run --package xtask --offline --locked -- check` 退出 0，343 个自有代码文件、279 个 Rust 文件、最大 449 行、87 个 Markdown、873 个本地链接通过。
- `cargo build -p kenvoy --release --offline --locked` 退出 0；`target/release/kenvoy.exe --version` 为 `kenvoy 0.2.0`。
- 本机 `cargo xtask dist` 已核对 x86_64 Windows PE32+ 与限时版本，归档根仅有 kenvoy.exe/README.md/LICENSE；源码候选 ZIP SHA256 为 `e4cc42ff8151e1fd096f9b72cf9bc93a80b0bc56e02674a139f07e3a26656f8b`，不冒充最终 registry 分发产物。
- 验收工具 cargo-binstall 1.26.2 来自官方不可变 Release，Windows ZIP 与 GitHub 资产 SHA256 `c377a4977ffdbdcbf58dd263055b2f1d56ba873c69fbd6bfc60bef98256183a3` 一致，仅解压至本机忽略的独占工具目录。
- 用户随后要求不做详细检验、以打包为准，因此取消额外 source 六目标预检门禁，保留 Cargo 自带源包编译和最终二进制打包。
- 首次多包 dry-run 未上传；它拦下了未锚定 README/LICENSE 导致 node_modules 同名文件入选，以及工作区中未跟踪前端产物的 dirty 限制。白名单已改为包根锚定；正式打包改用精确提交的仓库外独占源码快照生成前端，不提交产物、不跳过编译检查。
- 初始 source workflow `41462154639` 未生成发布资产：Windows 日志为 Python cp1252 UnicodeEncodeError，Linux/macOS 为 Zig host 包装器缺少 `--` 参数分隔；已分别设置 workflow UTF-8 与正确参数转发。该失败不记作任何目标构建通过。

### 已执行的源包发布

- 提交 `1d73b8d` 与白名单/包装器修正 `df2c17a` 已推送 main；`v0.2.0` tag 指向 `df2c17a` 并已推送。
- 正式输入从 `df2c17a` 的 git archive 复制到仓库外独占快照，快照内执行 npm ci（忽略脚本）与构建。多包 dry-run 正常打包、解压并编译通过，未使用 allow-dirty/no-verify。
- 第一次正式上传因未登录阻断，随后因 registry 403 authentication failed 阻断；用户在本机重新登录后重试，未读取或记录 Token。
- `cargo publish -p kengine` 与 `cargo publish -p kenvoy` 分别成功，输出 `Published ... v0.2.0 at registry crates-io`；Kenvoy 验证时已从 registry 下载真实 KEngine 0.2.0。

| 包 | registry 版本 | 文件数 / 压缩大小 | 官方 SHA256 |
|---|---|---|---|
| kengine | 0.2.0 | 58 / 66,341 字节 | `2d11cfbef3dd3ef967370222a6bbe5c7657872ae328cf77455d2290555e43ad7` |
| kenvoy | 0.2.0 | 242 / 432,065 字节 | `e740c8404d759d127eacf1131b2e240a5abcb1600adb7cfb039abfee4393d243` |

官方版本已通过 crates.io API 返回确认。registry workflow `41469164866` 已成功打包 Linux/macOS 四目标；Windows 两目标因 windows-2025 未找到所指定 Visual Studio 2022 而失败。已按官方镜像清单切换 windows-2022，并增加平台筛选只补打包 Windows，不重跑四份成功归档。该构建失败不记作 Windows 通过。

Windows 补打包 workflow `41472155432` 已正确找到 VS 2022，但 Git Bash 将自身的 `link.exe` 放到 MSVC 前面，导致 host 工具链接失败。Windows 两处 Cargo 编译改为 PowerShell，保留开发者环境 PATH；Linux/macOS 仍用 Bash/Zig。四份已下载的 Unix ZIP 已通过独立字节级 SHA256 对比，与各自侧车一致。

### 已执行的二进制发布与安装

- registry workflow [41469164866](https://github.com/zerobiubiu/kenvoy/actions/runs/41469164866) 提供 Linux/macOS 四份最终归档；修正后的 [41473758114](https://github.com/zerobiubiu/kenvoy/actions/runs/41473758114) 两个 Windows 目标均成功。六份均来自实际 crates.io 0.2.0 源包，而非本机 Workspace 候选。
- 六份 ZIP 的独立 SHA256 均与侧车一致；GitHub 草稿上传后，12 项资产全部为 uploaded，大小及官方 asset digest 与本地逐项一致。
- `gh release edit v0.2.0 --draft=false --latest` 成功；随后查询为 isDraft=false、isPrerelease=false，发布时间 `2026-11-02T15:53:24Z`。公开 [GitHub Release](https://github.com/zerobiubiu/kenvoy/releases/tag/v0.2.0) 包含六个 ZIP 与六个 SHA256 侧车。tag 仍指向 `df2c17a`，没有移动。

| 最终目标 | ZIP SHA256 |
|---|---|
| x86_64-pc-windows-msvc | `93b3cb3c3d40848d5fd6756c2acf8a086cb9cc90bd9c099e82883ba4e6971f03` |
| aarch64-pc-windows-msvc | `2a05cba66ad0f9fc31e9a0ec819a97aa11f919f83438bae590e3fe7ae27128e9` |
| x86_64-unknown-linux-musl | `9e956d150e6f71bd3f10f8c3539c838bcaec73cdce64ece97cf9f550b41d95a6` |
| aarch64-unknown-linux-musl | `13fbc5f82f23903f31c1927388311cc58f5e05bb2f90f4b3b3406e3c0d0d2160` |
| x86_64-apple-darwin | `f6d5aca8693755afd496c44e1bc1474c48537a4587ffb6c06b656eedc51ce2f9` |
| aarch64-apple-darwin | `110122b12d92a2fe21c637ac66c5a453d4e63cf21f9657dd469c39ccb4565493` |

Windows x64 的实际安装使用官方 cargo-binstall 1.26.2，仅允许 crate-meta-data 策略，禁用源码编译和 QuickInstall 回退，不覆盖现有安装：

```text
D:/kenvoy/artifacts/release-tools-6eafc306/cargo-binstall.exe kenvoy@0.2.0 --no-confirm --strategies crate-meta-data --disable-telemetry --dns system --install-path D:/kenvoy/artifacts/binstall-020-099d85f1
D:/kenvoy/artifacts/binstall-020-099d85f1/kenvoy.exe --version
```

关键实际输出为 `has been downloaded from github.com`、`Done in 5.1584815s`、`kenvoy 0.2.0`，组合命令退出 0。首次刻意关闭 GitHub 认证发现的安装请求收到 API 403，已停止；改用默认的既有 GitHub CLI 认证发现后成功，没有读取或输出 Token。跨盘安装出现原子重命名/ReplaceFileW 警告，安装器随后以临时文件复制并重命名完成；这不是源码构建回退。

文档收尾执行 `cargo run --package xtask --offline --locked -- docs`，退出 0；实际输出 `markdown_files: 87`、`local_links: 880`、`status: 通过`。本项仅检查文档编码、链接及索引，没有执行测试套件。

### 发布验收边界

本次按用户要求不新增/运行测试套件，只执行发布必需编译、独立包验证、架构/归档/SHA256 和基本安装版本核验。交叉编译不能声称为对应架构的完整运行测试；安装场景仅为 Windows x64，不外推到其他平台。发布、CI、Release 和安装结论均来自实际输出，不由工作流存在推定成功。

## 后续验证建议

- 后续功能迭代再按[工程流程](../development/engineering-workflow.md)选择必要回归；本次不补跑用户明确排除的功能测试。
- 若承诺最低系统版本或各平台完整运行支持，单独补齐[平台矩阵 V05](../development/implementation-backlog.md#v05目标平台浏览器与最低版本运行矩阵)，不把六目标编译等同运行验收。
- 签名、完整供应链审计、历史产物保留和恢复工具仍按 [E06](../development/implementation-backlog.md#e06发行产物保留与恢复工具) 的剩余范围处理，未因本次发布自动获得实施授权。
