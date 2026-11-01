# KEngine 公共 API

状态：参考（现状文档），包含阶段 02 的通用语义和增量接口；API 位于 [lib.rs](../../crates/kengine/src/lib.rs)，历史基础结果见 [阶段 01 验证记录](../verification/0003-kengine-foundation.md)。

## 调用和结果所有权

Engine::new 创建无状态引擎；每次解析/项目分析结果由调用方持有，Engine 不保存“最近结果”，不启动 HTTP、数据库或 JavaScript 执行器。Kenvoy 通过路径依赖调用这些自有类型，不接触 SWC AST。

| 接口 | 实际输入 | 实际输出 |
|---|---|---|
| parse_source | path: &Path、source: &str、ParseOptions | Result<ParseOutput, EngineError> |
| parse_file | path: &Path、ParseOptions | Result<ParseOutput, EngineError> |
| analyze_project | root: &Path、AnalyzeOptions | Result<ProgramIR, EngineError> |
| resolve_module | importer: &Path、specifier: &str、&ResolveOptions | Result<ResolvedModule, ResolveError> |
| AnalysisSession::analyze_project | &mut self、root: &Path、AnalyzeOptions | Result<AnalysisUpdate, EngineError> |

ParseOptions.language 为 Auto、JavaScript、TypeScript、Jsx 或 Tsx。Auto 按源码扩展名选择语法，未知扩展返回 KE0001；调用方可明确指定语言。当前按 ECMAScript module 解析，不声称提供完整 TS 类型检查或 Node/Bun 运行环境。

AnalyzeOptions.entry_points 必须明确指定；默认空列表返回 KE0002，而不是猜测 Kenvoy 的入口。parse 与 resolve 子选项控制语言/模块定位，KEngine 不读取 kenvoy.toml。

ParseOutput 包含可选 ModuleIR 和 diagnostics；ProgramIR 包含模块、静态模块图和汇总诊断。两者提供 get_diagnostics、has_errors；ProgramIR 提供 get_module_graph。语法恢复后可能仍有 ModuleIR，但 has_errors 为 true，调用方不得把部分 AST 激活为有效应用状态。

## 模块解析

ResolveOptions 包含 root、精确 externals 集合、map_js_to_ts；analyze_project 把 root 固定为实际分析根。按以下规则定位：精确文件 → 允许的 JS 到 TS 映射 → 追加源码扩展 → 目录 index。对 CustomerFields.diu 追加 .tsx，不错误替换 .diu。

存在目标 canonicalize 后检查根目录包含关系，路径和 symlink 不能越界；以规范路径去重，循环不会导致无限读取。项目图保留 import、type-only import、re-export 和 type-only re-export，可无损表示为字符串的说明符即使目标未知也保留边并输出诊断；无效 UTF-16 路径保留在原 ImportIR/ExportIR 和诊断中，不生成字符串图边，含错误的图可能不完整。cycles 为含环的强连通分量（包括自环），合法 ESM 环本身不自动成为错误。

bare package 默认拒绝，只有 externals 中的精确说明符可成为 External；它表示“留给调用方处理”，不代表 npm 依赖已安装或兼容。首版未实现 tsconfig paths、package exports/conditions、完整 npm Resolver 或动态 import 依赖求解；动态表达式仍留在模块 IR。

## 诊断和宿主错误

Diagnostic 有 code、category、severity、path、可选 span、message、suggestion 和 related locations。源码 span 为文件内半开字节区间及从 1 开始的 Unicode 标量行列；不把 UTF-16 列与标量列混用。无源码位置的配置/读取错误不伪造行列。

| 代码 | 含义 |
|---|---|
| KE0001 | 无法从文件名确定语言 |
| KE0002 | 未指定分析入口 |
| KE1001 | 致命或可恢复语法错误 |
| KE2001 | 模块目标未找到 |
| KE2002 | 路径超出分析根目录 |
| KE2003 | 不支持或不可无损表示的模块说明符 |
| KE2004 | 模块路径定位 I/O 失败 |
| KE2101 | 本地导入或导出名称缺失 |
| KE2102 | 导入或显式重导出指向歧义的星号导出 |
| KE2103 | 导入导出名称含无法无损表示的 UTF-16 code units |
| KE3001 | 项目遍历中的源文件读取失败 |

parse_file 直接读取失败使用带 path 和 source 的 EngineError::Io；analyze_project 在多个依赖上收集诊断，以便一次展示多个错误。resolve_module 返回独立 ResolveError，不把不同失败都压成字符串 API。

## 当前边界与消费测试

正式 IR 见 [generic-ir.md](generic-ir.md)。DeclarationIR.exported 仍只是顶层 inline/default 声明导出语法标记；最终导出使用 ProgramIR.module_bindings。ModuleIR.semantic 提供作用域、值/类型符号和引用。按标识符精确 span 调用 references_at，再通过 symbol 查询声明；None 表示环境全局或未解析名称，不等于语言错误。

ExpressionIR.shape 表示直接表达式关系，支持 JSX、类型/括号包装、数组、对象、运算符、条件、成员及模板。变量 declarators 分别保留模式和初始化表达式。未建模表达式为 Opaque 并保留源码；jsx_roots 仍包括函数体中出现的 JSX，不能作为求值结果。

## 增量会话

AnalysisSession 由调用者拥有；不使用全局状态。每轮读取可达文件内容，以规范路径、完整源码和 ParseOptions 比较缓存，只重解析变化模块。依赖定位、图/SCC 和模块链接重算，避免新文件、外部规则变化或扩展名优先级改变引起陈旧结果。

AnalysisUpdate.program 是当前候选；stats 分别列出 parsed、reused、evicted、affected。affected 包括新旧图的反向导入闭包，不代表全部重新解析。last_successful 只在候选没有 Error 时替换；它只证明语言/模块分析有效，不证明宿主业务、迁移或应用激活成功。clear 释放当前缓存、前次图和最后有效快照；不保存无限历史。

当前不是完整 TypeScript 类型检查器，不判断所有声明合并合法性、运行时 TDZ、动态 eval 或 namespace 成员值；不执行代码。外部模块导出不验证，external 星号导出显式保留不确定边界。尚未实现跨进程缓存、并行解析及完整 npm/tsconfig 解析。

可执行消费示例是 [Kenvoy 边界测试](../../crates/kenvoy/tests/engine_boundary.rs)；正式解析案例和模块分析测试位于 [KEngine 测试](../../crates/kengine/tests/engine_api.rs)。这些调用不含 Web 业务解释，不证明 DSL、CRUD 或页面 Runtime 已完成。
