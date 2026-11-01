use crate::domain::model::{ModuleReferenceKind, NavigationItem, Role};

use super::{Fixture, TestResult};

#[test]
fn actions_are_explicit_and_navigation_inherits_target_permission() -> TestResult {
    let fixture = Fixture::new(
        r#"import {calculateDiscount} from './discount.server'; export default <app name="动作"><action name="calculateDiscount" handler={calculateDiscount} runtime="server" permission="admin"/><page path="/private" permission="admin"><group label="内容"/></page><navigation label="导航"><group label="管理"><item label="私有" path="/private"/></group></navigation></app>;"#,
        &[(
            "discount.server.ts",
            "export function calculateDiscount(amount: number) { return amount >= 10000 ? 0.8 : 0.9; }",
        )],
    )?;
    let application = fixture.valid()?;
    assert_eq!(application.actions[0].permission, Role::Admin);
    assert_eq!(
        application.module_references[0].kind,
        ModuleReferenceKind::ServerFunction
    );
    let NavigationItem::Group { items, .. } = &application.navigation[0].items[0] else {
        return Err("导航组缺失".into());
    };
    assert!(matches!(
        items[0],
        NavigationItem::Link {
            permission: Role::Admin,
            ..
        }
    ));
    let arrow = Fixture::new(r#"import {compute} from './compute'; export default <app name="箭头"><action name="compute" handler={compute} runtime="server" permission="user"/></app>;"#,
        &[("compute.ts", "export const compute = (value: number) => value * 2;")])?.valid()?;
    assert_eq!(arrow.actions.len(), 1);
    Ok(())
}

#[test]
fn client_reexports_and_dynamic_import_cannot_reach_server_modules() -> TestResult {
    for body in [
        "import {secret} from './secret.server'; export function Card(){return secret;}",
        "export {secret as Card} from './secret.server';",
        "export function Card(){return import /* hidden */ ('./secret.server');}",
    ] {
        let fixture = Fixture::new(
            "import {Card} from './card'; export default <page path='/view'><Card/></page>;",
            &[
                ("card.tsx", body),
                (
                    "secret.server.ts",
                    "export function secret(){return 'PRIVATE';}",
                ),
            ],
        )?;
        let result = fixture.analyze()?;
        assert!(result.application.is_none());
        assert!(
            result
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "KV2101"),
            "{:?}",
            result.diagnostics
        );
    }
    let safe = Fixture::new("import {Card} from './card'; export default <page path='/view'><Card/></page>;", &[("card.tsx", "import type {Secret} from './secret.server'; export function Card(){const value: Secret | null = null; return value;}"), ("secret.server.ts", "export interface Secret { value: string }")])?.valid()?;
    assert_eq!(safe.module_references.len(), 1);
    Ok(())
}

#[test]
fn server_private_dependency_closure_requires_explicit_shared_boundaries() -> TestResult {
    let app = "import {Card} from './card'; import {execute} from './logic.server'; export default <app name='隔离'><action name='execute' handler={execute} runtime='server' permission='admin'/><page path='/view'><Card/></page></app>;";
    let leaked = Fixture::new(
        app,
        &[
            (
                "logic.server.ts",
                "import {secret} from './helper'; export function execute(){return secret;}",
            ),
            ("helper.ts", "export const secret = 'PRIVATE';"),
            (
                "card.tsx",
                "import {secret} from './helper'; export function Card(){return secret;}",
            ),
        ],
    )?
    .analyze()?;
    assert!(leaked.application.is_none());
    assert!(
        leaked
            .diagnostics
            .iter()
            .any(|error| error.code == "KV2101")
    );
    let shared = Fixture::new(app, &[("logic.server.ts", "import {format} from './helper.shared'; export function execute(){return format(1);}"), ("helper.shared.ts", "export const format = (value: number) => String(value);"), ("card.tsx", "import {format} from './helper.shared'; export function Card(){return format(2);}")])?.valid()?;
    assert_eq!(shared.actions.len(), 1);
    Ok(())
}

#[test]
fn bundler_globs_and_aliased_require_are_not_unchecked_module_edges() -> TestResult {
    for body in [
        "export function Card(){return import.meta.glob('./*.server.ts', {eager:true});}",
        "const load = require; export function Card(){return load('./secret.server');}",
    ] {
        let result = Fixture::new(
            "import {Card} from './card'; export default <page path='/view'><Card/></page>;",
            &[("card.tsx", body)],
        )?
        .analyze()?;
        assert!(result.application.is_none());
        assert!(
            result
                .diagnostics
                .iter()
                .any(|error| error.code == "KV2101"),
            "{:?}",
            result.diagnostics
        );
    }
    Ok(())
}
