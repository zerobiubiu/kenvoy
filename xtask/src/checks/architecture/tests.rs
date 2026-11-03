use super::{
    layout::{self, Source},
    rust, typescript,
};
use crate::Result;
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

fn source(path: &str, text: &str) -> Source {
    Source {
        path: path.into(),
        text: text.into(),
    }
}

fn rust_errors(sources: &[Source]) -> Result<Vec<String>> {
    let mut errors = Vec::new();
    rust::inspect(sources, &mut errors)?;
    Ok(errors)
}

#[test]
fn rust_dependencies_follow_paths_aliases_and_public_facades() -> Result<()> {
    let errors = rust_errors(&[
        source(
            "crates/kenvoy/src/main.rs",
            "mod domain; mod infrastructure; pub use infrastructure::storage::Database as Store;",
        ),
        source(
            "crates/kenvoy/src/infrastructure/mod.rs",
            "pub mod storage;",
        ),
        source(
            "crates/kenvoy/src/infrastructure/storage.rs",
            "pub struct Database;",
        ),
        source(
            "crates/kenvoy/src/domain/mod.rs",
            r#"
            use std::{fs as disk, collections::BTreeMap};
            type Repository = crate::Store;
            fn read() { let _ = disk::read("input"); }
        "#,
        ),
    ])?;
    assert!(errors.iter().any(|error| error.contains("std::fs")));
    assert!(
        errors
            .iter()
            .any(|error| error.contains("infrastructure::storage::Database"))
    );
    let errors = rust_errors(&[
        source(
            "crates/kengine/src/lib.rs",
            "mod domain; mod interfaces; pub use interfaces::*;",
        ),
        source("crates/kengine/src/interfaces/mod.rs", "pub struct Engine;"),
        source(
            "crates/kengine/src/domain/mod.rs",
            "type HiddenOuterDependency = crate::Engine;",
        ),
    ])?;
    assert!(
        errors
            .iter()
            .any(|error| error.contains("interfaces::Engine"))
    );
    Ok(())
}

