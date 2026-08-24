//! 中间表示（`IR`）—— kengine 解析 XML 后的唯一对外产物。
//!
//! 契约见 `docs/01-ir-contract.md`。渲染器只读 `IR`，不读 XML。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// `IR` 版本号，渲染器必须校验兼容性。
pub const IR_VERSION: &str = "1.0";

/// 顶层 `IR`：一份 DSL 项目解析后的完整表示。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectIr {
    /// `IR` 版本号，见 [`IR_VERSION`]。
    pub version: String,
    /// 项目元信息。
    pub project: ProjectMeta,
    /// 全部数据模型，按模型名索引（有序，保证序列化输出确定）。
    #[serde(default)]
    pub models: BTreeMap<String, Model>,
    /// 全部布局，按布局路径索引（有序，保证序列化输出确定）。
    #[serde(default)]
    pub layouts: BTreeMap<String, Layout>,
    /// 全部页面。
    #[serde(default)]
    pub pages: Vec<Page>,
    /// 审批流（demo 占位，结构后续补充）。
    #[serde(default)]
    pub flows: Vec<Flow>,
    /// 权限（demo 占位，结构后续补充）。
    #[serde(default)]
    pub permissions: Permissions,
    /// 用户自定义组件（页面内 `<Component>` + `module/` 目录），按组件名索引。
    #[serde(default)]
    pub components: BTreeMap<String, ComponentDef>,
}

/// 项目元信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMeta {
    /// 项目名称。
    pub name: String,
    /// 项目描述。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// 数据模型。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    /// 模型名。
    pub name: String,
    /// 字段定义。
    #[serde(default)]
    pub fields: Vec<FieldDef>,
}

/// 数据模型字段定义。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldDef {
    /// 字段名。
    pub name: String,
    /// 字段类型。
    #[serde(rename = "type")]
    pub field_type: FieldType,
    /// 显示标签。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// 选项（select 等类型使用）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
    /// 是否必填。
    #[serde(default)]
    pub required: bool,
}

/// 字段类型（demo 子集）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    /// 短文本。
    Text,
    /// 长文本。
    Textarea,
    /// 数字。
    Number,
    /// 日期。
    Date,
    /// 单选。
    Select,
    /// 布尔。
    Boolean,
    /// 文件。
    File,
    /// 用户选择器。
    User,
    /// 部门选择器。
    Department,
}

/// 布局。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    /// 布局路径（如 `/请假管理`）。
    pub path: String,
    /// 布局标题。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// 布局根节点（含 `Slot` 占位）。
    pub root: Node,
}

/// 页面。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    /// 页面路径（如 `/请假管理/申请`）。
    pub path: String,
    /// 页面标题。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// 已解析的布局链（外层在前），由 kengine 在解析期计算。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layout_chain: Vec<String>,
    /// 页面内容节点树。
    pub content: Node,
    /// 显式引入的审批流名（索引到 `ProjectIr.flows`）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flows: Vec<String>,
    /// 页面权限配置（`publish.kenvoy.xml`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publish: Option<Publish>,
}

/// 用户自定义组件定义（页面内 `<Component>` 或 `module/` 目录文件）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentDef {
    /// 组件名（使用处标签名与之匹配）。
    pub name: String,
    /// 定义体根节点（渲染时实例 props 绑定 `{{名}}`，实例子元素注入 `<Slot/>`）。
    pub root: Node,
}

