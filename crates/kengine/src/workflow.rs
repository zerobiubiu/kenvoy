//! 审批流状态机执行：基于表达式引擎求值分支条件，推进 / 退回 / 更新 / 通知。
//!
//! 纯函数、无 I/O：`step` 返回下一节点与副作用，由上层（CLI / 云平台）落地通知、消息等。

use std::collections::HashMap;
use std::fmt;

use crate::expr::{self, Context, Value};
use crate::ir::{Flow, FlowAction, FlowActionKind, FlowNodeKind};

/// 审批决策。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// 通过。
    Approve,
    /// 驳回。
    Reject,
}

/// 执行产生的副作用（供上层落地）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Effects {
    /// 模型字段更新（字段名, 值）。
    pub updates: Vec<(String, String)>,
    /// 通知用户列表（每次 `<Notify>` 一条）。
    pub notifications: Vec<Vec<String>>,
    /// 消息（接收人, 内容）。
    pub messages: Vec<(String, String)>,
}

/// 单步执行结果。
#[derive(Debug, Clone, PartialEq)]
pub struct StepOutcome {
    /// 下一节点名；`None` = 流程结束。
    pub next: Option<String>,
    /// 副作用。
    pub effects: Effects,
}

/// 工作流错误。
#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowError {
    /// 错误信息。
    pub message: String,
}

impl WorkflowError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for WorkflowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for WorkflowError {}

/// 流程起始节点名；空流程返回 `None`。
#[must_use]
pub fn start_node(flow: &Flow) -> Option<String> {
    flow.nodes.first().map(|n| n.name.clone())
}

/// 执行当前节点的一个决策步。
///
/// `ctx` 需包含字段值与系统变量（`currentUser` 等）；`data` 会被 `<Update>` 就地修改。
///
/// # Errors
/// 节点不存在或条件求值失败时返回 [`WorkflowError`]。
#[allow(clippy::implicit_hasher)] // 内部 API，默认哈希器足够
pub fn step(
    flow: &Flow,
    current: &str,
    decision: Decision,
    data: &mut HashMap<String, Value>,
    ctx: &Context,
) -> Result<StepOutcome, WorkflowError> {
    let node = flow
        .nodes
        .iter()
        .find(|n| n.name == current)
        .ok_or_else(|| WorkflowError::new(format!("节点 `{current}` 不存在")))?;
    let actions = if node.kind == FlowNodeKind::Notify {
        // 知会节点无审批决策，执行完成后动作
        &node.on_complete
    } else {
        match decision {
            Decision::Approve => &node.on_approve,
            Decision::Reject => &node.on_reject,
        }
    };
    let mut effects = Effects::default();
    let next = run_actions(actions, data, ctx, &mut effects)?;
    Ok(StepOutcome { next, effects })
}

