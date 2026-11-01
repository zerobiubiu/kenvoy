//! 基于实际 Cargo 解析图验证产品边界，而不是搜索 manifest 字符串。

use crate::{Result, cargo_command};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

#[derive(Deserialize)]
struct Metadata {
    packages: Vec<Package>,
    workspace_members: Vec<String>,
    workspace_default_members: Vec<String>,
    resolve: Resolve,
}

#[derive(Deserialize)]
struct Package {
    id: String,
    name: String,
    targets: Vec<Target>,
    dependencies: Vec<Dependency>,
    publish: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct Target {
    kind: Vec<String>,
}

#[derive(Deserialize)]
struct Dependency {
    name: String,
    kind: Option<String>,
    path: Option<String>,
}

#[derive(Deserialize)]
struct Resolve {
    nodes: Vec<Node>,
}

#[derive(Deserialize)]
struct Node {
    id: String,
    deps: Vec<Edge>,
}

#[derive(Deserialize)]
struct Edge {
    pkg: String,
    dep_kinds: Vec<DependencyKind>,
}

#[derive(Deserialize)]
struct DependencyKind {
    kind: Option<String>,
}

pub(crate) fn verify(root: &Path) -> Result<()> {
    let output = cargo_command(root)
        .args(["metadata", "--format-version", "1", "--offline", "--locked"])
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "Cargo 元数据读取失败：{}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let metadata: Metadata = serde_json::from_slice(&output.stdout)?;
    let count = verify_metadata(&metadata)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "status": "通过",
            "workspace": ["kengine:lib", "kenvoy:bin", "kenvoy-tests:tests", "xtask:bin"],
            "product_dependency": "kenvoy -> kengine",
            "kengine_normal_dependency_packages": count,
        }))?
    );
    Ok(())
}

fn verify_metadata(metadata: &Metadata) -> Result<usize> {
    let members: BTreeMap<_, _> = metadata
        .packages
        .iter()
        .filter(|package| metadata.workspace_members.contains(&package.id))
        .map(|package| (package.name.as_str(), package))
        .collect();
    let expected = BTreeSet::from(["kengine", "kenvoy", "kenvoy-tests", "xtask"]);
    let actual: BTreeSet<_> = members.keys().copied().collect();
    if actual != expected {
        return Err(format!("Workspace 成员不符合约定：{actual:?}").into());
    }
    let defaults: BTreeSet<_> = metadata
        .packages
        .iter()
        .filter(|package| metadata.workspace_default_members.contains(&package.id))
        .map(|package| package.name.as_str())
        .collect();
    if defaults != BTreeSet::from(["kengine", "kenvoy"]) {
        return Err(format!("默认成员必须只包含两个产品 Crate：{defaults:?}").into());
    }
    let engine = members["kengine"];
    let host = members["kenvoy"];
    let has = |package: &Package, kind: &str| {
        package
            .targets
            .iter()
            .filter(|target| target.kind.iter().any(|item| item == kind))
            .count()
    };
    if has(engine, "lib") != 1 || has(engine, "bin") != 0 {
        return Err("KEngine 必须只有一个 Library，不允许 Binary".into());
    }
    if has(host, "bin") != 1 || has(host, "lib") != 0 {
        return Err("Kenvoy 必须只有一个产品 Binary，不增加 Library".into());
    }
    if !host.dependencies.iter().any(|dependency| {
        dependency.name == "kengine" && dependency.kind.is_none() && dependency.path.is_some()
    }) {
        return Err("Kenvoy 缺少对 KEngine 的普通本地路径依赖".into());
    }
    for name in ["kenvoy-tests", "xtask"] {
        if members[name]
            .publish
            .as_ref()
            .is_none_or(|registries| !registries.is_empty())
        {
            return Err(format!("内部工程成员 {name} 必须设置 publish = false").into());
        }
    }
    if has(members["kenvoy-tests"], "bin") != 0 {
        return Err("跨 Crate 测试成员不能新增产品 Binary".into());
    }
    let packages: BTreeMap<_, _> = metadata
        .packages
        .iter()
        .map(|package| (package.id.as_str(), package))
        .collect();
    let nodes: BTreeMap<_, _> = metadata
        .resolve
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();
    let engine_dependencies = normal_closure(&engine.id, &nodes)?;
    let forbidden = [
        "kenvoy",
        "kenvoy-tests",
        "xtask",
        "axum",
        "tokio",
        "tower",
        "tower-http",
        "rusqlite",
        "rquickjs",
        "notify",
    ];
    for id in &engine_dependencies {
        let package = packages.get(id).ok_or("Cargo 解析图引用了不存在的包")?;
        if forbidden.contains(&package.name.as_str()) {
            return Err(format!("KEngine 普通依赖越界：{}", package.name).into());
        }
    }
    for id in normal_closure(&host.id, &nodes)? {
        let package = packages.get(id).ok_or("Cargo 解析图引用了不存在的包")?;
        if ["kenvoy-tests", "xtask"].contains(&package.name.as_str()) {
            return Err(format!("产品依赖了内部工程成员：{}", package.name).into());
        }
    }
    Ok(engine_dependencies.len().saturating_sub(1))
}

