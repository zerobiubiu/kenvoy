use std::collections::BTreeMap;

use super::{
    ReferenceIR, ReferenceKind, ScopeId, ScopeKind, SemanticIR, SymbolId, SymbolKind,
    SymbolNamespace,
};

pub(crate) fn resolve_reference(
    semantic: &SemanticIR,
    lexical: &BTreeMap<(ScopeId, String, SymbolNamespace), SymbolId>,
    reference: &ReferenceIR,
    direct: impl FnOnce() -> Option<SymbolId>,
) -> Option<SymbolId> {
    if reference.namespace == SymbolNamespace::Type || reference.kind == ReferenceKind::Component {
        // TypeScript 的 infer、签名和泛型拥有独立的类型作用域；不将
        // 变换器为了擦除类型建立的卫生上下文视为完整类型绑定规则。
        let mut scope = Some(reference.scope);
        let mut found = None;
        while let Some(id) = scope {
            if let Some(symbol) = lexical.get(&(id, reference.name.clone(), reference.namespace)) {
                found = Some(*symbol);
                break;
            }
            scope = semantic.scopes[id.0].parent;
        }
        found
    } else {
        let direct = direct();
        if direct.is_none() && reference.kind == ReferenceKind::TypeQuery {
            let mut scope = Some(reference.scope);
            let mut found = None;
            while let Some(id) = scope {
                if let Some(symbol) =
                    lexical.get(&(id, reference.name.clone(), SymbolNamespace::Type))
                {
                    if semantic.symbols[symbol.0].kind == SymbolKind::Import {
                        found = Some(*symbol);
                    }
                    break;
                }
                scope = semantic.scopes[id.0].parent;
            }
            found
        } else {
            direct
        }
    }
}

pub(crate) fn function_scope(semantic: &SemanticIR, current: ScopeId) -> ScopeId {
    let mut scope = current;
    while let Some(parent) = semantic.scopes[scope.0].parent {
        if matches!(
            semantic.scopes[scope.0].kind,
            ScopeKind::Function | ScopeKind::Module | ScopeKind::Namespace
        ) {
            break;
        }
        scope = parent;
    }
    scope
}
