//! SWC 解析树与卫生上下文适配为自有词法绑定模型。

mod collector;
mod types;
mod visit;

use std::collections::BTreeMap;

use swc_common::{GLOBALS, Globals, Mark, SourceFile};
use swc_ecma_ast as ast;
use swc_ecma_transforms_base::resolver;
use swc_ecma_visit::{VisitMutWith, VisitWith};

use crate::domain::{
    semantic::{
        ReferenceId, ReferenceKind, ScopeIR, ScopeId, ScopeKind, SemanticIR, SymbolId,
        SymbolNamespace, resolve_reference,
    },
    source::SourceIndex,
};

pub(crate) fn analyze(
    module: &ast::Module,
    source: &str,
    file: &SourceFile,
    prefix: usize,
) -> SemanticIR {
    GLOBALS.set(&Globals::default(), || {
        let mut resolved = module.clone();
        resolved.visit_mut_with(&mut resolver(Mark::new(), Mark::new(), true));
        let mut collector = Collector {
            output: SemanticIR::default(),
            index: SourceIndex::new(source),
            base: file.start_pos.0,
            prefix,
            current: ScopeId(0),
            symbols: BTreeMap::new(),
            lexical: BTreeMap::new(),
            pending: vec![],
            namespace: SymbolNamespace::Value,
            reference_kind: ReferenceKind::Read,
        };
        collector.output.scopes.push(ScopeIR {
            id: ScopeId(0),
            parent: None,
            kind: ScopeKind::Module,
            span: collector.index.span(0, source.len()),
        });
        resolved.visit_with(&mut collector);
        for (id, key) in &collector.pending {
            let reference = &collector.output.references[id.0];
            let symbol =
                resolve_reference(&collector.output, &collector.lexical, reference, || {
                    collector.symbols.get(key).copied()
                });
            collector.output.references[id.0].symbol = symbol;
        }
        collector.output
    })
}

type BindingKey = (String, u32, SymbolNamespace);

struct Collector<'a> {
    output: SemanticIR,
    index: SourceIndex<'a>,
    base: u32,
    prefix: usize,
    current: ScopeId,
    symbols: BTreeMap<BindingKey, SymbolId>,
    lexical: BTreeMap<(ScopeId, String, SymbolNamespace), SymbolId>,
    pending: Vec<(ReferenceId, BindingKey)>,
    namespace: SymbolNamespace,
    reference_kind: ReferenceKind,
}
