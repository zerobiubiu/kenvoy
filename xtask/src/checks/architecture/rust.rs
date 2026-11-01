//! 通过 syn 的 use/path 节点检查实际 Rust 层依赖；测试项只受行数门禁约束。

use super::{
    layout::Source,
    rust_catalog::Catalog,
    rust_syntax::{child_directory, external_module, module_path, test_only},
    rust_visitor::inspect_file,
};
use crate::Result;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};
use syn::Item;

pub(super) fn inspect(sources: &[Source], errors: &mut Vec<String>) -> Result<usize> {
    let units: BTreeMap<_, _> = sources
        .iter()
        .filter(|source| {
            source
                .path
                .extension()
                .is_some_and(|extension| extension == "rs")
        })
        .map(|source| {
            syn::parse_file(&source.text)
                .map(|syntax| (source.path.clone(), syntax))
                .map_err(|error| format!("Rust 源码解析失败：{}：{error}", source.path.display()))
        })
        .collect::<std::result::Result<_, _>>()?;
    let mut tests = BTreeSet::new();
    let mut seen = BTreeSet::new();
    for (path, syntax) in &units {
        let testing = path.iter().any(|part| part == "tests") || test_only(&syntax.attrs);
        if testing {
            tests.insert(path.clone());
        }
        mark_modules(
            path,
            &child_directory(path),
            &syntax.items,
            testing,
            &units,
            &mut tests,
            &mut seen,
        );
    }
    let mut catalog = Catalog::default();
    for (path, syntax) in &units {
        if !tests.contains(path) {
            let (owner, module) = module_path(path);
            catalog.add(&owner, &module, &syntax.items);
        }
    }
    for (path, syntax) in &units {
        if tests.contains(path) {
            continue;
        }
        let (owner, module) = module_path(path);
        inspect_file(path, &owner, module, syntax, &catalog, errors);
    }
    Ok(units.len())
}

fn mark_modules(
    file: &Path,
    directory: &Path,
    items: &[Item],
    testing: bool,
    units: &BTreeMap<PathBuf, syn::File>,
    tests: &mut BTreeSet<PathBuf>,
    seen: &mut BTreeSet<(PathBuf, bool)>,
) {
    for item in items {
        let Item::Mod(item) = item else {
            continue;
        };
        let testing = testing || test_only(&item.attrs);
        if let Some((_, items)) = &item.content {
            mark_modules(
                file,
                &directory.join(item.ident.to_string()),
                items,
                testing,
                units,
                tests,
                seen,
            );
        } else {
            for path in external_module(file, directory, item) {
                if let Some(syntax) = units.get(&path) {
                    if testing {
                        tests.insert(path.clone());
                    }
                    if seen.insert((path.clone(), testing)) {
                        mark_modules(
                            &path,
                            &child_directory(&path),
                            &syntax.items,
                            testing,
                            units,
                            tests,
                            seen,
                        );
                    }
                }
            }
        }
    }
}
