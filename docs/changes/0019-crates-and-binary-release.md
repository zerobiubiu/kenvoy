# 0019：0.2.0 源包与跨平台二进制发布

状态：待执行；治理：发布实施草稿；日期：2026-11-02。影响范围：kengine/kenvoy 发布包、构建元数据、分发工程与文档；前置：[0018](0018-runtime-root-entry.md)；现行流程：[发布操作](../operations/releasing.md)。

## 背景与授权

用户明确要求将两个产品更新为 0.2，推送并发布新版，再提供 cargo-binstall 二进制分发。版本归一为 SemVer `0.2.0`；现有线上 `0.1.0` 不覆盖。后续用户要求三大平台各架构使用交叉编译、Zig 替代 GCC、不做测试套件，并确认本机 Zig 由 Scoop 安装。

范围为 Windows MSVC、Linux musl、macOS 的 x86_64/aarch64 六目标。Windows MSVC 仍使用微软 SDK/工具链而不是 GCC；Linux/macOS 使用 cargo-zigbuild/Zig，macOS 仍需要 Apple SDK。没有加入移动平台或 WebAssembly。

发布、推送与版本更新已获授权；不读取/输出 Token，不修改用户项目数据。GitHub 和 crates.io 分别认证，不能复用 GitHub OAuth Token 冒充 registry Token。

## 方案与实施文件

- 两个产品 Cargo.toml 更新 `0.2.0`、发布描述、README/关键词/分类和明确 include 白名单；Kenvoy 的 KEngine 依赖改为 path+version，内部工程成员保持不发布。
- 两个 Crate 添加包级 README/LICENSE。Kenvoy 包含预构建前端发行资产及框架源码/构建脚本，不带 node_modules、用户数据库、凭据或运行缓存。
- `crates/kenvoy/build.rs` 明确区分 Workspace 和独立包布局；后者只读取包内锁文件，前者继续跟踪 KEngine 源码及 Workspace 内容。构建时生成实际锁文件 SHA256 和真实 KEngine 版本。
- `crates/kenvoy/src/bootstrap/build.rs` 使用构建期元数据，删除越包 Cargo.lock include 和将 Kenvoy 版本误记为 KEngine 版本的实现。
- xtask 增加二进制架构检查、ZIP/SHA256 归档；手动触发的 CI 区分上传前 `source` 构建与来自实际 registry 发布包的最终 `registry` 构建，不偷偷上传 crate 或创建 Release。
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

### 发布验收边界

本次按用户要求不新增/运行测试套件。后续只执行发布必需编译、独立包验证、架构/归档/SHA256 和基本安装版本核验；不能把交叉编译声称为对应架构的运行测试。最终 registry、CI、Release 和安装结果以本记录实际执行输出为准，不由工作流存在推定已经发布。
