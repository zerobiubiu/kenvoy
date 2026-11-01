# P01：SWC 语言探针依赖与执行计划

状态：参考；本计划保留准备时的范围与验证事实。后续用户已授予本工程自主决策权，安装与探针执行结果见 [阶段 00 报告](0001-stage00-probes.md)；下文“待批准/未执行”均指准备当时，不代表当前状态。

## 背景与范围

用户已要求创建项目 AGENTS.md 并继续执行。项目规则已经按现有明确约束写入根目录；下一项依赖实际安装的是阶段 00 P01 语言探针。本计划先把包、版本、feature、位置和执行后果列清，再请求具体批准，不把继续工作推定为任意依赖安装授权。

本次只准备 SWC 与通用 IR 验证，不接入 Axum、SQLite、QuickJS、React 或 npm 构建链，不增加第三个 Rust Crate。P02–P05 仍待各自准备和验收，P01 通过不表示阶段 00 全部集成验收通过。

## 直接依赖批准清单

以下明确版本已核对 crates.io 官方发布元数据，仍未安装、求解或编译。最终依赖树以批准后生成的 Cargo.lock 为准。

| 包 | 精确版本 | feature 与用途 |
|---|---|---|
| swc_ecma_parser | =47.0.0 | 保留默认 typescript、stacker；解析 JS/TS/JSX/TSX，验证真实原生编译链 |
| swc_common | =27.0.0 | 默认配置；SourceMap、SourceFile、Span，不额外启插件/TTY/sourcemap |
| swc_ecma_ast | =30.0.0 | 默认配置；读取内部 AST，不启 AST serde 作为公共协议 |
| serde | =1.0.229 | derive；自有最小通用表示序列化 |
| serde_json | =1.0.154 | JSON 往返断言 |
| thiserror | =2.0.21 | 结构化错误与源错误保留 |

Parser 47.0.0 的正式依赖要求 swc_common ^27.0.0 和 swc_ecma_ast ^30.0.0，因此以上为配套候选，不是分别取 latest。swc_ecma_visit 为可选 verify 依赖，本次有限手工遍历不引入；完整符号遍历后续再评估。

当前推荐保留 stacker，而不是为绕过 C 编译工具链关闭默认保护。若本机原生编译失败，先记录具体失败；缺失 MSVC/SDK 等工具须单独批准，不能自动安装。仅 typescript、无 stacker 的受控输入备选需要明确说明栈深度限制，不能默认作为生产结论。

## 安装位置与副作用

- 项目位置：D:\kenvoy，批准后在根 workspace dependencies 登记精确版本，在 kengine 的探针 dev-dependencies 引用；保存 Cargo.lock。正式 API 的依赖归属在探针通过后按阶段实施调整，不暗中扩大范围。
- 下载位置：已核对的 Cargo Home 为 C:\Users\root\scoop\persist\rustup\.cargo，依赖进入其 registry 缓存；构建与测试输出位于 D:\kenvoy\target。不执行 cargo install，不安装系统级 CLI，不改用户工具链配置。
- 批准范围需包含以上直接版本和 Cargo 求解出的必要传递依赖、下载编译、过程宏和构建脚本执行。stacker 的原生构建可能调用已安装的 C 工具链；不批准自动获取缺失工具。
- 下载源为 crates.io 及现有 Cargo 有效 registry 配置；执行前核对源配置。若存在未说明的替代源、Git 依赖或缺失证书等问题，先报告，不静默替换配置。
- 不运行 npm install/npm ci/npx，不下载浏览器，不安装其他技术栈依赖。不写入业务数据库、不启动面向外部的 HTTP 服务，不提交或推送。

## 最小探针设计

优先在现有 crates/kengine/tests 下建立一个有限技术探针，使用测试本地定义的通用结构验证 SWC→自有表示，不先发布完整 Engine API，不实现 Kenvoy DSL 业务解释器。测试夹具包含普通 UI 标签和任意 JSX 标签，KEngine 不为名称赋予业务语义。

1. 解析 JS、TS、JSX、TSX；真实 TSX 覆盖 Fragment、嵌套、spread、数字/布尔/字符串属性、泛型函数、函数引用、import/export。
2. 检查 SWC 主解析结果与 recovered_errors，错误输入不得记为有效结果。
3. 转为测试用最小 Generic IR，保持属性顺序、动态 ExpressionRef、模块与源码位置；做 JSON 序列化往返。不公开 SWC AST，不把函数和动态表达式转成字符串值或 null。
4. 检查中文、emoji 和 CRLF 的字节/行列关联；只测试已明确的通用结构，不把模拟 Application IR 当成 Kenvoy 已实现。
5. 单独验证 KEngine 和整个 Workspace 编译/测试/Clippy/fmt，保存实际版本与 feature 树；不得以关闭检查或忽略失败通过探针。

