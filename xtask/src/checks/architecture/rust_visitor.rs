//! 生产路径依赖、HTTP 装配接缝与 cfg(test) 语法访问。

use super::{
    rust_catalog::{Catalog, Target},
    rust_syntax::{attributes, expression_attributes, imports, test_only},
};
use std::path::Path;
use syn::{
    Item,
    visit::{self, Visit},
};

pub(super) fn inspect_file(
    path: &Path,
    owner: &str,
    module: Vec<String>,
    syntax: &syn::File,
    catalog: &Catalog,
    errors: &mut Vec<String>,
) {
    Inspector {
        path,
        owner,
        module,
        catalog,
        errors,
    }
    .visit_file(syntax);
}

struct Inspector<'a> {
    path: &'a Path,
    owner: &'a str,
    module: Vec<String>,
    catalog: &'a Catalog,
    errors: &'a mut Vec<String>,
}

impl Inspector<'_> {
    fn check(&mut self, path: &[String]) {
        let target = self.catalog.resolve(self.owner, &self.module, path);
        let Some(layer) = self.module.first().map(String::as_str) else {
            return;
        };
        let violation = match &target {
            Target::Internal(target) => {
                let target_layer = target.first().map(String::as_str).unwrap_or("");
                match layer {
                    "domain" => ["application", "infrastructure", "interfaces", "bootstrap"]
                        .contains(&target_layer),
                    "application" => {
                        ["infrastructure", "interfaces", "bootstrap"].contains(&target_layer)
                    }
                    "infrastructure" => ["interfaces", "bootstrap"].contains(&target_layer),
                    "interfaces" if self.http_controller() => {
                        target_layer == "infrastructure"
                            && target.get(1).is_some_and(|part| part == "persistence")
                    }
                    _ => false,
                }
            }
            Target::External(target) if layer == "domain" => domain_driver(target),
            Target::External(target) if self.http_controller() => {
                target.first().is_some_and(|root| root == "rusqlite")
            }
            _ => false,
        };
        if violation {
            let resolved = match target {
                Target::Internal(path) => format!("crate::{}", path.join("::")),
                Target::External(path) => path.join("::"),
            };
            self.errors.push(format!(
                "Rust 层依赖越界：{}：{layer} -> {resolved}",
                self.path.display()
            ));
        }
    }

    fn http_controller(&self) -> bool {
        self.module.first().is_some_and(|part| part == "interfaces")
            && self.module.get(1).is_some_and(|part| part == "http")
            && self.module.get(2).is_none_or(|part| part != "wiring")
    }
}

