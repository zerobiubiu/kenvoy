//! 源码翻译预检：只分析已加载源码与词典，不执行 JavaScript，也不访问业务数据。
mod bindings;
mod links;
mod plural;
mod source;
mod values;

#[cfg(test)]
mod tests;

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::LazyLock,
};

use kengine::ProgramIR;
use swc_ecma_visit::VisitWith;

use crate::domain::{
    i18n::I18nBundle,
    source::{DslDiagnostic, DslDiagnosticCategory, SourceLocation},
};

use super::parser;
use bindings::{Expression, Origin, Resolver};
use source::{Call, Source};

/// 构建和启动前检查全部已分析模块；词典配置校验仍由既有 i18n 装载入口负责。
pub fn validate_i18n(program: &ProgramIR, bundle: &I18nBundle) -> Result<(), Vec<DslDiagnostic>> {
    let mut sources = Vec::with_capacity(program.modules.len());
    let mut diagnostics = Vec::new();
    for module in &program.modules {
        let name = module.path.to_string_lossy();
        let extension = module.path.extension().and_then(|value| value.to_str());
        let typescript = matches!(extension, Some("ts" | "tsx" | "mts" | "cts"));
        let jsx = matches!(extension, Some("jsx" | "tsx"));
        match parser::parse(&module.source, &name, typescript, jsx) {
            Ok((_, parsed, base)) => {
                let mut source = Source::new(module, base);
                parsed.visit_with(&mut source);
                sources.push(source);
            }
            Err(error) => {
                diagnostics.push(DslDiagnostic::new(
                    "KV2203",
                    error
                        .source
                        .map(|source| *source)
                        .unwrap_or_else(|| parser::location(&name, &module.source, 0, 0)),
                    format!("无法分析源码翻译调用：{}", error.message),
                    "修正模块语法或源码规模后重新构建；翻译检查不会执行项目代码",
                ));
            }
        }
    }
    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }
    let mut resolver = Resolver::new(program, &sources);
    for (module, source) in sources.iter().enumerate() {
        for call in source.calls.values() {
            inspect_call(&mut resolver, bundle, module, call, &mut diagnostics);
        }
        for node in &source.module.expressions {
            let expression = Expression {
                module,
                id: node.id,
            };
            if resolver.origin(expression).capability() && !source.permitted_use(node.id) {
                diagnostics.push(unresolved(
                    &resolver,
                    expression,
                    "翻译能力进入未支持的调用、返回值或容器，无法证明后续调用的完整键集合",
                ));
            }
        }
    }
    diagnostics.sort_by(|left, right| {
        (
            &left.source.path,
            left.source.span.start,
            &left.code,
            &left.message,
        )
            .cmp(&(
                &right.source.path,
                right.source.span.start,
                &right.code,
                &right.message,
            ))
    });
    diagnostics.dedup_by(|left, right| {
        left.source == right.source && left.code == right.code && left.message == right.message
    });
    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(diagnostics)
    }
}