P01 语言部分通过后，还需在后续受控范围验证 Kenvoy 消费公共 API 的链路；测试内通用表示不是正式公共 API 或完成的 Application IR。本探针不因可编译而冒充阶段 01/02 已完成。

计划中的执行入口为 cargo test -p kengine、cargo check --workspace、cargo test --workspace、cargo clippy --workspace --all-targets、cargo fmt --all -- --check。首次获批准备依赖需要联网，锁文件生成后使用 --locked；依赖已缓存时可加 --offline。以上为未来计划，当前没有运行带这些新依赖的命令。

## 批准后与失败处理

批准后先核对 Cargo 源配置并接入上述依赖，再获取锁文件、审查实际新增包与 feature、编写并执行最小探针。出现不兼容时保留错误证据，不自动升级直接版本或引入新工具；在同一批准版本范围内可修正测试集成方式。

全部通过后另建不可变验证记录，更新技术栈状态和风险；未通过则继续处理该探针，不开始 HTTP、数据库和前端业务。阶段 00 其他探针未通过前不宣称阶段 00 全部验收完成。

## 官方依据

- [Parser 47.0.0 发布与 feature](https://crates.io/api/v1/crates/swc_ecma_parser/47.0.0)
- [Parser 47.0.0 依赖](https://crates.io/api/v1/crates/swc_ecma_parser/47.0.0/dependencies)
- [parse_file_as_module 47.0.0](https://docs.rs/swc_ecma_parser/47.0.0/swc_ecma_parser/fn.parse_file_as_module.html)
- [TsSyntax 47.0.0](https://docs.rs/swc_ecma_parser/47.0.0/swc_ecma_parser/struct.TsSyntax.html)
- [serde 1.0.229](https://crates.io/api/v1/crates/serde/1.0.229)、[serde_json 1.0.154](https://crates.io/api/v1/crates/serde_json/1.0.154)、[thiserror 2.0.21](https://crates.io/api/v1/crates/thiserror/2.0.21)
- [Rust Windows MSVC 支持](https://doc.rust-lang.org/rustc/platform-support/windows-msvc.html)

SWC 元数据未声明可据以确认的 rust-version；本机 rustc -vV 输出 Rust 1.99.1、x86_64-pc-windows-msvc，不能替代 MSRV 1.98 测试。现有 vswhere 找到带 VC Tools 组件的 Visual Studio 安装目录 C:\Program Files\Microsoft Visual Studio\18\Community；这只是已安装组件元数据，具体原生编译能力仍待上述探针执行。

## 本轮准备验证记录

环境为 D:\kenvoy、Windows/PowerShell，Git 分支 main、HEAD b381cba；开始时仅存在上一轮未跟踪 docs/，未覆盖这些内容。

| 实际执行 | 退出码 | 关键输出与结论 |
|---|---|---|
| git status --short、git branch --show-current、git log -3 --oneline | 各 0 | main、b381cba；保留已有文档 |
| rustc -vV；读取 CARGO_HOME/RUSTUP_HOME 路径 | 0 | Rust 1.99.1、MSVC host；路径如上 |
| 已安装 vswhere 查询 VC Tools 组件 | 0 | 找到 VS 18 Community；不是编译通过证明 |
| Invoke-RestMethod 查询六项明确 crates.io 发布元数据 | 0 | 六项均存在且 Yanked=False；parser 默认 typescript/stacker |
| Node 只读检查 Cargo 配置位置 | 0 | 项目、D 盘上层及 Cargo Home 的 config/config.toml 均不存在；未读或输出凭据 |
| PowerShell here-string 输入 node --input-type=module 的 Markdown 检查 | 0 | 23 份 Markdown（含根 AGENTS）、62 个本地链接，严格 UTF-8/空白/链接 errors=[]；AGENTS 为 7293 字节 |
| git diff --exit-code -- Cargo.toml Cargo.lock .gitignore crates；git diff --check | 各 0 | 原有源码和配置无改动，跟踪文件无空白错误；未跟踪 Markdown 由 Node 另行检查 |

独立只读复核未发现 AGENTS 新增未授权长期规则，也未新增逐阶段人工批准要求。OpenAI 官方 [AGENTS.md 指引](https://learn.chatgpt.com/docs/agent-configuration/agents-md)用于核对根目录作用范围；规则内容来自用户已确认约定。

以上仅为准备工作。依赖安装批准问题已经发出；截至本记录尚未收到答复，因此未修改依赖、未下载编译包、未运行 SWC 探针，不将等待时间当作批准。后续实际探针结果另建执行记录。
