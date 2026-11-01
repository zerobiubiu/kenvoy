//! 模块级导入、别名与重导出解析；边界检查追踪真实目标而非别名拼写。

use super::rust_syntax::{attributes, imports, test_only};
use std::collections::{BTreeMap, BTreeSet};
use syn::Item;

#[derive(Clone, Debug)]
pub(super) enum Target {
    Internal(Vec<String>),
    External(Vec<String>),
}

impl Target {
    fn append(mut self, suffix: &[String]) -> Self {
        match &mut self {
            Self::Internal(path) | Self::External(path) => path.extend_from_slice(suffix),
        }
        self
    }
}

#[derive(Default)]
struct Module {
    names: BTreeSet<String>,
    aliases: BTreeMap<String, Vec<String>>,
    globs: Vec<Vec<String>>,
}

#[derive(Default)]
pub(super) struct Catalog {
    modules: BTreeMap<String, BTreeMap<Vec<String>, Module>>,
}

impl Catalog {
    pub fn add(&mut self, owner: &str, module: &[String], items: &[Item]) {
        for item in items {
            if test_only(attributes(item)) {
                continue;
            }
            let scope = self
                .modules
                .entry(owner.to_owned())
                .or_default()
                .entry(module.to_vec())
                .or_default();
            let name = match item {
                Item::Const(item) => Some(&item.ident),
                Item::Enum(item) => Some(&item.ident),
                Item::Fn(item) => Some(&item.sig.ident),
                Item::Mod(item) => Some(&item.ident),
                Item::Static(item) => Some(&item.ident),
                Item::Struct(item) => Some(&item.ident),
                Item::Trait(item) => Some(&item.ident),
                Item::TraitAlias(item) => Some(&item.ident),
                Item::Type(item) => Some(&item.ident),
                Item::Union(item) => Some(&item.ident),
                _ => None,
            };
            if let Some(name) = name {
                scope.names.insert(name.to_string());
            }
            if let Item::Use(item) = item {
                for import in imports(&item.tree) {
                    if import.glob {
                        scope.globs.push(import.path);
                    } else if let Some(alias) = import.alias {
                        scope.aliases.insert(alias, import.path);
                    }
                }
            }
            if let Item::ExternCrate(item) = item {
                let alias = item
                    .rename
                    .as_ref()
                    .map(|(_, name)| name)
                    .unwrap_or(&item.ident);
                scope
                    .aliases
                    .insert(alias.to_string(), vec![item.ident.to_string()]);
            }
            if let Item::Mod(item) = item
                && let Some((_, items)) = &item.content
            {
                let mut nested = module.to_vec();
                nested.push(item.ident.to_string());
                self.add(owner, &nested, items);
            }
        }
    }

    pub fn resolve(&self, owner: &str, module: &[String], path: &[String]) -> Target {
        self.resolve_at(owner, module, path, 0)
    }

    fn scope(&self, owner: &str, module: &[String]) -> Option<&Module> {
        self.modules.get(owner)?.get(module)
    }

    fn resolve_at(&self, owner: &str, module: &[String], path: &[String], depth: usize) -> Target {
        if depth > 64 {
            return Target::Internal(path.to_vec());
        }
        let Some(first) = path.first().map(String::as_str) else {
            return Target::Internal(module.to_vec());
        };
        if first == "crate" {
            return self.expand(owner, &path[1..], depth + 1);
        }
        if first == "self" {
            return self.expand(owner, &[module, &path[1..]].concat(), depth + 1);
        }
        if first == "super" {
            let mut base = module.to_vec();
            let count = path
                .iter()
                .take_while(|part| part.as_str() == "super")
                .count();
            base.truncate(base.len().saturating_sub(count));
            base.extend_from_slice(&path[count..]);
            return self.expand(owner, &base, depth + 1);
        }
        if let Some(scope) = self.scope(owner, module) {
            if let Some(alias) = scope.aliases.get(first) {
                // extern crate 自别名直接指向外部包，避免递归自身。
                if alias.len() == 1 && alias[0] == first {
                    return Target::External(path.to_vec());
                }
                return self
                    .resolve_at(owner, module, alias, depth + 1)
                    .append(&path[1..]);
            }
            if scope.names.contains(first) {
                return self.expand(owner, &[module, path].concat(), depth + 1);
            }
            if let Some(target) = self.resolve_globs(owner, module, first, depth + 1) {
                return target.append(&path[1..]);
            }
        }
        Target::External(path.to_vec())
    }

    fn expand(&self, owner: &str, path: &[String], depth: usize) -> Target {
        if depth > 64 {
            return Target::Internal(path.to_vec());
        }
        for index in 0..path.len() {
            let prefix = &path[..index];
            if let Some(scope) = self.scope(owner, prefix) {
                if let Some(alias) = scope.aliases.get(&path[index]) {
                    return self
                        .resolve_at(owner, prefix, alias, depth + 1)
                        .append(&path[index + 1..]);
                }
                if !scope.names.contains(&path[index])
                    && let Some(target) = self.resolve_globs(owner, prefix, &path[index], depth + 1)
                {
                    return target.append(&path[index + 1..]);
                }
            }
        }
        Target::Internal(path.to_vec())
    }

    fn resolve_globs(
        &self,
        owner: &str,
        module: &[String],
        name: &str,
        depth: usize,
    ) -> Option<Target> {
        if depth > 64 {
            return None;
        }
        let scope = self.scope(owner, module)?;
        for glob in &scope.globs {
            let Target::Internal(target) = self.resolve_at(owner, module, glob, depth + 1) else {
                continue;
            };
            let Some(imported) = self.scope(owner, &target) else {
                continue;
            };
            if imported.names.contains(name) || imported.aliases.contains_key(name) {
                let mut target = target;
                target.push(name.to_owned());
                return Some(self.expand(owner, &target, depth + 1));
            }
            if let Some(target) = self.resolve_globs(owner, &target, name, depth + 1) {
                return Some(target);
            }
        }
        None
    }
}