impl<'ast> Visit<'ast> for Inspector<'_> {
    fn visit_item(&mut self, item: &'ast Item) {
        if !test_only(attributes(item)) {
            visit::visit_item(self, item);
        }
    }

    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        if let Some((_, items)) = &item.content {
            self.module.push(item.ident.to_string());
            for item in items {
                self.visit_item(item);
            }
            self.module.pop();
        }
    }

    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        for import in imports(&item.tree) {
            if import.glob
                && import.path.len() == 1
                && ["crate", "super"].contains(&import.path[0].as_str())
            {
                self.errors.push(format!(
                    "生产源码不允许兜底通配导入：{}：{}::*",
                    self.path.display(),
                    import.path[0]
                ));
            }
            self.check(&import.path);
        }
    }

    fn visit_item_extern_crate(&mut self, item: &'ast syn::ItemExternCrate) {
        self.check(&[item.ident.to_string()]);
    }

    fn visit_path(&mut self, path: &'ast syn::Path) {
        self.check(
            &path
                .segments
                .iter()
                .map(|part| part.ident.to_string())
                .collect::<Vec<_>>(),
        );
        visit::visit_path(self, path);
    }

    fn visit_impl_item(&mut self, item: &'ast syn::ImplItem) {
        let attrs = match item {
            syn::ImplItem::Const(item) => &item.attrs,
            syn::ImplItem::Fn(item) => &item.attrs,
            syn::ImplItem::Type(item) => &item.attrs,
            syn::ImplItem::Macro(item) => &item.attrs,
            _ => return visit::visit_impl_item(self, item),
        };
        if !test_only(attrs) {
            visit::visit_impl_item(self, item);
        }
    }

    fn visit_trait_item(&mut self, item: &'ast syn::TraitItem) {
        let attrs = match item {
            syn::TraitItem::Const(item) => &item.attrs,
            syn::TraitItem::Fn(item) => &item.attrs,
            syn::TraitItem::Type(item) => &item.attrs,
            syn::TraitItem::Macro(item) => &item.attrs,
            _ => return visit::visit_trait_item(self, item),
        };
        if !test_only(attrs) {
            visit::visit_trait_item(self, item);
        }
    }

    fn visit_expr(&mut self, expression: &'ast syn::Expr) {
        if !test_only(expression_attributes(expression)) {
            visit::visit_expr(self, expression);
        }
    }

    fn visit_field(&mut self, field: &'ast syn::Field) {
        if !test_only(&field.attrs) {
            visit::visit_field(self, field);
        }
    }

    fn visit_variant(&mut self, variant: &'ast syn::Variant) {
        if !test_only(&variant.attrs) {
            visit::visit_variant(self, variant);
        }
    }

    fn visit_foreign_item(&mut self, item: &'ast syn::ForeignItem) {
        let attrs = match item {
            syn::ForeignItem::Fn(item) => &item.attrs,
            syn::ForeignItem::Static(item) => &item.attrs,
            syn::ForeignItem::Type(item) => &item.attrs,
            syn::ForeignItem::Macro(item) => &item.attrs,
            _ => return visit::visit_foreign_item(self, item),
        };
        if !test_only(attrs) {
            visit::visit_foreign_item(self, item);
        }
    }

    fn visit_local(&mut self, local: &'ast syn::Local) {
        if !test_only(&local.attrs) {
            visit::visit_local(self, local);
        }
    }

    fn visit_arm(&mut self, arm: &'ast syn::Arm) {
        if !test_only(&arm.attrs) {
            visit::visit_arm(self, arm);
        }
    }

    fn visit_stmt_macro(&mut self, statement: &'ast syn::StmtMacro) {
        if !test_only(&statement.attrs) {
            visit::visit_stmt_macro(self, statement);
        }
    }

    fn visit_field_value(&mut self, field: &'ast syn::FieldValue) {
        if !test_only(&field.attrs) {
            visit::visit_field_value(self, field);
        }
    }
}

fn domain_driver(path: &[String]) -> bool {
    let Some(root) = path.first().map(String::as_str) else {
        return false;
    };
    if root.starts_with("swc_")
        || [
            "axum",
            "tokio",
            "tower",
            "tower_http",
            "rusqlite",
            "rquickjs",
            "notify",
            "reqwest",
            "hyper",
            "tokio_tungstenite",
            "getrandom",
            "rand",
            "rand_core",
            "argon2",
            "hmac",
            "sha2",
            "rpassword",
            "react",
            "clap",
            "tracing",
            "tracing_subscriber",
        ]
        .contains(&root)
    {
        return true;
    }
    if [
        "include",
        "include_str",
        "include_bytes",
        "print",
        "println",
        "eprint",
        "eprintln",
        "dbg",
    ]
    .contains(&root)
    {
        return true;
    }
    if root == "kengine"
        && path
            .get(1)
            .is_some_and(|item| ["Engine", "AnalysisSession"].contains(&item.as_str()))
    {
        return true;
    }
    if root == "std" {
        let branch = path.get(1).map(String::as_str).unwrap_or("");
        return [
            "fs",
            "io",
            "net",
            "process",
            "env",
            "thread",
            "os",
            "include",
            "include_str",
            "include_bytes",
            "print",
            "println",
            "eprint",
            "eprintln",
            "dbg",
        ]
        .contains(&branch)
            || (branch == "time"
                && path
                    .get(2)
                    .is_some_and(|item| ["Instant", "SystemTime"].contains(&item.as_str())));
    }
    root == "chrono" && path.last().is_some_and(|item| item == "now")
}
