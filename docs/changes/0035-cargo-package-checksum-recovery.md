# 发布后校验与草稿 Release 恢复

状态：已执行（本地修复与回归；0.4.4 源包、六目标 Release、npm OIDC、Homebrew 与 Scoop 已发布）。影响范围：发布工程，不改变已发布程序。前置：[0034](0034-cargo-auth-and-044-release.md)。CLI 保持 0.4.4，KEngine 保持 0.2.0；不重传源包、不移动标签、不重新递增产品版本。

## 实际失败边界

[0.4.4 正式运行 41922611322](https://github.com/zerobiubiu/kenvoy/actions/runs/41922611322) 的 Cargo 鉴权已成功，日志先后出现：

```text
Uploaded kenvoy v0.4.4 to registry `crates-io`
Published kenvoy v0.4.4 at registry `crates-io`
```

随后发布器读取 `<source>/target/package/kenvoy-0.4.4.crate` 计算摘要时遇到文件不存在，导致整个 job 被标记失败。因此这是“源包已经提交，后处理失败”，不是上传失败或回滚；不能再次上传相同版本。

独立查询和下载验证确认：

- crates.io 已存在 `kenvoy@0.4.4`。
- 归档长度 533,657 字节，SHA-256 为 `0cec578df29a7d07bcfc5b3c5dfee695827e096a02e4dfd3a4b4390d53a710a0`，实际下载字节与官方 metadata 一致。
- 包名/版本正确，包含 LICENSE、前端 index.html 与 JavaScript 资产。

本机既有 Cargo 1.98 dry-run 的实际输出位于 `target/package/tmp-crate/`；发布器不能把 `cargo publish` 的内部暂存布局当作 `cargo package` 的正式输出，也不能假设 `target` 永远位于源码目录内。

## 修复方案

1. 通过 `cargo metadata --no-deps --format-version 1 --locked` 获取真实 `target_directory`，遵循 Cargo 的目标目录配置。
2. 用 `cargo package --locked` 创建并编译验证正式源包，保留构建脚本修改检查，不使用 `--no-verify` 或 `--allow-dirty`。
3. 在上传前读取正式归档并固化 SHA-256，避免依赖上传后的内部临时文件。
4. 保留最新 main 与精确版本检查，用正确的 `CARGO_REGISTRY_TOKEN` 执行实际上传。
5. 将 registry 返回的 checksum 与上传前摘要比较，不重新读取或猜测 publish 的内部归档位置。

采用 Cargo 文档保证的输出，而不是另加 `tmp-crate` 特例或遇到文件缺失时忽略校验。依据：[cargo package](https://doc.rust-lang.org/cargo/commands/cargo-package.html)、[cargo metadata](https://doc.rust-lang.org/cargo/commands/cargo-metadata.html)。

本次仅修复发布器与测试/文档，不改动已公开的 0.4.4 源码包。后续二进制继续从该真实 registry 源包构建，不拿新的 checkout 冒充。新源包的首次发布仍要求完整标签树一致；已发布版本的补齐流程不放宽身份、版本或 checksum 检查。

## 草稿 Release 检索修复

源码恢复后六个平台均构建成功，但 Release 阶段把 Windows x64 ZIP 上传到草稿后，错误报告“上传后未出现在 Release 中”。实查 Release ID `434529963` 的草稿和 ZIP 均存在；按标签接口返回 404，认证后的列表接口返回该草稿。

GitHub 的按标签接口只返回已发布版本。`GitHub.release` 现在先查已发布版本，404 后分页查找可访问的 Release 列表，并按 tag_name 定位草稿；列表不可读时失败，不将其当作不存在。依据：[GitHub Release API](https://docs.github.com/en/rest/releases/releases?apiVersion=2022-11-28)。新增用例覆盖新草稿上传全流程及第二页已有草稿恢复，检查不重复创建、不覆盖已有 ZIP、六组侧车正确；修复前两项均失败，修复后通过。

## 恢复安排

确认源包存在且下载校验通过后，执行 `gh run rerun 41922611322 --failed --repo zerobiubiu/kenvoy` 恢复失败阶段；既有发布器跳过已发布源包，六目标构建完成。草稿误判后下载该次运行的原始六份产物，重新核对 ZIP/SHA256、PE/ELF/Mach-O 架构、README/LICENSE，并实际执行 Windows/Linux x64 的 `--version`，均为 0.4.4。

在核对 main、tag 与构建来源同为 `6a2d863` 后，仅向原草稿补齐缺失的 11 个文件；原 Windows x64 ZIP 的 asset ID `711177694` 和摘要保持不变。按 Release ID 核验全部 12 个文件的官方 digest 后公开 Release，再恢复最后失败的校验 job。主发布最终成功，并自动触发[三渠道运行 41929186955](https://github.com/zerobiubiu/kenvoy/actions/runs/41929186955)，四个 job 均成功。资产补齐属于授权恢复，不声称原工作流第一次执行就全绿。

修复在 dev 完成；远端运行期间没有提前推送新的 main，避免触发旧运行被替代的门禁。产品版本与发布源不变；后续只同步工程修补和现状文档。最终渠道状态见[渠道进度](../development/channel-rollout-status.md)。

## 改动范围

| 文件 | 变化 |
|---|---|
| `.github/scripts/publish_crates.py` | 使用 Cargo metadata、正式 package 输出、上传前摘要与上传后 registry 比较 |
| `.github/scripts/tests/test_publish_crates_auth.py` | 真实本机发布成功、索引更新、上传字节摘要及外部 target 目录回归 |
| `.github/scripts/release_common.py`、`.github/scripts/tests/test_release_assets.py` | 正确定位已发布版本与分页草稿，验证完整发布和已有 ZIP 恢复 |
| 发布操作、工程流程、版本日志、渠道进度及各级索引 | 正确描述包已发布后的失败、恢复与验证入口 |

## 执行验证记录

环境：Windows 11 x64，Rust/Cargo 1.98、Python 3.12；实际远端上传为 GitHub Ubuntu 24.04 runner。

| 验证 | 实际结果 |
|---|---|
| 正式运行上传日志与精确 registry 查询 | `kenvoy@0.4.4` 已发布；不是“整个发布已回滚” |
| 官方 `.crate` 实际下载、SHA-256 与内容核对 | 533,657 字节，摘要一致；LICENSE、已构建前端及包名/版本存在且正确 |
| 修复前，真实 Cargo 向本机 registry 发布并更新索引 | 上传成功后复现 FileNotFoundError，原实现读取错误位置 |
| 修复后，同一完整本机发布场景 | 带正确鉴权完成上传，索引可见，上传前/服务端实收归档 SHA-256 一致 |
| 显式外部 `CARGO_TARGET_DIR` | 同样完成真实本机发布及摘要核对，不再依赖 `<source>/target` |
| `uv run --no-project --python 3.12 python -B -m unittest discover -s .github/scripts/tests -v` | 14 项全部通过；两项真实 Cargo 发布、两项草稿生命周期、十项原有发布策略 |
| 修复后的 `GitHub.release` 通过 GitHub CLI 传输进行实查 | 返回真实草稿 ID 434529963 与已上传 Windows x64 ZIP，不读取或打印登录令牌 |
| 六份 Actions 原始归档 | ZIP 与侧车一致，架构头部正确，README/LICENSE 完整；Windows/Linux x64 实际输出 `kenvoy 0.4.4` |
| Release 正式公开后的 12 项 API metadata | 每个 ZIP/侧车的大小与 SHA256 均与已验产物一致；旧 Windows asset ID 不变 |
| npm 精确版本与发布者查询 | `kenvoy@0.4.4`、latest=0.4.4；GitHub Actions、trustedPublisher=github，存在 SLSA provenance |
| Windows/Linux 匿名隔离 npm 全局安装 | 两者成功并输出 0.4.4；仅批准 kenvoy 脚本，二进制摘要与正式 ZIP 一致；六目标下载清单全部匹配 |
| Windows npm 安装后的 create/check/build | 均退出 0，检查为 1 资源、1 页面、0 动作，真实生成构建产物 |
| Homebrew/Scoop 自动发布与实际仓库内容 | formula/manifest 均为 0.4.4，四个 Homebrew 目标和两个 Scoop 目标的 URL/SHA256 全部一致 |
| 旧 `kenvoy-win32-x64` 最终查询 | npm 返回 E404，旧平台包已不存在 |
| `cargo run --package xtask --offline --locked -- check` | 架构、规模及文档通过：437 个自有源码文件，最大 449 行；107 份 Markdown、1,159 条本地链接 |

本机测试只使用独占 Cargo Home、Git 索引、127.0.0.1 服务和无效测试 Token；网络出口仍限定本机，不读取真实凭据、不向 crates.io 上传测试包。测试从先前的“鉴权后主动拒绝”扩展到“本机接收归档并更新索引”，以覆盖真正的成功上传后处理。

Homebrew 提交为 `151bdf98b60c2c57f423564af31a7b1bcfb7c734`，Scoop 提交为 `fc92ef75c9d987cb84866fb618654069226e07bb`。npm tarball SHA-1 为 `93867f83875561cdbee63ee63fe8aa3440b285c5`，integrity 为 `sha512-jGjFEtoocY9uceS8vJeIUSCd3sRYK4SdA3/IZAS38xYaiAoFRuFSRPTrJdcrzHBaPXkpy3EhC9HDYHRosl4xeA==`。

验证边界：未运行 Homebrew/Scoop 的原生安装命令，未额外在 macOS/ARM64 上做本机安装验收，未重跑与发布修复无关的全量 Rust/前端/浏览器套件。源码包已经公开时，不为修复 CI 后处理而改写该版本或新增产品版本。