/// 组件树节点：内置组件为类型化变体，用户 DSL 组件走 [`Node::Custom`]。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "PascalCase")]
pub enum Node {
    /// 行布局容器。
    Row {
        #[serde(default)]
        props: RowProps,
        #[serde(default)]
        children: Vec<Node>,
    },
    /// 列布局容器。
    Col {
        #[serde(default)]
        props: ColProps,
        #[serde(default)]
        children: Vec<Node>,
    },
    /// 内容区块。
    Section {
        #[serde(default)]
        props: SectionProps,
        #[serde(default)]
        children: Vec<Node>,
    },
    /// 布局占位，渲染时被内层布局或页面内容替换。
    Slot,
    /// 表单，绑定数据模型。
    Form {
        #[serde(default)]
        props: FormProps,
        #[serde(default)]
        children: Vec<Node>,
    },
    /// 表单字段。
    Field {
        #[serde(default)]
        props: FieldProps,
    },
    /// 数据表格。
    Table {
        #[serde(default)]
        props: TableProps,
        #[serde(default)]
        children: Vec<Node>,
    },
    /// 表格列。
    Column {
        #[serde(default)]
        props: ColumnProps,
    },
    /// 统计卡。
    StatCard {
        #[serde(default)]
        props: StatCardProps,
    },
    /// 纯文本。
    Text {
        #[serde(default)]
        props: TextProps,
    },
    /// 用户 DSL 自定义组件。
    ///
    /// serde 内部标签（`type`）占用 JSON 的 `type` 字段，因此组件名放在
    /// `component` 字段中，JSON 形态：
    /// `{"type":"custom","component":"my-widget","props":{},"children":[]}`。
    #[serde(rename = "custom")]
    Custom {
        /// 用户 DSL 中定义的组件名。
        component: String,
        #[serde(default)]
        props: BTreeMap<String, Value>,
        #[serde(default)]
        children: Vec<Node>,
    },
}

/// 行布局属性（demo 占位，后续可扩展 gap / align 等）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RowProps {}

/// 列布局属性。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ColProps {
    /// 栅格宽度（1-12，默认 12 占满一行）。
    pub span: u8,
}

impl Default for ColProps {
    fn default() -> Self {
        Self { span: 12 }
    }
}

/// 内容区块属性。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SectionProps {
    /// 区块标题。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// 表单属性。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FormProps {
    /// 绑定的数据模型名。
    pub model: String,
    /// 表单级校验规则。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub validations: Vec<Validation>,
}

/// 表单字段属性。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FieldProps {
    /// 字段名（对应数据模型字段）。
    pub name: String,
    /// 显示标签，缺省用模型字段 label。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// 是否只读。
    pub readonly: bool,
    /// 是否必填。
    pub required: bool,
    /// 默认值，支持 `{{ }}` 绑定。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    /// 后缀单位（如「天」）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suffix: Option<String>,
    /// textarea 行数。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rows: Option<u32>,
    /// 选项（覆盖模型定义）。
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
    /// 校验规则。
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub validations: Vec<Validation>,
}

/// 校验规则（demo 阶段表达式为字符串约定）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Validation {
    /// 校验字段。
    pub field: String,
    /// 校验表达式。
    pub expr: String,
    /// 失败提示。
    pub message: String,
}

/// 表格属性。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TableProps {
    /// 数据源模型名。
    pub model: String,
    /// 筛选条件（字段名 → 值，支持 `{{ }}`）。
    #[serde(skip_serializing_if = "Map::is_empty")]
    pub filter: Map<String, Value>,
    /// 排序（字段名 → "asc" | "desc"）。
    #[serde(skip_serializing_if = "Map::is_empty")]
    pub sort: Map<String, Value>,
    /// 每页条数。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_size: Option<u32>,
}

/// 表格列属性。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ColumnProps {
    /// 字段名。
    pub field: String,
    /// 列标题。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// 列宽（px）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u16>,
}

/// 统计卡属性。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StatCardProps {
    /// 统计表达式（demo 为字符串约定，如 `统计(请假单.数量)`）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    /// 单位。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    /// 颜色。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// 纯文本属性。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TextProps {
    /// 文本内容。
    pub value: String,
}

/// 审批流（由页面 `<Import>` 显式引入）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Flow {
    /// 流程名。
    pub name: String,
    /// 触发入口（如 `请假单.提交申请`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger: Option<String>,
    /// 流程节点。
    #[serde(default)]
    pub nodes: Vec<FlowNode>,
}

