use super::super::{EvalLimits, Evaluator};
use super::{Project, SwcClientCodeTransformer, TestResult, evaluator};

#[test]
fn nested_functions_control_flow_hooks_and_runtime_statements_are_rejected() -> TestResult {
    for source in [
        "export default function App(){function Inner(){return <app name='嵌套'/>;}}",
        "export default function App(){function Inner(){return <app name='嵌套'/>;} return <app name='外层'/>;}",
        "export default function App(){const inner=()=> <app name='嵌套'/>; return inner;}",
        "export default function App(){return ()=> <app name='嵌套'/>;}",
        "export default function App(){if(true){return <app name='条件'/>;} return <app name='其他'/>;}",
        "export default function App(){for(const item of []){} return <app name='循环'/>;}",
        "export default function App(){throw new Error('不能执行'); return <app name='错误'/>;}",
        "export default function App(){let name='可变'; return <app name={name}/>;}",
        "export default function App(){const [value]=useState(0); return <app name={value}/>;}",
        "export default function App(){const unused=useThing(); return <app name='不能漏掉调用'/>;}",
        "export default function App(){sideEffect(); return <app name='副作用'/>;}",
        "export default function App(){const config={name:'原值'};config.name='修改';return <app name={config.name}/>;}",
        "export default function App(){const values=[];return <app name='运行时'>{values.map(()=> <text name='x'/>)}</app>;}",
        "export default function App(){return createApp();}",
        "export default function App(){const dynamic=Boolean(1);return dynamic?<app name='动态'/>:<app name='其他'/>;}",
        "export default function App(){return <app name='运行时'>{createFields()}</app>;}",
        "export default function App(){return <app name='直接返回'/>;const unused='不能忽略';}",
        "export default function App(){const name=later;const later='不能越过声明顺序';return <app name={name}/>;}",
        "export default function App(){const config={get name(){return '动态'}};return <app name={config.name}/>;}",
        "export default function App(props){return <app name={props.name}/>;}",
        "export default function App(props=sideEffect()){return <app name='参数副作用'/>;}",
        "export default async function App(){return <app name='异步'/>;}",
        "export default function* App(){return <app name='生成器'/>;}",
        "export default async ()=> <app name='异步'/>;",
        "globalThis.changed=true;export default ()=> <app name='顶层副作用'/>;",
    ] {
        let project = Project::new(&[("app.tsx", source)])?;
        let program = project.program()?;
        let error = evaluator(&program)
            .entry(&project.entry()?)
            .expect_err("不能执行函数或从嵌套 JSX 推断 Schema");
        assert_eq!(error.code, "KV1101", "{source}");
        assert!(!error.suggestion.is_empty());
        assert!(error.source.path.ends_with("app.tsx"));
    }
    Ok(())
}

#[test]
fn aliased_function_root_still_rejects_top_level_side_effects_in_its_owner() -> TestResult {
    let project = Project::new(&[
        ("app.tsx", "export {Template as default} from './template';"),
        (
            "template.tsx",
            "console.log('不能执行'); export function Template(){return <app name='副作用'/>;}",
        ),
    ])?;
    let program = project.program()?;
    let error = evaluator(&program)
        .entry(&project.entry()?)
        .expect_err("转导不能绕过函数所属模块的副作用检查");
    assert_eq!(error.code, "KV1101");
    assert!(error.source.path.ends_with("template.tsx"));
    Ok(())
}

#[test]
fn template_return_values_and_repeated_local_jsx_remain_budgeted() -> TestResult {
    for limits in [
        EvalLimits {
            depth: 2,
            ..Default::default()
        },
        EvalLimits {
            nodes: 3,
            ..Default::default()
        },
        EvalLimits {
            steps: 3,
            ..Default::default()
        },
        EvalLimits {
            collection: 1,
            ..Default::default()
        },
    ] {
        let project = Project::new(&[(
            "app.tsx",
            "export default function App(){const field=<text name='x'/>;const name='模板';return <app name={name}>{field}{field}{field}</app>;}",
        )])?;
        let program = project.program()?;
        let error = Evaluator::with_limits(&program, &SwcClientCodeTransformer, limits)
            .entry(&project.entry()?)
            .expect_err("模板与局部 JSX 复用不能绕过求值预算");
        assert_eq!(error.code, "KV1103");
    }
    Ok(())
}

#[test]
fn root_function_alias_cycles_use_the_existing_cycle_diagnostic() -> TestResult {
    let project = Project::new(&[(
        "app.tsx",
        "const First=Second,Second=First;export default First;",
    )])?;
    let program = project.program()?;
    let error = evaluator(&program)
        .entry(&project.entry()?)
        .expect_err("入口别名循环必须诊断");
    assert_eq!(error.code, "KV1102");
    Ok(())
}

#[test]
fn composing_cached_local_jsx_cannot_flatten_the_depth_budget() -> TestResult {
    let project = Project::new(&[(
        "app.tsx",
        "export default function App(){const first=<group/>;const second=<group>{first}</group>;const third=<group>{second}</group>;const fourth=<group>{third}</group>;return <app name='深度'>{fourth}</app>;}",
    )])?;
    let program = project.program()?;
    let error = Evaluator::with_limits(
        &program,
        &SwcClientCodeTransformer,
        EvalLimits {
            depth: 4,
            ..Default::default()
        },
    )
    .entry(&project.entry()?)
    .expect_err("缓存 JSX 仍须计算组合后的真实嵌套");
    assert_eq!(error.code, "KV1103");
    Ok(())
}
