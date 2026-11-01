use swc_common::Spanned;
use swc_ecma_ast as ast;

use crate::domain::ir::{
    DeclarationIR, DeclarationKind, ExportIR, ExportKind, ExportSpecifierIR, ExportSpecifierKind,
    ImportIR, ImportSpecifierIR, ImportSpecifierKind, ModuleNameIR,
};

use super::Converter;

impl Converter<'_> {
    pub(super) fn module_declarations(&mut self, module: &ast::Module) {
        for item in &module.body {
            let ast::ModuleItem::ModuleDecl(declaration) = item else {
                continue;
            };
            match declaration {
                ast::ModuleDecl::Import(import) => {
                    let specifiers = import
                        .specifiers
                        .iter()
                        .map(|specifier| {
                            let local = specifier.local();
                            let (kind, imported) = match specifier {
                                ast::ImportSpecifier::Default(_) => {
                                    (ImportSpecifierKind::Default, None)
                                }
                                ast::ImportSpecifier::Namespace(_) => {
                                    (ImportSpecifierKind::Namespace, None)
                                }
                                ast::ImportSpecifier::Named(named) => (
                                    ImportSpecifierKind::Named,
                                    Some(
                                        named
                                            .imported
                                            .as_ref()
                                            .map(|name| self.module_name(name))
                                            .unwrap_or_else(|| {
                                                ModuleNameIR::Identifier(
                                                    self.identifier(local.sym.as_ref(), local.span),
                                                )
                                            }),
                                    ),
                                ),
                            };
                            ImportSpecifierIR {
                                span: self.span(specifier.span()),
                                kind,
                                imported,
                                local: self.identifier(local.sym.as_ref(), local.span),
                                type_only: import.type_only || specifier.is_type_only(),
                            }
                        })
                        .collect();
                    self.output.imports.push(ImportIR {
                        span: self.span(import.span),
                        source: Some(self.string(&import.src)),
                        specifiers,
                        type_only: import.type_only,
                        attributes: import.with.as_ref().map(|value| self.span(value.span)),
                        typescript_module_reference: None,
                    });
                }
                ast::ModuleDecl::TsImportEquals(import) => {
                    let source = match &import.module_ref {
                        ast::TsModuleRef::TsExternalModuleRef(value) => {
                            Some(self.string(&value.expr))
                        }
                        _ => None,
                    };
                    self.output.imports.push(ImportIR {
                        span: self.span(import.span),
                        source,
                        specifiers: vec![ImportSpecifierIR {
                            span: self.span(import.id.span),
                            kind: ImportSpecifierKind::TypeScriptEquals,
                            local: self.identifier(import.id.sym.as_ref(), import.id.span),
                            imported: None,
                            type_only: import.is_type_only,
                        }],
                        type_only: import.is_type_only,
                        attributes: None,
                        typescript_module_reference: Some(self.span(import.module_ref.span())),
                    });
                    if import.is_export {
                        self.output.exports.push(ExportIR {
                            span: self.span(import.span),
                            kind: ExportKind::Named,
                            source: None,
                            specifiers: vec![ExportSpecifierIR {
                                span: self.span(import.id.span),
                                kind: ExportSpecifierKind::Named,
                                local: Some(ModuleNameIR::Identifier(
                                    self.identifier(import.id.sym.as_ref(), import.id.span),
                                )),
                                exported: ModuleNameIR::Identifier(
                                    self.identifier(import.id.sym.as_ref(), import.id.span),
                                ),
                                type_only: import.is_type_only,
                            }],
                            type_only: import.is_type_only,
                            attributes: None,
                        });
                    }
                }
                ast::ModuleDecl::ExportDecl(export) => {
                    let span = export.decl.span();
                    self.exported.insert((span.lo.0, span.hi.0));
                    self.output.exports.push(ExportIR {
                        span: self.span(export.span),
                        kind: ExportKind::Declaration {
                            declaration_span: self.span(span),
                        },
                        source: None,
                        specifiers: vec![],
                        type_only: matches!(
                            export.decl,
                            ast::Decl::TsInterface(_) | ast::Decl::TsTypeAlias(_)
                        ),
                        attributes: None,
                    });
                }
                ast::ModuleDecl::ExportNamed(export) => {
                    let specifiers = export
                        .specifiers
                        .iter()
                        .map(|specifier| match specifier {
                            ast::ExportSpecifier::Named(named) => ExportSpecifierIR {
                                span: self.span(named.span),
                                kind: ExportSpecifierKind::Named,
                                local: Some(self.module_name(&named.orig)),
                                exported: self
                                    .module_name(named.exported.as_ref().unwrap_or(&named.orig)),
                                type_only: export.type_only || named.is_type_only,
                            },
                            ast::ExportSpecifier::Namespace(namespace) => ExportSpecifierIR {
                                span: self.span(namespace.span),
                                kind: ExportSpecifierKind::Namespace,
                                local: None,
                                exported: self.module_name(&namespace.name),
                                type_only: export.type_only,
                            },
                            ast::ExportSpecifier::Default(default) => ExportSpecifierIR {
                                span: self.span(default.exported.span),
                                kind: ExportSpecifierKind::Default,
                                local: None,
                                exported: ModuleNameIR::Identifier(self.identifier(
                                    default.exported.sym.as_ref(),
                                    default.exported.span,
                                )),
                                type_only: export.type_only,
                            },
                        })
                        .collect();
                    self.output.exports.push(ExportIR {
                        span: self.span(export.span),
                        kind: ExportKind::Named,
                        source: export.src.as_ref().map(|source| self.string(source)),
                        specifiers,
                        type_only: export.type_only,
                        attributes: export.with.as_ref().map(|value| self.span(value.span)),
                    });
                }
                ast::ModuleDecl::ExportAll(export) => self.output.exports.push(ExportIR {
                    span: self.span(export.span),
                    kind: ExportKind::All,
                    source: Some(self.string(&export.src)),
                    specifiers: vec![],
                    type_only: export.type_only,
                    attributes: export.with.as_ref().map(|value| self.span(value.span)),
                }),
                ast::ModuleDecl::ExportDefaultExpr(export) => {
                    let expression = self.expression(&export.expr);
                    self.output.exports.push(ExportIR {
                        span: self.span(export.span),
                        kind: ExportKind::DefaultExpression { expression },
                        source: None,
                        specifiers: vec![],
                        type_only: false,
                        attributes: None,
                    });
                }
                ast::ModuleDecl::ExportDefaultDecl(export) => {
                    let (kind, name) = match &export.decl {
                        ast::DefaultDecl::Fn(value) => {
                            (DeclarationKind::Function, value.ident.as_ref())
                        }
                        ast::DefaultDecl::Class(value) => {
                            (DeclarationKind::Class, value.ident.as_ref())
                        }
                        ast::DefaultDecl::TsInterfaceDecl(value) => {
                            (DeclarationKind::Interface, Some(&value.id))
                        }
                    };
                    let name = name.map(|name| self.identifier(name.sym.as_ref(), name.span));
                    let span = self.span(export.decl.span());
                    self.output.declarations.push(DeclarationIR {
                        span: span.clone(),
                        kind,
                        bindings: name.iter().cloned().collect(),
                        exported: true,
                        declarators: vec![],
                    });
                    self.output.exports.push(ExportIR {
                        span: self.span(export.span),
                        kind: ExportKind::DefaultDeclaration {
                            declaration_span: span,
                            name,
                        },
                        source: None,
                        specifiers: vec![],
                        type_only: kind == DeclarationKind::Interface,
                        attributes: None,
                    });
                }
                ast::ModuleDecl::TsExportAssignment(export) => {
                    let expression = self.expression(&export.expr);
                    self.output.exports.push(ExportIR {
                        span: self.span(export.span),
                        kind: ExportKind::TypeScriptAssignment { expression },
                        source: None,
                        specifiers: vec![],
                        type_only: false,
                        attributes: None,
                    });
                }
                ast::ModuleDecl::TsNamespaceExport(export) => self.output.exports.push(ExportIR {
                    span: self.span(export.span),
                    kind: ExportKind::TypeScriptNamespace {
                        name: self.identifier(export.id.sym.as_ref(), export.id.span),
                    },
                    source: None,
                    specifiers: vec![],
                    type_only: false,
                    attributes: None,
                }),
            }
        }
    }
}
