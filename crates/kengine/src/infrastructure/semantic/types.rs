use swc_ecma_ast as ast;
use swc_ecma_visit::VisitWith;

use crate::domain::semantic::{
    ImportBindingIR, ReferenceKind, ScopeKind, SymbolKind, SymbolNamespace,
};

use super::Collector;

impl Collector<'_> {
    pub(super) fn collect_ts_param_prop(&mut self, prop: &ast::TsParamProp) {
        prop.decorators.visit_with(self);
        let pat = match &prop.param {
            ast::TsParamPropParam::Ident(pat) => ast::Pat::Ident(pat.clone()),
            ast::TsParamPropParam::Assign(pat) => ast::Pat::Assign(pat.clone()),
        };
        self.bind(&pat, SymbolKind::Parameter);
    }
    pub(super) fn collect_ts_type(&mut self, ty: &ast::TsType) {
        self.context(SymbolNamespace::Type, ReferenceKind::Type, |this| {
            ty.visit_children_with(this)
        });
    }
    pub(super) fn collect_ts_type_ann(&mut self, ann: &ast::TsTypeAnn) {
        self.context(SymbolNamespace::Type, ReferenceKind::Type, |this| {
            ann.visit_children_with(this)
        });
    }
    pub(super) fn collect_ts_type_param(&mut self, param: &ast::TsTypeParam) {
        self.declare(
            &param.name,
            SymbolNamespace::Type,
            SymbolKind::TypeParameter,
            None,
        );
        param.constraint.visit_with(self);
        param.default.visit_with(self);
    }
    pub(super) fn collect_ts_type_alias_decl(&mut self, decl: &ast::TsTypeAliasDecl) {
        self.declare(&decl.id, SymbolNamespace::Type, SymbolKind::TypeAlias, None);
        self.scoped(ScopeKind::Type, decl.span, |this| {
            decl.type_params.visit_with(this);
            decl.type_ann.visit_with(this);
        });
    }
    pub(super) fn collect_ts_interface_decl(&mut self, decl: &ast::TsInterfaceDecl) {
        self.declare(&decl.id, SymbolNamespace::Type, SymbolKind::Interface, None);
        self.scoped(ScopeKind::Type, decl.span, |this| {
            this.context(SymbolNamespace::Type, ReferenceKind::Type, |this| {
                decl.type_params.visit_with(this);
                decl.extends.visit_with(this);
                decl.body.visit_with(this);
            })
        });
    }
    pub(super) fn collect_ts_enum_decl(&mut self, decl: &ast::TsEnumDecl) {
        self.both(&decl.id, SymbolKind::Enum);
        self.scoped(ScopeKind::Namespace, decl.span, |this| {
            for member in &decl.members {
                if let ast::TsEnumMemberId::Ident(ident) = &member.id {
                    this.declare(ident, SymbolNamespace::Value, SymbolKind::Const, None);
                }
                member.init.visit_with(this);
            }
        });
    }
    pub(super) fn collect_ts_module_decl(&mut self, decl: &ast::TsModuleDecl) {
        if let ast::TsModuleName::Ident(ident) = &decl.id {
            self.both(ident, SymbolKind::Namespace);
        }
        self.scoped(ScopeKind::Namespace, decl.span, |this| {
            decl.body.visit_with(this)
        });
    }
    pub(super) fn collect_ts_namespace_decl(&mut self, decl: &ast::TsNamespaceDecl) {
        self.both(&decl.id, SymbolKind::Namespace);
        self.scoped(ScopeKind::Namespace, decl.span, |this| {
            decl.body.visit_with(this)
        });
    }
    pub(super) fn collect_ts_import_equals_decl(&mut self, decl: &ast::TsImportEqualsDecl) {
        let import = if let ast::TsModuleRef::TsExternalModuleRef(external) = &decl.module_ref {
            Some(ImportBindingIR {
                source_utf16: external.expr.value.to_ill_formed_utf16().collect(),
                imported: None,
                namespace: true,
                span: self.span(decl.span),
            })
        } else {
            None
        };
        if !decl.is_type_only {
            self.declare(
                &decl.id,
                SymbolNamespace::Value,
                SymbolKind::Import,
                import.clone(),
            );
        }
        self.declare(&decl.id, SymbolNamespace::Type, SymbolKind::Import, import);
        if let ast::TsModuleRef::TsEntityName(name) = &decl.module_ref {
            name.visit_with(self);
        }
    }
    pub(super) fn collect_ts_qualified_name(&mut self, name: &ast::TsQualifiedName) {
        name.left.visit_with(self);
    }
    pub(super) fn collect_ts_type_query(&mut self, query: &ast::TsTypeQuery) {
        self.context(SymbolNamespace::Value, ReferenceKind::TypeQuery, |this| {
            query.expr_name.visit_with(this)
        });
        query.type_args.visit_with(self);
    }
    pub(super) fn collect_ts_fn_type(&mut self, ty: &ast::TsFnType) {
        self.scoped(ScopeKind::Type, ty.span, |this| {
            ty.visit_children_with(this)
        });
    }
    pub(super) fn collect_ts_constructor_type(&mut self, ty: &ast::TsConstructorType) {
        self.scoped(ScopeKind::Type, ty.span, |this| {
            ty.visit_children_with(this)
        });
    }
    pub(super) fn collect_ts_fn_param(&mut self, param: &ast::TsFnParam) {
        let pat = match param {
            ast::TsFnParam::Ident(pat) => ast::Pat::Ident(pat.clone()),
            ast::TsFnParam::Array(pat) => ast::Pat::Array(pat.clone()),
            ast::TsFnParam::Rest(pat) => ast::Pat::Rest(pat.clone()),
            ast::TsFnParam::Object(pat) => ast::Pat::Object(pat.clone()),
        };
        self.bind(&pat, SymbolKind::Parameter);
    }
    pub(super) fn collect_ts_mapped_type(&mut self, ty: &ast::TsMappedType) {
        self.scoped(ScopeKind::Type, ty.span, |this| {
            ty.visit_children_with(this)
        });
    }
    pub(super) fn collect_ts_conditional_type(&mut self, ty: &ast::TsConditionalType) {
        ty.check_type.visit_with(self);
        self.scoped(ScopeKind::Type, ty.span, |this| {
            ty.extends_type.visit_with(this);
            ty.true_type.visit_with(this);
        });
        ty.false_type.visit_with(self);
    }
    pub(super) fn collect_ts_property_signature(&mut self, prop: &ast::TsPropertySignature) {
        if prop.computed {
            self.context(SymbolNamespace::Value, ReferenceKind::Read, |this| {
                prop.key.visit_with(this)
            });
        }
        prop.type_ann.visit_with(self);
    }
    pub(super) fn collect_ts_getter_signature(&mut self, prop: &ast::TsGetterSignature) {
        if prop.computed {
            self.context(SymbolNamespace::Value, ReferenceKind::Read, |this| {
                prop.key.visit_with(this)
            });
        }
        prop.type_ann.visit_with(self);
    }
    pub(super) fn collect_ts_setter_signature(&mut self, prop: &ast::TsSetterSignature) {
        if prop.computed {
            self.context(SymbolNamespace::Value, ReferenceKind::Read, |this| {
                prop.key.visit_with(this)
            });
        }
        self.scoped(ScopeKind::Type, prop.span, |this| {
            prop.param.visit_with(this)
        });
    }
    pub(super) fn collect_ts_method_signature(&mut self, method: &ast::TsMethodSignature) {
        if method.computed {
            self.context(SymbolNamespace::Value, ReferenceKind::Read, |this| {
                method.key.visit_with(this)
            });
        }
        self.scoped(ScopeKind::Type, method.span, |this| {
            method.type_params.visit_with(this);
            method.params.visit_with(this);
            method.type_ann.visit_with(this);
        });
    }
    pub(super) fn collect_ts_call_signature_decl(&mut self, sig: &ast::TsCallSignatureDecl) {
        self.scoped(ScopeKind::Type, sig.span, |this| {
            sig.visit_children_with(this)
        });
    }
    pub(super) fn collect_ts_construct_signature_decl(
        &mut self,
        sig: &ast::TsConstructSignatureDecl,
    ) {
        self.scoped(ScopeKind::Type, sig.span, |this| {
            sig.visit_children_with(this)
        });
    }
    pub(super) fn collect_ts_index_signature(&mut self, sig: &ast::TsIndexSignature) {
        self.scoped(ScopeKind::Type, sig.span, |this| {
            sig.visit_children_with(this)
        });
    }
}
