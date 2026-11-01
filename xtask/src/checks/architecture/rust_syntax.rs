//! 从 syn 节点提取 use、cfg(test) 与模块路径，不读取注释或字符串里的伪代码。

use std::path::{Path, PathBuf};
use syn::{
    Attribute, Expr, Item, Lit, Meta, Token, UseTree, parse::Parser, punctuated::Punctuated,
};

pub(super) struct Import {
    pub path: Vec<String>,
    pub alias: Option<String>,
    pub glob: bool,
}

pub(super) fn imports(tree: &UseTree) -> Vec<Import> {
    let mut result = Vec::new();
    flatten(tree, &mut Vec::new(), &mut result);
    result
}

fn flatten(tree: &UseTree, prefix: &mut Vec<String>, result: &mut Vec<Import>) {
    match tree {
        UseTree::Path(path) => {
            prefix.push(path.ident.to_string());
            flatten(&path.tree, prefix, result);
            prefix.pop();
        }
        UseTree::Group(group) => {
            for item in &group.items {
                flatten(item, prefix, result);
            }
        }
        UseTree::Glob(_) => result.push(Import {
            path: prefix.clone(),
            alias: None,
            glob: true,
        }),
        UseTree::Name(name) => {
            let mut path = prefix.clone();
            if name.ident != "self" {
                path.push(name.ident.to_string());
            }
            result.push(Import {
                alias: path.last().cloned(),
                path,
                glob: false,
            });
        }
        UseTree::Rename(rename) => {
            let mut path = prefix.clone();
            if rename.ident != "self" {
                path.push(rename.ident.to_string());
            }
            result.push(Import {
                path,
                alias: Some(rename.rename.to_string()),
                glob: false,
            });
        }
    }
}

pub(super) fn test_only(attributes: &[Attribute]) -> bool {
    attributes.iter().any(|attribute| {
        if attribute
            .path()
            .segments
            .last()
            .is_some_and(|part| part.ident == "test")
        {
            return true;
        }
        if !attribute.path().is_ident("cfg") {
            return false;
        }
        attribute
            .parse_args::<Meta>()
            .is_ok_and(|meta| requires_test(&meta))
    })
}

fn requires_test(meta: &Meta) -> bool {
    match meta {
        Meta::Path(path) => path.is_ident("test"),
        Meta::List(list) => {
            let Ok(items) =
                Punctuated::<Meta, Token![,]>::parse_terminated.parse2(list.tokens.clone())
            else {
                return false;
            };
            if list.path.is_ident("all") {
                items.iter().any(requires_test)
            } else if list.path.is_ident("any") {
                !items.is_empty() && items.iter().all(requires_test)
            } else {
                false
            }
        }
        Meta::NameValue(_) => false,
    }
}

pub(super) fn attributes(item: &Item) -> &[Attribute] {
    match item {
        Item::Const(item) => &item.attrs,
        Item::Enum(item) => &item.attrs,
        Item::ExternCrate(item) => &item.attrs,
        Item::Fn(item) => &item.attrs,
        Item::ForeignMod(item) => &item.attrs,
        Item::Impl(item) => &item.attrs,
        Item::Macro(item) => &item.attrs,
        Item::Mod(item) => &item.attrs,
        Item::Static(item) => &item.attrs,
        Item::Struct(item) => &item.attrs,
        Item::Trait(item) => &item.attrs,
        Item::TraitAlias(item) => &item.attrs,
        Item::Type(item) => &item.attrs,
        Item::Union(item) => &item.attrs,
        Item::Use(item) => &item.attrs,
        _ => &[],
    }
}

pub(super) fn module_path(path: &Path) -> (String, Vec<String>) {
    let parts: Vec<_> = path
        .iter()
        .map(|part| part.to_string_lossy().into_owned())
        .collect();
    let Some(src) = parts.iter().position(|part| part == "src") else {
        return (path.display().to_string(), Vec::new());
    };
    let mut module = parts[src + 1..parts.len() - 1].to_vec();
    if let Some(stem) = path.file_stem().and_then(|value| value.to_str())
        && !["main", "lib", "mod"].contains(&stem)
    {
        module.push(stem.to_owned());
    }
    (parts[..src].join("/"), module)
}

pub(super) fn child_directory(file: &Path) -> PathBuf {
    if file
        .file_stem()
        .is_some_and(|stem| stem == "mod" || stem == "lib" || stem == "main")
    {
        file.parent().unwrap_or(Path::new("")).to_path_buf()
    } else {
        file.with_extension("")
    }
}

pub(super) fn external_module(file: &Path, directory: &Path, item: &syn::ItemMod) -> Vec<PathBuf> {
    for attribute in &item.attrs {
        if attribute.path().is_ident("path")
            && let Meta::NameValue(value) = &attribute.meta
            && let Expr::Lit(value) = &value.value
            && let Lit::Str(value) = &value.lit
        {
            let base = if directory == child_directory(file) {
                file.parent().unwrap_or(Path::new(""))
            } else {
                directory
            };
            return vec![normalize(&base.join(value.value()))];
        }
    }
    vec![
        directory.join(format!("{}.rs", item.ident)),
        directory.join(item.ident.to_string()).join("mod.rs"),
    ]
}

pub(super) fn normalize(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for part in path.components() {
        match part {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                result.pop();
            }
            part => result.push(part.as_os_str()),
        }
    }
    result
}

pub(super) fn expression_attributes(expression: &Expr) -> &[Attribute] {
    match expression {
        Expr::Array(item) => &item.attrs,
        Expr::Assign(item) => &item.attrs,
        Expr::Async(item) => &item.attrs,
        Expr::Await(item) => &item.attrs,
        Expr::Binary(item) => &item.attrs,
        Expr::Block(item) => &item.attrs,
        Expr::Break(item) => &item.attrs,
        Expr::Call(item) => &item.attrs,
        Expr::Cast(item) => &item.attrs,
        Expr::Closure(item) => &item.attrs,
        Expr::Const(item) => &item.attrs,
        Expr::Continue(item) => &item.attrs,
        Expr::Field(item) => &item.attrs,
        Expr::ForLoop(item) => &item.attrs,
        Expr::Group(item) => &item.attrs,
        Expr::If(item) => &item.attrs,
        Expr::Index(item) => &item.attrs,
        Expr::Infer(item) => &item.attrs,
        Expr::Let(item) => &item.attrs,
        Expr::Lit(item) => &item.attrs,
        Expr::Loop(item) => &item.attrs,
        Expr::Macro(item) => &item.attrs,
        Expr::Match(item) => &item.attrs,
        Expr::MethodCall(item) => &item.attrs,
        Expr::Paren(item) => &item.attrs,
        Expr::Path(item) => &item.attrs,
        Expr::Range(item) => &item.attrs,
        Expr::RawAddr(item) => &item.attrs,
        Expr::Reference(item) => &item.attrs,
        Expr::Repeat(item) => &item.attrs,
        Expr::Return(item) => &item.attrs,
        Expr::Struct(item) => &item.attrs,
        Expr::Try(item) => &item.attrs,
        Expr::TryBlock(item) => &item.attrs,
        Expr::Tuple(item) => &item.attrs,
        Expr::Unary(item) => &item.attrs,
        Expr::Unsafe(item) => &item.attrs,
        Expr::While(item) => &item.attrs,
        Expr::Yield(item) => &item.attrs,
        _ => &[],
    }
}
