//! 通用词法绑定。内部使用 SWC resolver，公开结果不包含 SWC 类型或卫生上下文。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use swc_common::{GLOBALS, Globals, Mark, SourceFile, Span, Spanned};
use swc_ecma_ast as ast;
use swc_ecma_transforms_base::resolver;
use swc_ecma_visit::{Visit, VisitMutWith, VisitWith};

use crate::{SourceSpan, source::SourceIndex};

/// ID 仅对所属模块当前快照有效，不是持久化的跨修订标识。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ScopeId(pub usize);
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SymbolId(pub usize);
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ReferenceId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScopeKind {
    Module,
    FunctionParameters,
    Function,
    Block,
    Class,
    Catch,
    Loop,
    Namespace,
    Type,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum SymbolNamespace {
    Value,
    Type,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SymbolKind {
    Const,
    Let,
    Var,
    Using,
    Function,
    Class,
    Parameter,
    CatchParameter,
    Import,
    Interface,
    TypeAlias,
    TypeParameter,
    Enum,
    Namespace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReferenceKind {
    Read,
    Write,
    ReadWrite,
    Call,
    Construct,
    Component,
    Type,
    TypeQuery,
    Export,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopeIR {
    pub id: ScopeId,
    pub parent: Option<ScopeId>,
    pub kind: ScopeKind,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportBindingIR {
    pub source_utf16: Vec<u16>,
    /// default 对应默认导入；namespace 为 true 时不选择单个导出。
    pub imported: Option<String>,
    pub namespace: bool,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolIR {
    pub id: SymbolId,
    pub name: String,
    pub namespace: SymbolNamespace,
    pub kind: SymbolKind,
    pub scope: ScopeId,
    /// 合法重复 var、声明合并等共享词法绑定，保留每处声明位置。
    pub declarations: Vec<SourceSpan>,
    pub import: Option<ImportBindingIR>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferenceIR {
    pub id: ReferenceId,
    pub name: String,
    pub namespace: SymbolNamespace,
    pub kind: ReferenceKind,
    pub scope: ScopeId,
    pub span: SourceSpan,
    /// None 包含环境全局或无法静态绑定的名称，不自动断言为错误。
    pub symbol: Option<SymbolId>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticIR {
    pub scopes: Vec<ScopeIR>,
    pub symbols: Vec<SymbolIR>,
    pub references: Vec<ReferenceIR>,
}

impl SemanticIR {
    pub fn scope(&self, id: ScopeId) -> Option<&ScopeIR> {
        self.scopes.get(id.0)
    }
    pub fn symbol(&self, id: SymbolId) -> Option<&SymbolIR> {
        self.symbols.get(id.0)
    }
    pub fn reference(&self, id: ReferenceId) -> Option<&ReferenceIR> {
        self.references.get(id.0)
    }
    /// 按标识符的精确字节范围查询；同一位置可分别包含值与类型引用。
    pub fn references_at(&self, start: usize, end: usize) -> impl Iterator<Item = &ReferenceIR> {
        self.references
            .iter()
            .filter(move |r| r.span.start == start && r.span.end == end)
    }
}

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
            let symbol = if reference.namespace == SymbolNamespace::Type
                || reference.kind == ReferenceKind::Component
            {
                // TypeScript 的 infer、签名和泛型拥有独立的类型作用域；不将
                // 变换器为了擦除类型建立的卫生上下文视为完整类型绑定规则。
                let mut scope = Some(reference.scope);
                let mut found = None;
                while let Some(id) = scope {
                    if let Some(symbol) =
                        collector
                            .lexical
                            .get(&(id, reference.name.clone(), reference.namespace))
                    {
                        found = Some(*symbol);
                        break;
                    }
                    scope = collector.output.scopes[id.0].parent;
                }
                found
            } else {
                let direct = collector.symbols.get(key).copied();
                if direct.is_none() && reference.kind == ReferenceKind::TypeQuery {
                    let mut scope = Some(reference.scope);
                    let mut found = None;
                    while let Some(id) = scope {
                        if let Some(symbol) = collector.lexical.get(&(
                            id,
                            reference.name.clone(),
                            SymbolNamespace::Type,
                        )) {
                            if collector.output.symbols[symbol.0].kind == SymbolKind::Import {
                                found = Some(*symbol);
                            }
                            break;
                        }
                        scope = collector.output.scopes[id.0].parent;
                    }
                    found
                } else {
                    direct
                }
            };
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

impl Collector<'_> {
    fn span(&self, span: Span) -> SourceSpan {
        self.index.span(
            span.lo.0.saturating_sub(self.base) as usize + self.prefix,
            span.hi.0.saturating_sub(self.base) as usize + self.prefix,
        )
    }
    fn scoped(&mut self, kind: ScopeKind, span: Span, f: impl FnOnce(&mut Self)) {
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
    fn context(
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
    fn function_scope(&self) -> ScopeId {
        let mut scope = self.current;
        while let Some(parent) = self.output.scopes[scope.0].parent {
            if matches!(
                self.output.scopes[scope.0].kind,
                ScopeKind::Function | ScopeKind::Module | ScopeKind::Namespace
            ) {
                break;
            }
            scope = parent;
        }
        scope
    }
    fn declare(
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
            self.function_scope()
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
    fn both(&mut self, ident: &ast::Ident, kind: SymbolKind) {
        self.declare(ident, SymbolNamespace::Value, kind, None);
        self.declare(ident, SymbolNamespace::Type, kind, None);
    }
    fn reference_ident(&mut self, ident: &ast::Ident) {
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
    fn bind(&mut self, pat: &ast::Pat, kind: SymbolKind) {
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
    fn function_contents(&mut self, f: &ast::Function) {
        f.type_params.visit_with(self);
        f.this_param.visit_with(self);
        f.params.visit_with(self);
        f.return_type.visit_with(self);
        f.body.visit_with(self);
    }
    fn member_root(&mut self, member: &ast::JSXMemberExpr) {
        match &member.obj {
            ast::JSXObject::Ident(ident) => self.reference_ident(ident),
            ast::JSXObject::JSXMemberExpr(member) => self.member_root(member),
        }
    }
}

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
        prop.decorators.visit_with(self);
        let pat = match &prop.param {
            ast::TsParamPropParam::Ident(pat) => ast::Pat::Ident(pat.clone()),
            ast::TsParamPropParam::Assign(pat) => ast::Pat::Assign(pat.clone()),
        };
        self.bind(&pat, SymbolKind::Parameter);
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
        self.context(SymbolNamespace::Type, ReferenceKind::Type, |this| {
            ty.visit_children_with(this)
        });
    }
    fn visit_ts_type_ann(&mut self, ann: &ast::TsTypeAnn) {
        self.context(SymbolNamespace::Type, ReferenceKind::Type, |this| {
            ann.visit_children_with(this)
        });
    }
    fn visit_ts_type_param(&mut self, param: &ast::TsTypeParam) {
        self.declare(
            &param.name,
            SymbolNamespace::Type,
            SymbolKind::TypeParameter,
            None,
        );
        param.constraint.visit_with(self);
        param.default.visit_with(self);
    }
    fn visit_ts_type_alias_decl(&mut self, decl: &ast::TsTypeAliasDecl) {
        self.declare(&decl.id, SymbolNamespace::Type, SymbolKind::TypeAlias, None);
        self.scoped(ScopeKind::Type, decl.span, |this| {
            decl.type_params.visit_with(this);
            decl.type_ann.visit_with(this);
        });
    }
    fn visit_ts_interface_decl(&mut self, decl: &ast::TsInterfaceDecl) {
        self.declare(&decl.id, SymbolNamespace::Type, SymbolKind::Interface, None);
        self.scoped(ScopeKind::Type, decl.span, |this| {
            this.context(SymbolNamespace::Type, ReferenceKind::Type, |this| {
                decl.type_params.visit_with(this);
                decl.extends.visit_with(this);
                decl.body.visit_with(this);
            })
        });
    }
    fn visit_ts_enum_decl(&mut self, decl: &ast::TsEnumDecl) {
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
    fn visit_ts_module_decl(&mut self, decl: &ast::TsModuleDecl) {
        if let ast::TsModuleName::Ident(ident) = &decl.id {
            self.both(ident, SymbolKind::Namespace);
        }
        self.scoped(ScopeKind::Namespace, decl.span, |this| {
            decl.body.visit_with(this)
        });
    }
    fn visit_ts_namespace_decl(&mut self, decl: &ast::TsNamespaceDecl) {
        self.both(&decl.id, SymbolKind::Namespace);
        self.scoped(ScopeKind::Namespace, decl.span, |this| {
            decl.body.visit_with(this)
        });
    }
    fn visit_ts_import_equals_decl(&mut self, decl: &ast::TsImportEqualsDecl) {
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
    fn visit_ts_qualified_name(&mut self, name: &ast::TsQualifiedName) {
        name.left.visit_with(self);
    }
    fn visit_ts_type_query(&mut self, query: &ast::TsTypeQuery) {
        self.context(SymbolNamespace::Value, ReferenceKind::TypeQuery, |this| {
            query.expr_name.visit_with(this)
        });
        query.type_args.visit_with(self);
    }
    fn visit_ts_fn_type(&mut self, ty: &ast::TsFnType) {
        self.scoped(ScopeKind::Type, ty.span, |this| {
            ty.visit_children_with(this)
        });
    }
    fn visit_ts_constructor_type(&mut self, ty: &ast::TsConstructorType) {
        self.scoped(ScopeKind::Type, ty.span, |this| {
            ty.visit_children_with(this)
        });
    }
    fn visit_ts_fn_param(&mut self, param: &ast::TsFnParam) {
        let pat = match param {
            ast::TsFnParam::Ident(pat) => ast::Pat::Ident(pat.clone()),
            ast::TsFnParam::Array(pat) => ast::Pat::Array(pat.clone()),
            ast::TsFnParam::Rest(pat) => ast::Pat::Rest(pat.clone()),
            ast::TsFnParam::Object(pat) => ast::Pat::Object(pat.clone()),
        };
        self.bind(&pat, SymbolKind::Parameter);
    }
    fn visit_ts_mapped_type(&mut self, ty: &ast::TsMappedType) {
        self.scoped(ScopeKind::Type, ty.span, |this| {
            ty.visit_children_with(this)
        });
    }
    fn visit_ts_conditional_type(&mut self, ty: &ast::TsConditionalType) {
        ty.check_type.visit_with(self);
        self.scoped(ScopeKind::Type, ty.span, |this| {
            ty.extends_type.visit_with(this);
            ty.true_type.visit_with(this);
        });
        ty.false_type.visit_with(self);
    }
    fn visit_ts_property_signature(&mut self, prop: &ast::TsPropertySignature) {
        if prop.computed {
            self.context(SymbolNamespace::Value, ReferenceKind::Read, |this| {
                prop.key.visit_with(this)
            });
        }
        prop.type_ann.visit_with(self);
    }
    fn visit_ts_getter_signature(&mut self, prop: &ast::TsGetterSignature) {
        if prop.computed {
            self.context(SymbolNamespace::Value, ReferenceKind::Read, |this| {
                prop.key.visit_with(this)
            });
        }
        prop.type_ann.visit_with(self);
    }
    fn visit_ts_setter_signature(&mut self, prop: &ast::TsSetterSignature) {
        if prop.computed {
            self.context(SymbolNamespace::Value, ReferenceKind::Read, |this| {
                prop.key.visit_with(this)
            });
        }
        self.scoped(ScopeKind::Type, prop.span, |this| {
            prop.param.visit_with(this)
        });
    }
    fn visit_ts_method_signature(&mut self, method: &ast::TsMethodSignature) {
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
    fn visit_ts_call_signature_decl(&mut self, sig: &ast::TsCallSignatureDecl) {
        self.scoped(ScopeKind::Type, sig.span, |this| {
            sig.visit_children_with(this)
        });
    }
    fn visit_ts_construct_signature_decl(&mut self, sig: &ast::TsConstructSignatureDecl) {
        self.scoped(ScopeKind::Type, sig.span, |this| {
            sig.visit_children_with(this)
        });
    }
    fn visit_ts_index_signature(&mut self, sig: &ast::TsIndexSignature) {
        self.scoped(ScopeKind::Type, sig.span, |this| {
            sig.visit_children_with(this)
        });
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
