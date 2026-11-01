use swc_common::Spanned;
use swc_ecma_ast as ast;
use swc_ecma_visit::{Visit, VisitWith};

use crate::domain::semantic::{
    ImportBindingIR, ReferenceKind, ScopeKind, SymbolKind, SymbolNamespace,
};

use super::Collector;

impl Visit for Collector<'_> {
    fn visit_ident(&mut self, ident: &ast::Ident) {
        self.reference_ident(ident);
    }
    fn visit_binding_ident(&mut self, binding: &ast::BindingIdent) {
        self.reference_ident(&binding.id);
        binding.type_ann.visit_with(self);
    }
    fn visit_var_decl(&mut self, decl: &ast::VarDecl) {
        let kind = match decl.kind {
            ast::VarDeclKind::Const => SymbolKind::Const,
            ast::VarDeclKind::Let => SymbolKind::Let,
            ast::VarDeclKind::Var => SymbolKind::Var,
        };
        for item in &decl.decls {
            self.bind(&item.name, kind);
            item.init.visit_with(self);
        }
    }
    fn visit_using_decl(&mut self, decl: &ast::UsingDecl) {
        for item in &decl.decls {
            self.bind(&item.name, SymbolKind::Using);
            item.init.visit_with(self);
        }
    }
    fn visit_fn_decl(&mut self, decl: &ast::FnDecl) {
        self.declare(
            &decl.ident,
            SymbolNamespace::Value,
            SymbolKind::Function,
            None,
        );
        decl.function.visit_with(self);
    }
    fn visit_fn_expr(&mut self, expr: &ast::FnExpr) {
        expr.function.decorators.visit_with(self);
        self.scoped(ScopeKind::FunctionParameters, expr.function.span, |this| {
            if let Some(ident) = &expr.ident {
                this.declare(ident, SymbolNamespace::Value, SymbolKind::Function, None);
            }
            this.function_contents(&expr.function);
        });
    }
    fn visit_function(&mut self, function: &ast::Function) {
        function.decorators.visit_with(self);
        self.scoped(ScopeKind::FunctionParameters, function.span, |this| {
            this.function_contents(function)
        });
    }
    fn visit_function_body(&mut self, body: &ast::FunctionBody) {
        // 参数默认表达式在函数体 var/函数声明环境外求值。
        self.scoped(ScopeKind::Function, body.span, |this| {
            body.visit_children_with(this)
        });
    }
    fn visit_param(&mut self, param: &ast::Param) {
        param.decorators.visit_with(self);
        self.bind(&param.pat, SymbolKind::Parameter);
    }
    fn visit_arrow_expr(&mut self, expr: &ast::ArrowExpr) {
        self.scoped(ScopeKind::FunctionParameters, expr.span, |this| {
            expr.type_params.visit_with(this);
            for pat in &expr.params {
                this.bind(pat, SymbolKind::Parameter);
            }
            expr.return_type.visit_with(this);
            expr.body.visit_with(this);
        });
    }
    fn visit_block_stmt(&mut self, block: &ast::BlockStmt) {
        self.scoped(ScopeKind::Block, block.span, |this| {
            block.visit_children_with(this)
        });
    }
    fn visit_catch_clause(&mut self, clause: &ast::CatchClause) {
        self.scoped(ScopeKind::Catch, clause.span, |this| {
            if let Some(pat) = &clause.param {
                this.bind(pat, SymbolKind::CatchParameter);
            }
            clause.body.visit_with(this);
        });
    }
    fn visit_for_stmt(&mut self, stmt: &ast::ForStmt) {
        self.scoped(ScopeKind::Loop, stmt.span, |this| {
            stmt.visit_children_with(this)
        });
    }
    fn visit_for_in_stmt(&mut self, stmt: &ast::ForInStmt) {
        self.scoped(ScopeKind::Loop, stmt.span, |this| {
            stmt.visit_children_with(this)
        });
    }
    fn visit_for_of_stmt(&mut self, stmt: &ast::ForOfStmt) {
        self.scoped(ScopeKind::Loop, stmt.span, |this| {
            stmt.visit_children_with(this)
        });
    }
    fn visit_switch_stmt(&mut self, stmt: &ast::SwitchStmt) {
        stmt.discriminant.visit_with(self);
        self.scoped(ScopeKind::Block, stmt.span, |this| {
            stmt.cases.visit_with(this)
        });
    }
    fn visit_class_decl(&mut self, decl: &ast::ClassDecl) {
        self.both(&decl.ident, SymbolKind::Class);
        decl.class.visit_with(self);
    }
    fn visit_class_expr(&mut self, expr: &ast::ClassExpr) {
        self.scoped(ScopeKind::Class, expr.class.span, |this| {
            if let Some(ident) = &expr.ident {
                this.both(ident, SymbolKind::Class);
            }
            expr.class.visit_children_with(this);
        });
    }
    fn visit_class(&mut self, class: &ast::Class) {
        class.decorators.visit_with(self);
        self.scoped(ScopeKind::Class, class.span, |this| {
            class.type_params.visit_with(this);
            class.super_class.visit_with(this);
            class.super_type_params.visit_with(this);
            this.context(SymbolNamespace::Type, ReferenceKind::Type, |this| {
                class.implements.visit_with(this)
            });
            class.body.visit_with(this);
        });
    }
    fn visit_constructor(&mut self, ctor: &ast::Constructor) {
        ctor.key.visit_with(self);
        self.scoped(ScopeKind::FunctionParameters, ctor.span, |this| {
            ctor.params.visit_with(this);
            ctor.body.visit_with(this);
        });
    }
    fn visit_ts_param_prop(&mut self, prop: &ast::TsParamProp) {
        self.collect_ts_param_prop(prop);
    }
    fn visit_getter_prop(&mut self, prop: &ast::GetterProp) {
        prop.key.visit_with(self);
        prop.function.visit_with(self);
    }
    fn visit_setter_prop(&mut self, prop: &ast::SetterProp) {
        prop.key.visit_with(self);
        prop.function.visit_with(self);
    }
    fn visit_static_block(&mut self, block: &ast::StaticBlock) {
        self.scoped(ScopeKind::Function, block.span, |this| {
            block.body.visit_with(this)
        });
    }
    fn visit_import_decl(&mut self, decl: &ast::ImportDecl) {
        for specifier in &decl.specifiers {
            let (local, imported, namespace, type_only) = match specifier {
                ast::ImportSpecifier::Default(item) => {
                    (&item.local, Some("default".into()), false, decl.type_only)
                }
                ast::ImportSpecifier::Namespace(item) => (&item.local, None, true, decl.type_only),
                ast::ImportSpecifier::Named(item) => (
                    &item.local,
                    match &item.imported {
                        Some(ast::ModuleExportName::Ident(ident)) => Some(ident.sym.to_string()),
                        Some(ast::ModuleExportName::Str(value)) => String::from_utf16(
                            &value.value.to_ill_formed_utf16().collect::<Vec<_>>(),
                        )
                        .ok(),
                        None => Some(item.local.sym.to_string()),
                    },
                    false,
                    decl.type_only || item.is_type_only,
                ),
            };
            let binding = ImportBindingIR {
                source_utf16: decl.src.value.to_ill_formed_utf16().collect(),
                imported,
                namespace,
                span: self.span(specifier.span()),
            };
            if !type_only {
                self.declare(
                    local,
                    SymbolNamespace::Value,
                    SymbolKind::Import,
                    Some(binding.clone()),
                );
            }
            self.declare(
                local,
                SymbolNamespace::Type,
                SymbolKind::Import,
                Some(binding),
            );
        }
    }
    fn visit_named_export(&mut self, export: &ast::NamedExport) {
        if export.src.is_some() {
            return;
        }
        for specifier in &export.specifiers {
            if let ast::ExportSpecifier::Named(named) = specifier
                && let ast::ModuleExportName::Ident(ident) = &named.orig
            {
                let type_only = export.type_only || named.is_type_only;
                if !type_only {
                    self.context(SymbolNamespace::Value, ReferenceKind::Export, |this| {
                        this.reference_ident(ident)
                    });
                }
                self.context(SymbolNamespace::Type, ReferenceKind::Export, |this| {
                    this.reference_ident(ident)
                });
            }
        }
    }
    fn visit_export_default_decl(&mut self, decl: &ast::ExportDefaultDecl) {
        match &decl.decl {
            ast::DefaultDecl::Fn(function) => {
                if let Some(ident) = &function.ident {
                    self.declare(ident, SymbolNamespace::Value, SymbolKind::Function, None);
                }
                function.function.visit_with(self);
            }
            ast::DefaultDecl::Class(class) => {
                if let Some(ident) = &class.ident {
                    self.both(ident, SymbolKind::Class);
                }
                class.class.visit_with(self);
            }
            ast::DefaultDecl::TsInterfaceDecl(interface) => interface.visit_with(self),
        }
    }
    fn visit_ts_type(&mut self, ty: &ast::TsType) {
        self.collect_ts_type(ty);
    }
    fn visit_ts_type_ann(&mut self, ann: &ast::TsTypeAnn) {
        self.collect_ts_type_ann(ann);
    }
    fn visit_ts_type_param(&mut self, param: &ast::TsTypeParam) {
        self.collect_ts_type_param(param);
    }
    fn visit_ts_type_alias_decl(&mut self, decl: &ast::TsTypeAliasDecl) {
        self.collect_ts_type_alias_decl(decl);
    }
    fn visit_ts_interface_decl(&mut self, decl: &ast::TsInterfaceDecl) {
        self.collect_ts_interface_decl(decl);
    }
    fn visit_ts_enum_decl(&mut self, decl: &ast::TsEnumDecl) {
        self.collect_ts_enum_decl(decl);
    }
    fn visit_ts_module_decl(&mut self, decl: &ast::TsModuleDecl) {
        self.collect_ts_module_decl(decl);
    }
    fn visit_ts_namespace_decl(&mut self, decl: &ast::TsNamespaceDecl) {
        self.collect_ts_namespace_decl(decl);
    }
    fn visit_ts_import_equals_decl(&mut self, decl: &ast::TsImportEqualsDecl) {
        self.collect_ts_import_equals_decl(decl);
    }
    fn visit_ts_qualified_name(&mut self, name: &ast::TsQualifiedName) {
        self.collect_ts_qualified_name(name);
    }
    fn visit_ts_type_query(&mut self, query: &ast::TsTypeQuery) {
        self.collect_ts_type_query(query);
    }
    fn visit_ts_fn_type(&mut self, ty: &ast::TsFnType) {
        self.collect_ts_fn_type(ty);
    }
    fn visit_ts_constructor_type(&mut self, ty: &ast::TsConstructorType) {
        self.collect_ts_constructor_type(ty);
    }
    fn visit_ts_fn_param(&mut self, param: &ast::TsFnParam) {
        self.collect_ts_fn_param(param);
    }
    fn visit_ts_mapped_type(&mut self, ty: &ast::TsMappedType) {
        self.collect_ts_mapped_type(ty);
    }
    fn visit_ts_conditional_type(&mut self, ty: &ast::TsConditionalType) {
        self.collect_ts_conditional_type(ty);
    }
    fn visit_ts_property_signature(&mut self, prop: &ast::TsPropertySignature) {
        self.collect_ts_property_signature(prop);
    }
    fn visit_ts_getter_signature(&mut self, prop: &ast::TsGetterSignature) {
        self.collect_ts_getter_signature(prop);
    }
    fn visit_ts_setter_signature(&mut self, prop: &ast::TsSetterSignature) {
        self.collect_ts_setter_signature(prop);
    }
    fn visit_ts_method_signature(&mut self, method: &ast::TsMethodSignature) {
        self.collect_ts_method_signature(method);
    }
    fn visit_ts_call_signature_decl(&mut self, sig: &ast::TsCallSignatureDecl) {
        self.collect_ts_call_signature_decl(sig);
    }
    fn visit_ts_construct_signature_decl(&mut self, sig: &ast::TsConstructSignatureDecl) {
        self.collect_ts_construct_signature_decl(sig);
    }
    fn visit_ts_index_signature(&mut self, sig: &ast::TsIndexSignature) {
        self.collect_ts_index_signature(sig);
    }
    fn visit_jsx_element_name(&mut self, name: &ast::JSXElementName) {
        self.context(
            SymbolNamespace::Value,
            ReferenceKind::Component,
            |this| match name {
                ast::JSXElementName::Ident(ident) => this.reference_ident(ident),
                ast::JSXElementName::JSXMemberExpr(member) => this.member_root(member),
                _ => {}
            },
        );
    }
    fn visit_jsx_attr_name(&mut self, _: &ast::JSXAttrName) {}
    fn visit_labeled_stmt(&mut self, stmt: &ast::LabeledStmt) {
        stmt.body.visit_with(self);
    }
    fn visit_break_stmt(&mut self, _: &ast::BreakStmt) {}
    fn visit_continue_stmt(&mut self, _: &ast::ContinueStmt) {}
    fn visit_call_expr(&mut self, expr: &ast::CallExpr) {
        self.context(SymbolNamespace::Value, ReferenceKind::Call, |this| {
            expr.callee.visit_with(this)
        });
        expr.args.visit_with(self);
        expr.type_args.visit_with(self);
    }
    fn visit_new_expr(&mut self, expr: &ast::NewExpr) {
        self.context(SymbolNamespace::Value, ReferenceKind::Construct, |this| {
            expr.callee.visit_with(this)
        });
        expr.args.visit_with(self);
        expr.type_args.visit_with(self);
    }
    fn visit_assign_expr(&mut self, expr: &ast::AssignExpr) {
        let kind = if expr.op == ast::AssignOp::Assign {
            ReferenceKind::Write
        } else {
            ReferenceKind::ReadWrite
        };
        self.context(SymbolNamespace::Value, kind, |this| {
            expr.left.visit_with(this)
        });
        expr.right.visit_with(self);
    }
    fn visit_update_expr(&mut self, expr: &ast::UpdateExpr) {
        self.context(SymbolNamespace::Value, ReferenceKind::ReadWrite, |this| {
            expr.arg.visit_with(this)
        });
    }
    fn visit_member_expr(&mut self, expr: &ast::MemberExpr) {
        // obj.x = value 写入属性，不写入 obj 绑定；计算属性本身也只是读取。
        self.context(SymbolNamespace::Value, ReferenceKind::Read, |this| {
            expr.visit_children_with(this)
        });
    }
}