/// 审批流节点。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowNode {
    /// 节点名。
    pub name: String,
    /// 审批人（用户/角色表达式，知会节点可为空）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approver: Option<String>,
    /// 节点类型。
    #[serde(default)]
    pub kind: FlowNodeKind,
    /// 通过分支动作。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub on_approve: Vec<FlowAction>,
    /// 驳回分支动作。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub on_reject: Vec<FlowAction>,
    /// 完成后动作（知会节点）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub on_complete: Vec<FlowAction>,
}

/// 审批节点类型。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowNodeKind {
    /// 普通审批节点。
    #[default]
    Normal,
    /// 知会节点。
    Notify,
}

/// 审批流动作。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FlowAction {
    /// 动作类型。
    pub kind: FlowActionKind,
    /// 条件表达式（`If` 分支）。
    pub expr: Option<String>,
    /// 目标节点（next / back）。
    pub target: Option<String>,
    /// 通知用户。
    pub users: Vec<String>,
    /// 更新模型名（update）。
    pub model: Option<String>,
    /// 更新字段（update）。
    pub field: Option<String>,
    /// 更新值 / 消息内容（`update` / `send_message`）。
    pub value: Option<String>,
    /// 消息接收人（`send_message`）。
    pub to: Option<String>,
    /// 条件分支内的子动作（if）。
    pub children: Vec<FlowAction>,
}

impl FlowAction {
    /// 创建指定类型的动作。
    #[must_use]
    pub fn new(kind: FlowActionKind) -> Self {
        Self {
            kind,
            expr: None,
            target: None,
            users: Vec::new(),
            model: None,
            field: None,
            value: None,
            to: None,
            children: Vec::new(),
        }
    }
}

impl Default for FlowAction {
    fn default() -> Self {
        Self::new(FlowActionKind::Next)
    }
}

/// 审批流动作类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowActionKind {
    /// 流转到下一节点。
    Next,
    /// 退回（目标节点或发起人）。
    Back,
    /// 发送通知。
    Notify,
    /// 更新模型字段。
    Update,
    /// 发送消息。
    SendMessage,
    /// 条件分支。
    If,
}

/// 页面权限配置（`publish.kenvoy.xml`）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Publish {
    /// 允许访问该页面的角色。
    pub allow_roles: Vec<String>,
    /// 字段级读写控制。
    pub fields: Vec<FieldAccess>,
    /// 角色定义（含继承与权限清单）。
    pub roles: Vec<Role>,
    /// 数据级行过滤规则（`<DataFilter>`，按模型）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub data_filters: Vec<DataFilterRule>,
}

/// 数据级行过滤规则：作用于指定模型的记录。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DataFilterRule {
    /// 目标模型名。
    pub model: String,
    /// 角色 → 过滤条件（多条命中其一）。
    pub filters: Vec<RoleFilter>,
}

/// 角色过滤条件。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RoleFilter {
    /// 适用角色（`role` 逗号分隔）。
    pub roles: Vec<String>,
    /// 全量可见（`<All/>`），此时忽略 `conditions`。
    pub all: bool,
    /// 比较条件（`<Eq/>` / `<Ne/>` / `<Gt/>` / `<Ge/>` / `<Lt/>` / `<Le/>`，AND 连接）。
    pub conditions: Vec<OpCondition>,
}

/// 相等条件；`value` 支持 `{{currentUser.*}}` 绑定。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OpCondition {
    /// 字段名。
    pub field: String,
    /// 比较操作符。
    pub op: FilterOp,
    /// 字段值（支持 `{{ }}` 绑定）。
    pub value: String,
}

/// 比较操作符（`DataFilter` 与字段值比较）。
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum FilterOp {
    /// 等于。
    #[default]
    Eq,
    /// 不等于。
    Ne,
    /// 大于。
    Gt,
    /// 大于等于。
    Ge,
    /// 小于。
    Lt,
    /// 小于等于。
    Le,
}

/// 字段访问控制。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FieldAccess {
    /// 字段名。
    pub field: String,
    /// 可读角色。
    pub read_roles: Vec<String>,
    /// 可写角色。
    pub write_roles: Vec<String>,
}

/// 角色定义。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Role {
    /// 角色名。
    pub name: String,
    /// 继承的角色名。
    pub extends: Option<String>,
    /// 权限清单。
    pub permissions: Vec<String>,
}

