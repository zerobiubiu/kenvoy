# crates.io 鉴权修复与 CLI 0.4.4 发布恢复

状态：已执行（本地修复与验证；远端发行结果由渠道进度页持续记录）。影响范围：源码包发布鉴权、Kenvoy CLI 版本与发布操作。前置：[0033](0033-single-npm-package-download.md)。不改写此前记录，不移动既有标签。

## 背景与失败事实

用户已在 npm 为 `kenvoy` 创建 GitHub Trusted Publisher，绑定 `zerobiubiu/kenvoy`、`channels.yml`，启用 npm publish、npm stage publish 和 npm dist-tag；截图状态仍为 Pending validation，须在 2026-11-06 21:07 UTC 前首次成功发布。

随后用户明确授权同步分支、推送所需标签，并进行 crates.io、GitHub Release、npm、Homebrew、Scoop 的正式发行。实查远端 main 是本地 dev 的祖先，主分支无独有提交；所有待推标签的 YAML 都只允许 main push，不因 tag push 补发历史版本。先逐个推送 v0.2.1、v0.3.0、v0.4.0、v0.4.1、v0.4.2、v0.4.3，再同步 dev，将 main 快进至 `d2f76e7` 并推送；没有改写历史。

[0.4.3 正式运行 41917200515](https://github.com/zerobiubiu/kenvoy/actions/runs/41917200515) 的版本与标签门禁通过，Ubuntu 上源包编译和 dry-run 通过，但实际上传前失败：

```text
error: no token found, please run `cargo login`
or use environment variable CARGO_REGISTRY_TOKEN
```

发布器已成功读取仓库 Secret `CARGO_TOKEN`，问题发生在子进程环境：原实现使用 `CARGO_REGISTRIES_CRATES_IO_TOKEN`，而默认 crates.io 使用 `CARGO_REGISTRY_TOKEN`。因此没有进行源码上传；随后精确 registry 查询也确认 `kenvoy@0.4.3` 不存在。六平台归档、npm OIDC 与辅助仓库发布尚未执行，不能将它归为 npm 配置失败。Node 20 action 的弃用提示不是此次失败原因。

0.4.3 已有远端标签，首次源包发布又要求 main 与标签完整树一致。用户选择修复后改发 0.4.4，而不是重指标签、放宽门禁或借本地凭据绕过流水线。

## 实施

- `publish_crates.py` 只在实际 Cargo 上传子进程注入 `CARGO_REGISTRY_TOKEN`；编译、前端和 dry-run 继续剥离发布凭据，GitHub Token 不发送给 crates.io。
- 保留 main、最新提交、精确版本、远端标签、完整源码树、官方摘要及上传后可见性检查；不增加自动重试写请求。
- CLI manifest 与锁文件递增至 0.4.4；KEngine 保持 0.2.0，项目格式保持 0.4.0。create 的最低 CLI 随创建版本变为 0.4.4，不修改旧项目配置。
- 0.4.3 保留为未发行的历史标签，其继续有效的功能由 0.4.4 交付；npm 仍只发布 `kenvoy` 一个包，旧渠道不恢复。

### 真实 Cargo 鉴权回归

新增测试不检查源码文本或只断言传参，而是调用发布器、真实 Cargo 和本机 registry，观察实际上传请求的鉴权及包名/版本。

测试使用独占 CARGO_HOME、本机 Git 索引、127.0.0.1 HTTP 服务和无效的测试 Token。沿用 Cargo 自身 `replace_crates_io` 的测试入口，只在测试环境设置 `__CARGO_TEST_CRATES_IO_URL_DO_NOT_USE_THIS`，生产发布不使用此入口；本机代理拒绝转发，阻止意外访问公网。服务接收到请求后故意返回 403，不创建真实包，也不等待索引更新。

同一有效夹具运行原 `d2f76e7` 发布器时，Cargo 报缺 Token，服务未收到上传；运行修复后发布器时，服务收到携带测试鉴权、包名 `kenvoy`、版本 `0.0.0` 的实际上传请求。只有测试地址被替换，Cargo 仍自行解析默认仓库凭据。

依据：[Cargo registry authentication](https://doc.rust-lang.org/cargo/reference/registry-authentication.html)、[Cargo publish](https://doc.rust-lang.org/cargo/commands/cargo-publish.html)、[Cargo 自测工具](https://github.com/rust-lang/cargo/blob/master/crates/cargo-test-support/src/lib.rs)。本仓库固定 Rust/Cargo 1.98；后续升级工具链时须同时核对测试专用入口，不能把它当作生产 API。

## 改动文件

| 文件 | 内容 |
|---|---|
| `.github/scripts/publish_crates.py` | 修正默认 crates.io 凭据变量 |
| `.github/scripts/tests/test_publish_crates_auth.py` | 真实 Cargo、隔离默认 registry 与实际鉴权请求回归 |
| `crates/kenvoy/Cargo.toml`、`Cargo.lock` | CLI 0.4.4，依赖版本不变 |
| 根 README、CLI README、配置契约、CLI 说明 | 当前源码版本与新建项目最低 CLI |
| `docs/operations/releasing.md`、`distribution-channels.md` | 凭据交付规则与本次版本恢复边界 |
| `docs/development/channel-rollout-status.md` | 已创建 OIDC、0.4.3 失败位置与 0.4.4 发行状态 |
| CHANGELOG、本文及文档/变更/验证索引 | 修复、授权、执行事实及替代关系 |

## 执行验证记录

环境：Windows 11 x64、Rust/Cargo 1.98、uv 管理的 Python 3.12；首次正式源码 dry-run 同时在 GitHub Ubuntu 24.04 runner 执行。

| 命令或场景 | 实际输出与结论 |
|---|---|
| 0.4.3 精确 Git 快照的 `npm ci --ignore-scripts`、`npm run build` | 安装 33 包，Vite 转换 129 模块，构建成功 |
| 0.4.3 仓库外快照 `cargo publish --dry-run --locked --package kenvoy` | 304 文件，使用已发布 KEngine 0.2.0 独立编译，明确 dry-run 未上传 |
| 0.4.3 正式 Actions | 源包编译通过；实际上传在凭据解析阶段失败，后续发行跳过 |
| 精确 registry 查询 `kenvoy/0.4.3` | 不存在；没有已上传源包需要撤销 |
| 同一真实鉴权夹具运行原发布器 | 预期失败：本机服务未收到上传，Cargo 报缺少默认仓库 Token |
| `python -B -m unittest discover -s .github/scripts/tests -p test_publish_crates_auth.py -v` | 修复后 1 项通过；本机服务收到真实带鉴权上传请求，随后主动拒绝落库 |
| `cargo build -p kenvoy --offline --locked`、真实 `--version` | 构建成功，输出 `kenvoy 0.4.4` |
| 0.4.4 临时应用的 `create`、`check`、`build` | 均退出 0；1 资源、1 页面、0 动作，最低 CLI=0.4.4，项目格式=0.4.0 |
| `uv run --no-project --python 3.12 python -B -m unittest discover -s .github/scripts/tests -v` | 11 项全部通过，包含真实 Cargo 鉴权与既有发布策略回归 |
| `cargo run --package xtask --offline --locked -- check` | 架构、规模、文档通过：437 个自有源码文件，最大 449 行；106 份 Markdown、1,149 条本地链接 |

最初本机夹具使用 `--index`，被 Cargo 要求显式 `--token` 拒绝；普通 index 环境覆盖也不能模拟默认 crates.io。两者都不是修复成功证据。最终回归改为上游测试专用替身入口，并重新运行原实现与修复实现，验证了真实的失败前/通过后差异。

## 验证边界与后续

本地鉴权回归仅证明发布器正确交付凭据，不证明 GitHub Secret 中真实 Token 的有效性和权限。0.4.4 仍须真实 Actions 验证源包上传、六归档、npm OIDC 与 Homebrew/Scoop。后续结果与运行链接统一更新[渠道进度](../development/channel-rollout-status.md)，不把本记录中的本地通过改写成远端发行成功。

不读取用户本机 Cargo 凭据，不改 npm 权限，不上传测试包，不移动旧标签。前一运行的代码和日志保持原状，不重写 0033 的历史事实。
