//! 阶段 00 的有限技术探针，测试类型不是正式公共 IR 或 Engine API。

#[path = "p01_language/cases.rs"]
mod cases;
#[path = "p01_language/convert.rs"]
mod convert;

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use swc_common::{FileName, SourceFile, SourceMap, Span, Spanned, sync::Lrc};
use swc_ecma_ast::*;
use swc_ecma_parser::{EsSyntax, Syntax, TsSyntax, parse_file_as_module};
use thiserror::Error;

type TestResult = Result<(), Box<dyn Error>>;

#[derive(Debug, Error)]
enum ProbeError {
    #[error("{path}:{line}:{column}：语法错误：{message}")]
    Syntax {
        path: String,
        line: usize,
        column: usize,
        message: String,
    },
    #[error("{importer} 无法解析模块 {specifier}")]
    Module { importer: String, specifier: String },
    #[error("读取 {path} 失败：{source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Location {
    start: usize,
    end: usize,
    line: usize,
    column: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ExpressionRef {
    source: String,
    kind: String,
    symbol: Option<String>,
    location: Location,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
enum Value {
    String(String),
    Number(f64),
    Boolean(bool),
    Null,
    Expression(ExpressionRef),
    Node(Box<Node>),
    Empty(Location),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
enum Attribute {
    Named { name: String, value: Value },
    Spread(ExpressionRef),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
enum Node {
    Element {
        name: String,
        attributes: Vec<Attribute>,
        children: Vec<Node>,
        location: Location,
    },
    Fragment(Vec<Node>, Location),
    Text(String, Location),
    Expression(ExpressionRef),
    Empty(Location),
    Spread(ExpressionRef),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Import {
    source: String,
    bindings: Vec<String>,
    type_only: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ProbeModule {
    format_version: u32,
    path: String,
    imports: Vec<Import>,
    exports: Vec<String>,
    root: Option<Node>,
}

struct Parsed {
    source_map: Lrc<SourceMap>,
    file: Lrc<SourceFile>,
    module: Module,
}
fn tsx() -> Syntax {
    Syntax::Typescript(TsSyntax {
        tsx: true,
        ..Default::default()
    })
}

fn analyze_fixture(
    root: &Path,
    entry: &Path,
) -> Result<BTreeMap<PathBuf, ProbeModule>, ProbeError> {
    let mut modules = BTreeMap::new();
    let mut pending = vec![entry.to_path_buf()];
    let mut visited = BTreeSet::new();
    while let Some(path) = pending.pop() {
        if !visited.insert(path.clone()) {
            continue;
        }
        let source = fs::read_to_string(&path).map_err(|source| ProbeError::Read {
            path: path.clone(),
            source,
        })?;
        let parsed = Parsed::parse(&path.to_string_lossy(), &source, tsx())?;
        let module = parsed.convert(&path.to_string_lossy());
        for import in &module.imports {
            let base = path.parent().unwrap_or(root).join(&import.source);
            let target = [base.with_extension("ts"), base.with_extension("tsx")]
                .into_iter()
                .find(|candidate| candidate.is_file());
            let Some(target) = target else {
                return Err(ProbeError::Module {
                    importer: module.path.clone(),
                    specifier: import.source.clone(),
                });
            };
            pending.push(target);
        }
        modules.insert(path, module);
    }
    Ok(modules)
}