/// 权限（全局角色注册表，由各页面 `publish` 文件合并）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Permissions {
    /// 全部角色定义（按名去重）。
    pub roles: Vec<Role>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 序列化 → 反序列化 → 再序列化，两次 JSON 必须完全一致。
    fn round_trip_json<T: Serialize + for<'de> Deserialize<'de>>(value: &T) -> String {
        let json = serde_json::to_string(value).expect("序列化失败");
        let decoded: T = serde_json::from_str(&json).expect("反序列化失败");
        let json2 = serde_json::to_string(&decoded).expect("再次序列化失败");
        assert_eq!(json, json2, "round-trip 前后 JSON 不一致");
        json
    }

    #[test]
    fn field_type_serde() {
        assert_eq!(serde_json::to_string(&FieldType::User).unwrap(), "\"user\"");
        assert_eq!(
            serde_json::to_string(&FieldType::Select).unwrap(),
            "\"select\""
        );
        assert_eq!(
            serde_json::from_str::<FieldType>("\"textarea\"").unwrap(),
            FieldType::Textarea
        );
        assert!(serde_json::from_str::<FieldType>("\"unknown\"").is_err());
    }

    #[test]
    fn node_round_trip_all_variants() {
        let nodes = vec![
            Node::Row {
                props: RowProps::default(),
                children: vec![],
            },
            Node::Col {
                props: ColProps::default(),
                children: vec![],
            },
            Node::Section {
                props: SectionProps {
                    title: Some("区块".into()),
                },
                children: vec![],
            },
            Node::Slot,
            Node::Form {
                props: FormProps {
                    model: "请假单".into(),
                    validations: Vec::new(),
                },
                children: vec![],
            },
            Node::Field {
                props: FieldProps {
                    name: "申请人".into(),
                    ..FieldProps::default()
                },
            },
            Node::Table {
                props: TableProps {
                    model: "请假单".into(),
                    ..TableProps::default()
                },
                children: vec![],
            },
            Node::Column {
                props: ColumnProps {
                    field: "请假类型".into(),
                    ..ColumnProps::default()
                },
            },
            Node::StatCard {
                props: StatCardProps {
                    data: Some("统计(请假单.数量)".into()),
                    ..StatCardProps::default()
                },
            },
            Node::Text {
                props: TextProps {
                    value: "hello".into(),
                },
            },
            Node::Custom {
                component: "my-widget".into(),
                props: BTreeMap::new(),
                children: vec![],
            },
        ];
        for node in nodes {
            let _ = round_trip_json(&node);
        }
    }

    #[test]
    fn col_default_span_is_twelve() {
        let col: ColProps = serde_json::from_str("{}").unwrap();
        assert_eq!(col.span, 12);
    }

    #[test]
    fn node_missing_props_defaults() {
        // props 缺失时使用默认值
        let node: Node = serde_json::from_str(r#"{"type":"Field"}"#).unwrap();
        match node {
            Node::Field { props } => {
                assert_eq!(props.name, "");
                assert!(!props.readonly);
            }
            other => panic!("期望 Field，得到 {other:?}"),
        }
    }

    #[test]
    fn custom_node_tag_is_custom() {
        let json = serde_json::to_string(&Node::Custom {
            component: "my-widget".into(),
            props: BTreeMap::new(),
            children: vec![],
        })
        .unwrap();
        assert!(json.contains("\"type\":\"custom\""));
        assert!(json.contains("\"component\":\"my-widget\""));
    }

    #[test]
    fn full_project_ir_round_trip() {
        let ir = ProjectIr {
            version: IR_VERSION.to_string(),
            project: ProjectMeta {
                name: "请假管理".into(),
                description: Some("企业请假管理数字化平台".into()),
            },
            models: BTreeMap::from([(
                "请假单".to_string(),
                Model {
                    name: "请假单".into(),
                    fields: vec![
                        FieldDef {
                            name: "申请人".into(),
                            field_type: FieldType::User,
                            label: Some("申请人".into()),
                            options: vec![],
                            required: false,
                        },
                        FieldDef {
                            name: "请假类型".into(),
                            field_type: FieldType::Select,
                            label: Some("请假类型".into()),
                            options: vec![
                                "年假".into(),
                                "病假".into(),
                                "事假".into(),
                                "调休".into(),
                            ],
                            required: false,
                        },
                    ],
                },
            )]),
            layouts: BTreeMap::from([(
                "/".to_string(),
                Layout {
                    path: "/".into(),
                    title: Some("请假管理".into()),
                    root: Node::Slot,
                },
            )]),
            pages: vec![Page {
                path: "/请假管理/申请".into(),
                title: Some("填写请假单".into()),
                layout_chain: vec!["/".into(), "/请假管理".into()],
                content: Node::Form {
                    props: FormProps {
                        model: "请假单".into(),
                        validations: Vec::new(),
                    },
                    children: vec![Node::Field {
                        props: FieldProps {
                            name: "申请人".into(),
                            readonly: true,
                            ..FieldProps::default()
                        },
                    }],
                },
                flows: vec!["请假审批流".into()],
                publish: Some(Publish {
                    allow_roles: vec!["普通员工".into()],
                    fields: vec![FieldAccess {
                        field: "审批意见".into(),
                        read_roles: vec!["部门主管".into()],
                        write_roles: Vec::new(),
                    }],
                    roles: vec![Role {
                        name: "普通员工".into(),
                        extends: None,
                        permissions: vec!["请假申请.查看".into()],
                    }],
                    data_filters: vec![],
                }),
            }],
            flows: vec![],
            permissions: Permissions::default(),
            components: BTreeMap::from([(
                "状态卡".to_string(),
                ComponentDef {
                    name: "状态卡".into(),
                    root: Node::StatCard {
                        props: StatCardProps {
                            data: Some("{{data}}".into()),
                            unit: Some("次".into()),
                            color: None,
                        },
                    },
                },
            )]),
        };
        let _ = round_trip_json(&ir);
    }

    #[test]
    fn flow_and_publish_round_trip() {
        let mut next = FlowAction::new(FlowActionKind::Next);
        next.target = Some("人事备案".into());
        let mut back = FlowAction::new(FlowActionKind::Back);
        back.target = Some("发起人".into());
        let mut notify = FlowAction::new(FlowActionKind::Notify);
        notify.users = vec!["申请人".into()];
        let mut update = FlowAction::new(FlowActionKind::Update);
        update.model = Some("请假单".into());
        update.field = Some("审批状态".into());
        update.value = Some("已通过".into());
        let mut condition = FlowAction::new(FlowActionKind::If);
        condition.expr = Some("请假天数 <= 3".into());
        condition.children = vec![next.clone()];

        let flow = Flow {
            name: "请假审批流".into(),
            trigger: Some("请假单.提交申请".into()),
            nodes: vec![FlowNode {
                name: "部门主管审批".into(),
                approver: Some("部门主管".into()),
                kind: FlowNodeKind::Normal,
                on_approve: vec![condition, next],
                on_reject: vec![back],
                on_complete: vec![notify, update],
            }],
        };
        let _ = round_trip_json(&flow);

        let publish = Publish {
            allow_roles: vec!["普通员工".into(), "部门主管".into()],
            fields: vec![FieldAccess {
                field: "审批意见".into(),
                read_roles: vec!["部门主管".into()],
                write_roles: vec!["部门主管".into()],
            }],
            roles: vec![Role {
                name: "部门主管".into(),
                extends: Some("普通员工".into()),
                permissions: vec!["请假管理.审批.操作".into()],
            }],
            data_filters: vec![DataFilterRule {
                model: "请假单".into(),
                filters: vec![RoleFilter {
                    roles: vec!["普通员工".into()],
                    all: false,
                    conditions: vec![OpCondition {
                        field: "申请人".into(),
                        op: FilterOp::Eq,
                        value: "{{currentUser.id}}".into(),
                    }],
                }],
            }],
        };
        let _ = round_trip_json(&publish);
    }
}
