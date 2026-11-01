use swc_common::Span;
use swc_ecma_ast as ast;
use swc_ecma_visit::VisitWith;

use crate::domain::{
    semantic::{
        ImportBindingIR, ReferenceIR, ReferenceId, ReferenceKind, ScopeIR, ScopeId, ScopeKind,
        SymbolIR, SymbolId, SymbolKind, SymbolNamespace, function_scope,
    },
    source::SourceSpan,
};

use super::Collector;

impl Collector<'_> {
    pub(super) fn span(&self, span: Span) -> SourceSpan {
        self.index.span(
            span.lo.0.saturating_sub(self.base) as usize + self.prefix,
            span.hi.0.saturating_sub(self.base) as usize + self.prefix,
        )
    }
    pub(super) fn scoped(&mut self, kind: ScopeKind, span: Span, f: impl FnOnce(&mut Self)) {
        let previous = self.current;
        let id = ScopeId(self.output.scopes.len());
        self.output.scopes.push(ScopeIR {
            id,
            parent: Some(previous),
            kind,
            span: self.span(span),
        });
        self.current = id;
        f(self);
        self.current = previous;
    }
    pub(super) fn context(
        &mut self,
        namespace: SymbolNamespace,
        kind: ReferenceKind,
        f: impl FnOnce(&mut Self),
    ) {
        let previous = (self.namespace, self.reference_kind);
        self.namespace = namespace;
        self.reference_kind = kind;
        f(self);
        (self.namespace, self.reference_kind) = previous;
    }
    pub(super) fn declare(
        &mut self,
        ident: &ast::Ident,
        namespace: SymbolNamespace,
        kind: SymbolKind,
        import: Option<ImportBindingIR>,
    ) {
        let context = if namespace == SymbolNamespace::Type {
            self.current.0 as u32
        } else {
            ident.ctxt.as_u32()
        };
        let key = (ident.sym.to_string(), context, namespace);
        let span = self.span(ident.span);
        if let Some(id) = self.symbols.get(&key) {
            if !self.output.symbols[id.0].declarations.contains(&span) {
                self.output.symbols[id.0].declarations.push(span);
            }
            return;
        }
        let scope = if kind == SymbolKind::Var {
            function_scope(&self.output, self.current)
        } else {
            self.current
        };
        let id = SymbolId(self.output.symbols.len());
        self.output.symbols.push(SymbolIR {
            id,
            name: ident.sym.to_string(),
            namespace,
            kind,
            scope,
            declarations: vec![span],
            import,
        });
        self.lexical
            .insert((scope, ident.sym.to_string(), namespace), id);
        self.symbols.insert(key, id);
    }
    pub(super) fn both(&mut self, ident: &ast::Ident, kind: SymbolKind) {
        self.declare(ident, SymbolNamespace::Value, kind, None);
        self.declare(ident, SymbolNamespace::Type, kind, None);
    }
    pub(super) fn reference_ident(&mut self, ident: &ast::Ident) {
        let id = ReferenceId(self.output.references.len());
        self.output.references.push(ReferenceIR {
            id,
            name: ident.sym.to_string(),
            namespace: self.namespace,
            kind: self.reference_kind,
            scope: self.current,
            span: self.span(ident.span),
            symbol: None,
        });
        self.pending.push((
            id,
            (ident.sym.to_string(), ident.ctxt.as_u32(), self.namespace),
        ));
    }
    pub(super) fn bind(&mut self, pat: &ast::Pat, kind: SymbolKind) {
        match pat {
            ast::Pat::Ident(binding) => {
                self.declare(&binding.id, SymbolNamespace::Value, kind, None);
                binding.type_ann.visit_with(self);
            }
            ast::Pat::Array(array) => {
                for pat in array.elems.iter().flatten() {
                    self.bind(pat, kind);
                }
                array.type_ann.visit_with(self);
            }
            ast::Pat::Rest(rest) => {
                self.bind(&rest.arg, kind);
                rest.type_ann.visit_with(self);
            }
            ast::Pat::Object(object) => {
                for prop in &object.props {
                    match prop {
                        ast::ObjectPatProp::KeyValue(pair) => {
                            pair.key.visit_with(self);
                            self.bind(&pair.value, kind);
                        }
                        ast::ObjectPatProp::Assign(assign) => {
                            self.declare(&assign.key.id, SymbolNamespace::Value, kind, None);
                            assign.value.visit_with(self);
                        }
                        ast::ObjectPatProp::Rest(rest) => self.bind(&rest.arg, kind),
                    }
                }
                object.type_ann.visit_with(self);
            }
            ast::Pat::Assign(assign) => {
                self.bind(&assign.left, kind);
                assign.right.visit_with(self);
            }
            ast::Pat::Expr(expr) => expr.visit_with(self),
            ast::Pat::Invalid(_) => {}
        }
    }
    pub(super) fn function_contents(&mut self, f: &ast::Function) {
        f.type_params.visit_with(self);
        f.this_param.visit_with(self);
        f.params.visit_with(self);
        f.return_type.visit_with(self);
        f.body.visit_with(self);
    }
    pub(super) fn member_root(&mut self, member: &ast::JSXMemberExpr) {
        match &member.obj {
            ast::JSXObject::Ident(ident) => self.reference_ident(ident),
            ast::JSXObject::JSXMemberExpr(member) => self.member_root(member),
        }
    }
}