fn inspect_call(
    resolver: &mut Resolver<'_>,
    bundle: &I18nBundle,
    module: usize,
    call: &Call,
    diagnostics: &mut Vec<DslDiagnostic>,
) {
    let expression = Expression {
        module,
        id: call.expression,
    };
    let origin = call.callee.map_or(Origin::Other, |id| {
        resolver.origin(Expression { module, id })
    });
    if origin == Origin::Unresolved {
        diagnostics.push(unresolved(
            resolver,
            expression,
            "无法静态确定翻译调用来源或成员名称",
        ));
    }
    if origin == Origin::Translate {
        let key = call
            .arguments
            .first()
            .filter(|argument| !argument.spread)
            .and_then(|argument| argument.expression)
            .map(|id| Expression { module, id });
        let Some((key, keys)) = key.and_then(|key| resolver.strings(key).map(|keys| (key, keys)))
        else {
            diagnostics.push(unresolved(
                resolver,
                key.unwrap_or(expression),
                "无法静态确定翻译键的完整集合",
            ));
            return;
        };
        let plural = call
            .arguments
            .get(1)
            .filter(|argument| !argument.spread)
            .and_then(|argument| argument.expression)
            .is_some_and(|id| resolver.plural(Expression { module, id }));
        let system = match system_keys() {
            Ok(keys) => keys,
            Err(error) => {
                diagnostics.push(diagnostic(
                    resolver,
                    expression,
                    "KV2203",
                    error.clone(),
                    "修复框架内置系统词典后重新构建",
                ));
                return;
            }
        };
        for name in keys {
            let other = plural.then(|| format!("{name}.other"));
            if system.contains(&name) || other.as_ref().is_some_and(|key| system.contains(key)) {
                continue;
            }
            let missing: Vec<_> = bundle
                .languages
                .iter()
                .filter(|language| {
                    !bundle.messages.get(&language.code).is_some_and(|messages| {
                        messages.contains_key(&name)
                            || other.as_ref().is_some_and(|key| messages.contains_key(key))
                    })
                })
                .map(|language| language.code.as_str())
                .collect();
            if !missing.is_empty() {
                diagnostics.push(diagnostic(resolver, key, "KV2201",
                    format!("翻译键 {name:?} 未配置完整词条（缺少语言：{}）", missing.join("、")),
                    &format!("在 translations.json 为全部已配置语言补齐 {name:?}{}；第三个 fallback 参数不能代替词典配置",
                        if plural { " 或对应的 .other 词条" } else { " 的原始键词条（未传 count 时不会读取 .other）" })));
            }
        }
    }
}

fn unresolved(resolver: &Resolver<'_>, expression: Expression, message: &str) -> DslDiagnostic {
    diagnostic(
        resolver,
        expression,
        "KV2202",
        message.into(),
        "直接调用 useI18n() 的 t，使用字符串、可静态确定的 const 或有限条件键；在 translations.json 中为全部已配置语言补齐所有可能词条",
    )
}

fn diagnostic(
    resolver: &Resolver<'_>,
    expression: Expression,
    code: &str,
    message: String,
    suggestion: &str,
) -> DslDiagnostic {
    let module = resolver.sources[expression.module].module;
    let mut diagnostic = DslDiagnostic::new(
        code,
        SourceLocation {
            path: module.path.clone(),
            span: module.expressions[expression.id.0].span.clone(),
        },
        message,
        suggestion,
    );
    diagnostic.category = if code == "KV2201" {
        DslDiagnosticCategory::Semantic
    } else {
        DslDiagnosticCategory::StaticEvaluation
    };
    diagnostic
}

fn system_keys() -> &'static Result<BTreeSet<String>, String> {
    static KEYS: LazyLock<Result<BTreeSet<String>, String>> = LazyLock::new(|| {
        // 与 frontend/domain/i18n/messages.ts 相同：每种系统语言合并 UI 与错误词典。
        let dictionaries = [
            [
                include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/frontend/src/domain/i18n/ui.zh-CN.json"
                )),
                include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/frontend/src/domain/i18n/errors.zh-CN.json"
                )),
            ],
            [
                include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/frontend/src/domain/i18n/ui.en-US.json"
                )),
                include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/frontend/src/domain/i18n/errors.en-US.json"
                )),
            ],
        ];
        let mut languages = Vec::new();
        for dictionaries in dictionaries {
            let mut keys = BTreeSet::new();
            for dictionary in dictionaries {
                let messages: BTreeMap<String, String> = serde_json::from_str(dictionary)
                    .map_err(|error| format!("框架内置系统词典无效：{error}"))?;
                keys.extend(messages.into_keys());
            }
            languages.push(keys);
        }
        Ok(languages[0].intersection(&languages[1]).cloned().collect())
    });
    &KEYS
}