#[test]
fn rust_test_cfg_is_not_confused_with_production_cfg() -> Result<()> {
    let allowed = rust_errors(&[
        source(
            "crates/kenvoy/src/domain/mod.rs",
            r#"
            // use crate::infrastructure::*;
            const NOTE: &str = "std::fs::read and use crate::* are only text";
            #[cfg(all(test, unix))] mod inline { use crate::infrastructure::*; }
            #[cfg(test)] #[path = "helpers.rs"] mod external;
            mod nested { #[cfg(test)] #[path = "helper.rs"] mod helper; }
            struct Model { #[cfg(test)] store: crate::infrastructure::Database }
            fn check() { #[cfg(test)] { std::fs::read("test"); } }
        "#,
        ),
        source(
            "crates/kenvoy/src/domain/helpers.rs",
            "use crate::*; use std::fs;",
        ),
        source(
            "crates/kenvoy/src/domain/nested/helper.rs",
            "use std::fs; use crate::*;",
        ),
    ])?;
    assert!(allowed.is_empty(), "{allowed:?}");
    let errors = rust_errors(&[source(
        "crates/kenvoy/src/domain/mod.rs",
        "#[cfg(any(test, unix))] use std::net::TcpStream;",
    )])?;
    assert!(errors.iter().any(|error| error.contains("std::net")));
    Ok(())
}

#[test]
fn rust_layers_reject_reverse_dependencies_and_production_globs() -> Result<()> {
    let errors = rust_errors(&[
        source(
            "crates/kenvoy/src/application/use_case.rs",
            "type Store = crate::infrastructure::Database;",
        ),
        source(
            "crates/kenvoy/src/infrastructure/store.rs",
            "use crate::interfaces::http::HttpRuntime;",
        ),
        source(
            "crates/kenvoy/src/bootstrap/run.rs",
            "use crate::{*}; use super::*;",
        ),
        source(
            "crates/kengine/src/domain/parser.rs",
            "extern crate swc_ecma_parser as driver;",
        ),
    ])?;
    for expected in [
        "application -> crate::infrastructure",
        "infrastructure -> crate::interfaces",
        "兜底通配导入",
        "swc_ecma_parser",
    ] {
        assert!(
            errors.iter().any(|error| error.contains(expected)),
            "{expected}: {errors:?}"
        );
    }
    Ok(())
}

#[test]
fn http_wiring_may_bind_an_adapter_but_controllers_may_not_use_the_actor() -> Result<()> {
    let allowed = rust_errors(&[
        source(
            "crates/kenvoy/src/interfaces/http/wiring.rs",
            "use crate::infrastructure::persistence::sqlite::Database;",
        ),
        source(
            "crates/kenvoy/src/interfaces/http/records.rs",
            "use crate::application::resources::ResourceService;",
        ),
    ])?;
    assert!(allowed.is_empty(), "{allowed:?}");
    let errors = rust_errors(&[source(
        "crates/kenvoy/src/interfaces/http/records.rs",
        "use crate::infrastructure::persistence::sqlite::Database;",
    )])?;
    assert!(
        errors
            .iter()
            .any(|error| error.contains("persistence::sqlite::Database"))
    );
    Ok(())
}

fn frontend_errors(sources: &[Source]) -> Result<Vec<String>> {
    let mut errors = Vec::new();
    typescript::inspect(sources, &Default::default(), &mut errors)?;
    Ok(errors)
}

#[test]
fn frontend_imports_use_parsed_syntax_including_types_reexports_and_dynamic_imports() -> Result<()>
{
    let errors = frontend_errors(&[
        source(
            "crates/kenvoy/frontend/src/domain/model.ts",
            r#"
            import type { Client } from '../infrastructure/api.ts';
            export { client } from '../infrastructure/api.ts';
            export const load = () => import(/* 真实动态依赖 */ '../infrastructure/api.ts');
        "#,
        ),
        source(
            "crates/kenvoy/frontend/src/infrastructure/api.ts",
            "export interface Client {} export const client = 1;",
        ),
    ])?;
    assert!(errors.iter().any(|error| error.contains("前端层依赖越界")));
    let allowed = frontend_errors(&[source(
        "crates/kenvoy/frontend/src/domain/model.ts",
        r#"
            // import { client } from '../infrastructure/missing.ts';
            export const note = "import('../infrastructure/missing.ts')";
            export const url = new URL('https://example.test/image.png');
        "#,
    )])?;
    assert!(allowed.is_empty(), "{allowed:?}");
    Ok(())
}

#[test]
fn frontend_application_uses_ports_and_sdk_context_do_not_reach_the_registry() -> Result<()> {
    let allowed = frontend_errors(&[
        source(
            "crates/kenvoy/frontend/src/application/ports.ts",
            "export interface Client { upload(file: File): Promise<void> }",
        ),
        source(
            "crates/kenvoy/frontend/src/application/uploads.ts",
            "import type { Client } from './ports.ts'; export type Uploads = Client;",
        ),
        source(
            "crates/kenvoy/frontend/src/infrastructure/api.ts",
            "import type { Client } from '../application/ports.ts'; export type Api = Client;",
        ),
    ])?;
    assert!(allowed.is_empty(), "{allowed:?}");
    let errors = frontend_errors(&[
        source(
            "crates/kenvoy/frontend/src/sdk.ts",
            "export { value } from './presentation/runtime/context.ts';",
        ),
        source(
            "crates/kenvoy/frontend/src/presentation/runtime/context.ts",
            "export { value } from './RuntimePage.tsx';",
        ),
        source(
            "crates/kenvoy/frontend/src/presentation/runtime/RuntimePage.tsx",
            "import { components } from 'virtual:kenvoy-registry'; export const value = components;",
        ),
        source(
            "crates/kenvoy/frontend/src/application/uploads.ts",
            "import { client } from '../infrastructure/api.ts';",
        ),
        source(
            "crates/kenvoy/frontend/src/infrastructure/api.ts",
            "export const client = 1;",
        ),
    ])?;
    assert!(errors.iter().any(|error| error.contains("SDK/Context")));
    assert!(errors.iter().any(|error| error.contains("前端层依赖越界")));
    Ok(())
}

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Result<Self> {
        let path = std::env::temp_dir().join(format!(
            "kenvoy-architecture-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
        fs::create_dir(&path)?;
        Ok(Self(path))
    }

    fn write(&self, path: &str, text: &str) -> Result<()> {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().ok_or("夹具文件缺少父目录")?)?;
        fs::write(path, text)?;
        Ok(())
    }

    fn layout(&self) -> Result<()> {
        for (package, entry, layers) in [
            (
                "kenvoy",
                "main.rs",
                &[
                    "domain",
                    "application",
                    "infrastructure",
                    "interfaces",
                    "bootstrap",
                ][..],
            ),
            (
                "kengine",
                "lib.rs",
                &["domain", "application", "infrastructure", "interfaces"][..],
            ),
        ] {
            self.write(&format!("crates/{package}/src/{entry}"), "")?;
            for layer in layers {
                self.write(&format!("crates/{package}/src/{layer}/mod.rs"), "")?;
            }
        }
        for entry in [
            "main.tsx",
            "sdk.ts",
            "env.d.ts",
            "app/App.tsx",
            "domain/types.ts",
            "application/ports.ts",
            "infrastructure/api.ts",
            "presentation/ui.tsx",
        ] {
            self.write(
                &format!("crates/kenvoy/frontend/src/{entry}"),
                "export {};\n",
            )?;
        }
        Ok(())
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn source_layout_and_hard_line_limit_include_test_code() -> Result<()> {
    let fixture = Scratch::new()?;
    fixture.layout()?;
    layout::inspect(&fixture.0)?;
    fixture.write(
        "crates/kenvoy/tests/limit.rs",
        &"\n".repeat(layout::MAX_LINES),
    )?;
    layout::inspect(&fixture.0)?;
    fixture.write(
        "crates/kenvoy/tests/limit.rs",
        &"\n".repeat(layout::MAX_LINES + 1),
    )?;
    let error = layout::inspect(&fixture.0)
        .err()
        .ok_or("超限测试源码未被拒绝")?;
    assert!(error.to_string().contains("limit.rs"));
    fixture.write("crates/kenvoy/tests/limit.rs", "")?;
    fixture.write(
        "crates/kenvoy/src/infrastructure/artifacts/limit.rs",
        &"\n".repeat(layout::MAX_LINES + 1),
    )?;
    let error = layout::inspect(&fixture.0)
        .err()
        .ok_or("产品 artifacts 模块不能被当作构建产物忽略")?;
    assert!(error.to_string().contains("limit.rs"));
    fixture.write("crates/kenvoy/src/infrastructure/artifacts/limit.rs", "")?;
    fixture.write("crates/kenvoy/src/legacy.rs", "")?;
    let error = layout::inspect(&fixture.0)
        .err()
        .ok_or("旧平铺源码未被拒绝")?;
    assert!(error.to_string().contains("未约定条目"));
    Ok(())
}

#[test]
fn json_module_assets_resolve_without_bypassing_layers_or_counting_as_code() -> Result<()> {
    let fixture = Scratch::new()?;
    fixture.layout()?;
    let module = "crates/kenvoy/frontend/src/domain/messages.ts";
    let dictionary = "crates/kenvoy/frontend/src/domain/messages.json";
    fixture.write(
        module,
        "import messages from './messages.json' with { type: 'json' }; export default messages;",
    )?;
    fixture.write(
        dictionary,
        &format!(
            "{{\"hello\":\"world\"{}}}",
            "\n".repeat(layout::MAX_LINES + 1)
        ),
    )?;
    layout::inspect(&fixture.0)?;
    fs::remove_file(fixture.0.join(dictionary))?;
    let error = layout::inspect(&fixture.0)
        .err()
        .ok_or("缺失 JSON 模块未被拒绝")?;
    assert!(error.to_string().contains("前端相对依赖未解析"));
    fixture.write(
        "crates/kenvoy/frontend/src/presentation/messages.json",
        r#"{"hello":"world"}"#,
    )?;
    fixture.write(
        module,
        "import messages from '../presentation/messages.json' with { type: 'json' }; export default messages;",
    )?;
    let error = layout::inspect(&fixture.0)
        .err()
        .ok_or("JSON 资源绕过了分层约束")?;
    assert!(error.to_string().contains("前端层依赖越界"));
    Ok(())
}