fn run_actions(
    actions: &[FlowAction],
    data: &mut HashMap<String, Value>,
    ctx: &Context,
    effects: &mut Effects,
) -> Result<Option<String>, WorkflowError> {
    let mut next: Option<String> = None;
    for action in actions {
        match action.kind {
            FlowActionKind::If => {
                let expr = action.expr.as_deref().unwrap_or_default();
                let cond = expr::eval_bool(expr, ctx)
                    .map_err(|e| WorkflowError::new(format!("条件 `{expr}` 求值失败：{e}")))?;
                if cond && let Some(n) = run_actions(&action.children, data, ctx, effects)? {
                    next = Some(n);
                }
            }
            FlowActionKind::Next => {
                if let Some(t) = &action.target {
                    next = Some(t.clone());
                }
            }
            FlowActionKind::Back => {
                // 退回发起人 → 流程结束；退回具体节点 → 该节点
                next = action.target.clone().filter(|t| t != "发起人");
            }
            FlowActionKind::Notify => {
                if !action.users.is_empty() {
                    effects.notifications.push(action.users.clone());
                }
            }
            FlowActionKind::Update => {
                if let (Some(field), Some(value)) = (&action.field, &action.value) {
                    effects.updates.push((field.clone(), value.clone()));
                    data.insert(field.clone(), Value::String(value.clone()));
                }
            }
            FlowActionKind::SendMessage => {
                if let Some(to) = &action.to {
                    let content = action.value.clone().unwrap_or_default();
                    effects.messages.push((to.clone(), content));
                }
            }
        }
    }
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{FlowAction, FlowNode};

    fn action_next(target: &str) -> FlowAction {
        let mut a = FlowAction::new(FlowActionKind::Next);
        a.target = Some(target.to_string());
        a
    }

    fn action_if(expr: &str, children: Vec<FlowAction>) -> FlowAction {
        let mut a = FlowAction::new(FlowActionKind::If);
        a.expr = Some(expr.to_string());
        a.children = children;
        a
    }

    fn action_back(target: &str) -> FlowAction {
        let mut a = FlowAction::new(FlowActionKind::Back);
        a.target = Some(target.to_string());
        a
    }

    fn action_notify(users: &[&str]) -> FlowAction {
        let mut a = FlowAction::new(FlowActionKind::Notify);
        a.users = users.iter().map(ToString::to_string).collect();
        a
    }

    fn action_update(field: &str, value: &str) -> FlowAction {
        let mut a = FlowAction::new(FlowActionKind::Update);
        a.model = Some("请假单".to_string());
        a.field = Some(field.to_string());
        a.value = Some(value.to_string());
        a
    }

    fn action_send(to: &str, content: &str) -> FlowAction {
        let mut a = FlowAction::new(FlowActionKind::SendMessage);
        a.to = Some(to.to_string());
        a.value = Some(content.to_string());
        a
    }

    fn build_flow() -> Flow {
        Flow {
            name: "请假审批流".to_string(),
            trigger: Some("请假单.提交申请".to_string()),
            nodes: vec![
                FlowNode {
                    name: "部门主管审批".to_string(),
                    approver: Some("部门主管".to_string()),
                    kind: FlowNodeKind::Normal,
                    on_approve: vec![
                        action_if("请假天数 <= 3", vec![action_next("人事备案")]),
                        action_if("请假天数 > 3", vec![action_next("人事总监审批")]),
                        action_notify(&["申请人"]),
                    ],
                    on_reject: vec![action_back("发起人"), action_notify(&["申请人"])],
                    on_complete: vec![],
                },
                FlowNode {
                    name: "人事总监审批".to_string(),
                    approver: Some("角色.人事总监".to_string()),
                    kind: FlowNodeKind::Normal,
                    on_approve: vec![
                        action_next("人事备案"),
                        action_notify(&["申请人", "部门主管"]),
                    ],
                    on_reject: vec![action_back("部门主管审批")],
                    on_complete: vec![],
                },
                FlowNode {
                    name: "人事备案".to_string(),
                    approver: Some("角色.HR".to_string()),
                    kind: FlowNodeKind::Notify,
                    on_approve: vec![],
                    on_reject: vec![],
                    on_complete: vec![
                        action_update("审批状态", "已通过"),
                        action_send("申请人", "您的请假申请已通过"),
                    ],
                },
            ],
        }
    }

    fn ctx_with_data(data: &HashMap<String, Value>) -> Context {
        Context::from_map(data)
    }

    #[test]
    fn short_leave_goes_through_directly() {
        let flow = build_flow();
        assert_eq!(start_node(&flow).as_deref(), Some("部门主管审批"));

        let mut data = HashMap::from([("请假天数".to_string(), Value::Number(2.0))]);
        let ctx = ctx_with_data(&data);

        let out = step(&flow, "部门主管审批", Decision::Approve, &mut data, &ctx).unwrap();
        assert_eq!(out.next.as_deref(), Some("人事备案"));
        assert_eq!(out.effects.notifications, vec![vec!["申请人".to_string()]]);

        // 知会节点直接完成
        let out = step(&flow, "人事备案", Decision::Approve, &mut data, &ctx).unwrap();
        assert_eq!(out.next, None);
        assert_eq!(
            out.effects.updates,
            vec![("审批状态".to_string(), "已通过".to_string())]
        );
        assert_eq!(
            out.effects.messages,
            vec![("申请人".to_string(), "您的请假申请已通过".to_string())]
        );
        assert_eq!(
            data.get("审批状态"),
            Some(&Value::String("已通过".to_string()))
        );
    }

    #[test]
    fn long_leave_goes_through_manager() {
        let flow = build_flow();
        let mut data = HashMap::from([("请假天数".to_string(), Value::Number(5.0))]);
        let ctx = ctx_with_data(&data);

        let out = step(&flow, "部门主管审批", Decision::Approve, &mut data, &ctx).unwrap();
        assert_eq!(out.next.as_deref(), Some("人事总监审批"));

        let out = step(&flow, "人事总监审批", Decision::Approve, &mut data, &ctx).unwrap();
        assert_eq!(out.next.as_deref(), Some("人事备案"));
    }

    #[test]
    fn reject_back_to_initiator_ends_flow() {
        let flow = build_flow();
        let mut data = HashMap::from([("请假天数".to_string(), Value::Number(2.0))]);
        let ctx = ctx_with_data(&data);

        let out = step(&flow, "部门主管审批", Decision::Reject, &mut data, &ctx).unwrap();
        assert_eq!(out.next, None); // 退回发起人 → 结束
        assert_eq!(out.effects.notifications, vec![vec!["申请人".to_string()]]);
    }

    #[test]
    fn unknown_node_errors() {
        let flow = build_flow();
        let mut data = HashMap::new();
        let ctx = Context::new();
        assert!(step(&flow, "不存在的节点", Decision::Approve, &mut data, &ctx).is_err());
    }
}