fn normal_closure<'a>(
    start: &'a str,
    nodes: &BTreeMap<&str, &'a Node>,
) -> Result<BTreeSet<&'a str>> {
    let mut visited = BTreeSet::new();
    let mut pending = vec![start];
    while let Some(id) = pending.pop() {
        if !visited.insert(id) {
            continue;
        }
        let node = nodes
            .get(id)
            .ok_or_else(|| format!("Cargo 解析图缺少节点：{id}"))?;
        for edge in &node.deps {
            if edge.dep_kinds.iter().any(|kind| kind.kind.is_none()) {
                pending.push(edge.pkg.as_str());
            }
        }
    }
    Ok(visited)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_closure_ignores_dev_and_build_edges_and_handles_cycles() -> Result<()> {
        let nodes: Vec<Node> = serde_json::from_value(serde_json::json!([
            {"id":"engine","deps":[
                {"pkg":"generic","dep_kinds":[{"kind":null}]},
                {"pkg":"web","dep_kinds":[{"kind":"dev"}]},
                {"pkg":"build","dep_kinds":[{"kind":"build"}]}
            ]},
            {"id":"generic","deps":[{"pkg":"engine","dep_kinds":[{"kind":null}]}]}
        ]))?;
        let graph = nodes.iter().map(|node| (node.id.as_str(), node)).collect();
        assert_eq!(
            normal_closure("engine", &graph)?,
            BTreeSet::from(["engine", "generic"])
        );
        Ok(())
    }

    #[test]
    fn absent_normal_dependency_cannot_silently_pass() {
        let node = Node {
            id: "engine".into(),
            deps: vec![Edge {
                pkg: "missing".into(),
                dep_kinds: vec![DependencyKind { kind: None }],
            }],
        };
        assert!(normal_closure("engine", &BTreeMap::from([("engine", &node)])).is_err());
    }

    fn fixture() -> Result<Metadata> {
        Ok(serde_json::from_value(serde_json::json!({
            "packages":[
                {"id":"engine","name":"kengine","targets":[{"kind":["lib"]}],"dependencies":[],"publish":null},
                {"id":"host","name":"kenvoy","targets":[{"kind":["bin"]}],"dependencies":[{"name":"kengine","kind":null,"path":"../kengine"}],"publish":null},
                {"id":"tests","name":"kenvoy-tests","targets":[{"kind":["lib"]}],"dependencies":[],"publish":[]},
                {"id":"task","name":"xtask","targets":[{"kind":["bin"]}],"dependencies":[],"publish":[]},
                {"id":"relay","name":"relay","targets":[{"kind":["lib"]}],"dependencies":[],"publish":null}
            ],
            "workspace_members":["engine","host","tests","task"],
            "workspace_default_members":["engine","host"],
            "resolve":{"nodes":[
                {"id":"engine","deps":[]},
                {"id":"host","deps":[{"pkg":"engine","dep_kinds":[{"kind":null}]}]},
                {"id":"tests","deps":[]},{"id":"task","deps":[]},{"id":"relay","deps":[]}
            ]}
        }))?)
    }

    #[test]
    fn default_members_and_internal_publish_flags_are_enforced() -> Result<()> {
        let mut metadata = fixture()?;
        assert!(verify_metadata(&metadata).is_ok());
        metadata.workspace_default_members.push("task".into());
        assert!(verify_metadata(&metadata).is_err());
        metadata.workspace_default_members.pop();
        metadata
            .packages
            .iter_mut()
            .find(|package| package.id == "tests")
            .ok_or("缺少夹具包")?
            .publish = None;
        assert!(verify_metadata(&metadata).is_err());
        Ok(())
    }

    #[test]
    fn indirect_product_dependency_cannot_reach_engineering_members() -> Result<()> {
        let mut metadata = fixture()?;
        metadata
            .resolve
            .nodes
            .iter_mut()
            .find(|node| node.id == "host")
            .ok_or("缺少宿主节点")?
            .deps
            .push(Edge {
                pkg: "relay".into(),
                dep_kinds: vec![DependencyKind { kind: None }],
            });
        metadata
            .resolve
            .nodes
            .iter_mut()
            .find(|node| node.id == "relay")
            .ok_or("缺少转接节点")?
            .deps
            .push(Edge {
                pkg: "task".into(),
                dep_kinds: vec![DependencyKind { kind: None }],
            });
        assert!(verify_metadata(&metadata).is_err());
        Ok(())
    }
}
