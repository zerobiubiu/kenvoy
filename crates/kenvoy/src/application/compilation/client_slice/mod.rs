//! 从声明模块提取浏览器所需的词法依赖，不把应用默认导出作为浏览器入口。

mod collector;

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::PathBuf,
};

use kengine::{ModuleBindingTarget, ModuleIR, ProgramIR, SourceSpan, SymbolId, SymbolNamespace};

use crate::{
    application::execution::JsDiagnostic,
    domain::{
        model::{ApplicationIR, ModuleReference, ModuleReferenceKind},
        source::SourceLocation,
    },
};

use super::static_eval::FunctionTemplateParser;

pub trait ClientCodeTransformer: FunctionTemplateParser {
    fn transpile(&self, source: &str, name: &str, jsx: bool) -> Result<String, JsDiagnostic>;
}

#[derive(Debug, Clone)]
pub enum ClientImportTarget {
    File(PathBuf),
    External(String),
}

#[derive(Debug, Clone)]
pub struct ClientSlice {
    pub code: String,
    pub export_name: String,
    pub imports: BTreeMap<String, ClientImportTarget>,
    pub source: SourceLocation,
}

pub fn client_slice(
    program: &ProgramIR,
    app: &ApplicationIR,
    reference: &ModuleReference,
    transformer: &impl ClientCodeTransformer,
) -> Result<ClientSlice, JsDiagnostic> {
    if !app
        .module_references
        .iter()
        .any(|candidate| candidate == reference)
    {
        return Err(failure(reference, "客户端引用不在已验证的应用注册表内"));
    }
    slice(program, reference, transformer)
}

/// DSL 边界检查与真正构建使用相同的依赖收集，不以整个声明模块的依赖替代表达式闭包。
pub(crate) fn slice(
    program: &ProgramIR,
    reference: &ModuleReference,
    transformer: &impl ClientCodeTransformer,
) -> Result<ClientSlice, JsDiagnostic> {
    let module = program
        .modules
        .iter()
        .find(|module| module.path == reference.module)
        .ok_or_else(|| failure(reference, "找不到表达式所属的源码模块"))?;
    let mut collector = Collector {
        program,
        module,
        template_parser: transformer,
        template_bindings: BTreeMap::new(),
        reference,
        seen: BTreeSet::new(),
        pending: VecDeque::new(),
        pieces: BTreeMap::new(),
        local_pieces: BTreeMap::new(),
        imports: BTreeMap::new(),
        import_code: BTreeMap::new(),
    };
    let result = if reference.kind == ModuleReferenceKind::ClientExpression {
        let span = reference
            .expression
            .as_ref()
            .ok_or_else(|| failure(reference, "表达式缺少源码范围"))?;
        collector.capture(span)?;
        format!("() => ({})", raw(module, span, reference)?)
    } else {
        let targets = program
            .module_bindings
            .exports
            .iter()
            .find(|export| {
                export.module == reference.module
                    && export.name == reference.export
                    && export.namespace == SymbolNamespace::Value
            })
            .map(|export| export.targets.as_slice())
            .ok_or_else(|| failure(reference, "无法定位客户端导出"))?;
        match targets {
            [
                ModuleBindingTarget::Symbol {
                    module: owner,
                    symbol,
                },
            ] if owner == &module.path => {
                collector.pending.push_back(*symbol);
                module
                    .semantic
                    .symbol(*symbol)
                    .ok_or_else(|| failure(reference, "客户端导出的符号无效"))?
                    .name
                    .clone()
            }
            [
                ModuleBindingTarget::Expression {
                    module: owner,
                    expression,
                },
            ] if owner == &module.path => {
                let expression = module
                    .expression(*expression)
                    .ok_or_else(|| failure(reference, "客户端默认导出表达式缺失"))?;
                collector.capture(&expression.span)?;
                raw(module, &expression.span, reference)?.to_owned()
            }
            [
                ModuleBindingTarget::Declaration {
                    module: owner,
                    span,
                },
            ] if owner == &module.path => {
                collector.capture(span)?;
                raw(module, span, reference)?
                    .trim_start_matches("export ")
                    .trim_start_matches("default ")
                    .to_owned()
            }
            _ => {
                return Err(failure(
                    reference,
                    "声明文件转导或存在歧义的组件请从独立 UI 模块直接导入",
                ));
            }
        }
    };
    while let Some(symbol) = collector.pending.pop_front() {
        collector.include(symbol)?;
    }
    let mut source = String::new();
    for code in collector.import_code.values() {
        source.push_str(code);
        source.push('\n');
    }
    for code in collector.pieces.values() {
        source.push_str(code);
        source.push('\n');
    }
    let mut export_name = "__kenvoy_value".to_owned();
    while module
        .semantic
        .symbols
        .iter()
        .any(|symbol| symbol.name == export_name)
    {
        export_name.push('_');
    }
    if collector.local_pieces.is_empty() {
        source.push_str(&format!("export const {export_name} = ({result});\n"));
    } else {
        source.push_str(&format!("export const {export_name} = (() => {{\n"));
        for code in collector.local_pieces.values() {
            source.push_str(code);
            source.push('\n');
        }
        source.push_str(&format!("return ({result});\n}})();\n"));
    }
    let jsx = matches!(
        module
            .path
            .extension()
            .and_then(|extension| extension.to_str()),
        Some("tsx" | "jsx")
    );
    let code = transformer.transpile(
        &source,
        if jsx {
            "kenvoy-client-slice.tsx"
        } else {
            "kenvoy-client-slice.ts"
        },
        jsx,
    )?;
    Ok(ClientSlice {
        code,
        export_name,
        imports: collector.imports,
        source: reference.source.clone(),
    })
}

struct Collector<'a> {
    program: &'a ProgramIR,
    module: &'a ModuleIR,
    template_parser: &'a dyn FunctionTemplateParser,
    template_bindings: BTreeMap<(usize, usize), BTreeSet<SymbolId>>,
    reference: &'a ModuleReference,
    seen: BTreeSet<SymbolId>,
    pending: VecDeque<SymbolId>,
    pieces: BTreeMap<usize, String>,
    local_pieces: BTreeMap<usize, String>,
    imports: BTreeMap<String, ClientImportTarget>,
    import_code: BTreeMap<usize, String>,
}

fn raw<'a>(
    module: &'a ModuleIR,
    span: &SourceSpan,
    reference: &ModuleReference,
) -> Result<&'a str, JsDiagnostic> {
    module
        .raw(span)
        .ok_or_else(|| failure(reference, "源码范围不是有效的 UTF-8 字节区间"))
}
fn failure(reference: &ModuleReference, message: impl Into<String>) -> JsDiagnostic {
    JsDiagnostic::new(
        "KV6101",
        message,
        Some(reference.source.clone()),
        "把复杂组件或函数移到独立 client/shared 模块，使用静态 ESM 导入",
    )
}

#[cfg(test)]
mod tests;
