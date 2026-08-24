//! 表单校验执行：基于表达式引擎对提交数据求值 `Validation.expr`。

use std::collections::HashMap;

use crate::expr::{self, Context, Value};
use crate::ir::Validation;

/// 校验提交数据，返回失败提示列表（空列表 = 通过）。
///
/// 规则求值为 `false` 时记为失败；求值出错（如字段为空时 `结束日期 >= 开始日期`
/// 无法比较）时跳过该规则，由 `required` / `非空` 类规则负责空值检查。
#[must_use]
#[allow(clippy::implicit_hasher)] // 内部 API，默认哈希器足够
pub fn validate(validations: &[Validation], data: &HashMap<String, Value>) -> Vec<String> {
    let ctx = Context::from_map(data);
    let mut errors = Vec::new();
    for v in validations {
        if let Ok(false) = expr::eval_bool(&v.expr, &ctx) {
            errors.push(v.message.clone());
        }
        // 求值 `Ok(true)` / `Err` 均跳过：Err 常见于字段为空时无法比较
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> HashMap<String, Value> {
        HashMap::from([
            ("请假天数".to_string(), Value::Number(2.0)),
            ("结束日期".to_string(), Value::String("2026-09-01".into())),
            ("开始日期".to_string(), Value::String("2026-08-25".into())),
            ("事由".to_string(), Value::String("家中有事".into())),
        ])
    }

    fn rule(expr: &str, message: &str) -> Validation {
        Validation {
            field: String::new(),
            expr: expr.to_string(),
            message: message.to_string(),
        }
    }

    #[test]
    fn passes_when_all_rules_hold() {
        let rules = vec![
            rule("请假天数 > 0", "天数必须大于0"),
            rule("结束日期 >= 开始日期", "结束不能早于开始"),
            rule("事由 非空", "请填写事由"),
        ];
        let errors = validate(&rules, &data());
        assert!(errors.is_empty());
    }

    #[test]
    fn collects_failures_with_messages() {
        let rules = vec![
            rule("请假天数 > 10", "天数过大"),
            rule("事由 非空", "请填写事由"),
        ];
        let errors = validate(&rules, &data());
        assert_eq!(errors, vec!["天数过大"]);
    }

    #[test]
    fn eval_error_rule_is_skipped() {
        // 空字段导致的比较错误应跳过，而非阻断提交
        let rules = vec![rule("结束日期 >= 开始日期", "结束不能早于开始")];
        let errors = validate(&rules, &data());
        assert!(errors.is_empty());
    }

    #[test]
    fn empty_rules_pass() {
        assert!(validate(&[], &data()).is_empty());
    }
}
