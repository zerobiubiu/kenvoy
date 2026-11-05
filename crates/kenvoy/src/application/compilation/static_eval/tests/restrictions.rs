use super::super::{EvalLimits, Evaluator};
use super::{Project, SwcClientCodeTransformer, TestResult, evaluator};

#[test]
fn constant_cycles_and_schema_cycles_report_their_locations() -> TestResult {
    let project = Project::new(&[("app.tsx", "const a=b, b=a; export default <app name={a}/>;")])?;
    let program = project.program()?;
    assert_eq!(
        evaluator(&program)
            .entry(&project.entry()?)
            .expect_err("常量应循环")
            .code,
        "KV1102"
    );
    let project = Project::new(&[
        (
            "app.tsx",
            "import A from './A.diu'; export default <app><resource name='items'><A/></resource></app>;",
        ),
        ("A.diu.tsx", "import B from './B.diu'; export default <B/>;"),
        ("B.diu.tsx", "import A from './A.diu'; export default <A/>;"),
    ])?;
    let program = project.program()?;
    let error = evaluator(&program)
        .entry(&project.entry()?)
        .expect_err("Schema 应循环");
    assert_eq!(error.code, "KV1102");
    assert!(!error.related.is_empty());
    Ok(())
}

#[test]
fn dynamic_schema_mutation_getters_array_holes_and_calls_are_rejected() -> TestResult {
    for source in [
        "const config={name:'first'}; const alias=config; alias.name='changed'; export default <app name={config.name}/>;",
        "const values=[1]; values.push(2); export default <app name='test'/>;",
        "const config={get name(){return 'hidden'}}; export default <app name={config.name}/>;",
        "const values=[1,,3]; export default <app name={values[0]}/>;",
        "const field=()=> <text name='x'/>; export default <app>{field()}</app>;",
        "let name='mutable'; export default <app name={name}/>;",
        "export default <app name={1/0}/>;",
    ] {
        let project = Project::new(&[("app.tsx", source)])?;
        let program = project.program()?;
        assert_eq!(
            evaluator(&program)
                .entry(&project.entry()?)
                .expect_err("应拒绝动态或非法 Schema")
                .code,
            "KV1101",
            "{source}"
        );
    }
    Ok(())
}

#[test]
fn schema_arguments_are_rejected_instead_of_ignored() -> TestResult {
    let project = Project::new(&[
        (
            "app.tsx",
            "import Fields from './Fields.diu'; export default <app><Fields required/></app>;",
        ),
        ("Fields.diu.tsx", "export default <text name='x'/>;"),
    ])?;
    let program = project.program()?;
    assert_eq!(
        evaluator(&program)
            .entry(&project.entry()?)
            .expect_err("不应忽略参数")
            .code,
        "KV1106"
    );
    Ok(())
}

#[test]
fn recursion_node_and_collection_budgets_are_enforced() -> TestResult {
    for limits in [
        EvalLimits {
            depth: 2,
            ..Default::default()
        },
        EvalLimits {
            nodes: 2,
            ..Default::default()
        },
        EvalLimits {
            steps: 2,
            ..Default::default()
        },
    ] {
        let project = Project::new(&[(
            "app.tsx",
            "export default <app><page path='/test'><group><text name='x'/></group></page></app>;",
        )])?;
        let program = project.program()?;
        assert_eq!(
            Evaluator::with_limits(&program, &SwcClientCodeTransformer, limits)
                .entry(&project.entry()?)
                .expect_err("预算应生效")
                .code,
            "KV1103"
        );
    }
    let project = Project::new(&[(
        "app.tsx",
        "const values=[1,2,3]; export default <app name={values[0]}/>;",
    )])?;
    let program = project.program()?;
    assert_eq!(
        Evaluator::with_limits(
            &program,
            &SwcClientCodeTransformer,
            EvalLimits {
                collection: 2,
                ..Default::default()
            }
        )
        .entry(&project.entry()?)
        .expect_err("集合应受限")
        .code,
        "KV1103"
    );
    Ok(())
}
