use std::{collections::BTreeMap, path::Path};

use kengine::{AttributeKind, AttributeValueIR, ElementIR, ExpressionId, NodeId, NodeKind};
use serde_json::Value;

use crate::domain::source::{DslDiagnostic, SourceLocation};

use super::{
    Evaluated, Evaluator, ExpressionReference, StaticElement, StaticNode, StaticValue,
    evaluator::{ModuleSpan, empty_span, location},
    values::primitive_text,
};

impl Evaluator<'_> {
    pub(super) fn expand_node(
        &mut self,
        path: &Path,
        id: NodeId,
    ) -> Result<Vec<StaticNode>, DslDiagnostic> {
        if self.node_depth >= self.limits.depth {
            let module = self.module(path)?;
            let span = module
                .node(id)
                .map(|node| node.span.clone())
                .unwrap_or_else(empty_span);
            return Err(self.error(
                "KV1103",
                location(module, span),
                "JSX 嵌套超过深度限制",
                "减少嵌套层级",
            ));
        }
        self.node_depth += 1;
        let result = self.expand_node_inner(path, id);
        self.node_depth -= 1;
        result
    }

    fn expand_node_inner(
        &mut self,
        path: &Path,
        id: NodeId,
    ) -> Result<Vec<StaticNode>, DslDiagnostic> {
        let module = self.module(path)?;
        let node = module.node(id).cloned().ok_or_else(|| {
            self.error(
                "KV1104",
                location(module, module.source_span()),
                "JSX 节点引用无效",
                "重新分析项目",
            )
        })?;
        let source = location(module, node.span.clone());
        self.tick(&source)?;
        self.nodes += 1;
        if self.nodes > self.limits.nodes {
            return Err(self.error(
                "KV1103",
                source,
                "声明展开节点超过限制",
                "减少嵌套与重复的 Schema 组合",
            ));
        }
        match node.kind {
            NodeKind::Fragment { children } => self.expand_children(path, &children),
            NodeKind::Text { value_utf16 } => {
                let value = String::from_utf16(&value_utf16).map_err(|_| {
                    self.error(
                        "KV1101",
                        source.clone(),
                        "JSX 文本包含无效 Unicode",
                        "修复文本编码",
                    )
                })?;
                if value.trim().is_empty() {
                    Ok(vec![])
                } else {
                    Ok(vec![StaticNode::Text { value, source }])
                }
            }
            NodeKind::Expression { expression: None } => Ok(vec![]),
            NodeKind::Expression {
                expression: Some(expression),
            } => match self.eval_expr(path, expression)? {
                Evaluated::Nodes(nodes) => Ok(nodes),
                Evaluated::Json(Value::Null | Value::Bool(_)) => Ok(vec![]),
                Evaluated::Json(value) => primitive_text(&value)
                    .map(|value| {
                        vec![StaticNode::Text {
                            value,
                            source: source.clone(),
                        }]
                    })
                    .ok_or_else(|| {
                        self.error(
                            "KV1101",
                            source,
                            "JSX 子节点不是静态文本或声明树",
                            "使用静态 JSX；不要通过运行时数组映射生成字段",
                        )
                    }),
                _ => Err(self.error(
                    "KV1101",
                    source,
                    "函数引用不能直接成为 Schema 子节点",
                    "使用静态 JSX 声明或 Schema Component",
                )),
            },
            NodeKind::Spread { .. } => Err(self.error(
                "KV1101",
                source,
                "首版不支持 JSX spread 子节点",
                "使用静态 Fragment 或 Schema Component 组合",
            )),
            NodeKind::Element(element) => self.expand_element(path, &element, source),
        }
    }

    fn expand_children(
        &mut self,
        path: &Path,
        children: &[NodeId],
    ) -> Result<Vec<StaticNode>, DslDiagnostic> {
        let mut nodes = vec![];
        for child in children {
            nodes.extend(self.expand_node(path, *child)?);
        }
        Ok(nodes)
    }

    fn expand_element(
        &mut self,
        path: &Path,
        element: &ElementIR,
        source: SourceLocation,
    ) -> Result<Vec<StaticNode>, DslDiagnostic> {
        let reference = self.element_reference(path, &element.name, &source)?;
        if let Some((target, reference)) = &reference
            && !reference.external
            && reference.module.to_string_lossy().ends_with(".diu.tsx")
        {
            if !element.attributes.is_empty() || !element.children.is_empty() {
                return Err(self.error(
                    "KV1106",
                    source,
                    "首版 Schema Component 不接受 Props 或 children",
                    "在 .diu.tsx 中声明固定结构，通过静态模块常量复用值",
                ));
            }
            if self.schemas.iter().any(|(module, export, _)| {
                module == &reference.module && export == &reference.export
            }) {
                return Err(self.error(
                    "KV1102",
                    source,
                    "Schema Component 展开存在循环",
                    "解除组件之间的递归引用",
                ));
            }
            self.schemas.push((
                reference.module.clone(),
                reference.export.clone(),
                source.clone(),
            ));
            let result = self.eval_target(target.clone(), &source);
            self.schemas.pop();
            return match result? {
                Evaluated::Nodes(nodes) => Ok(nodes),
                _ => Err(self.error(
                    "KV1101",
                    source,
                    "Schema Component 必须导出静态 JSX",
                    "直接导出 JSX 或 Fragment，不要导出函数",
                )),
            };
        }
        let custom = reference.is_some();
        let mut attributes = BTreeMap::new();
        for attribute in &element.attributes {
            let attr_source = SourceLocation {
                path: path.into(),
                span: attribute.span.clone(),
            };
            match &attribute.kind {
                AttributeKind::Named { name, value } => {
                    let name = name.text();
                    let value = match value {
                        None => StaticValue::Json(Value::Bool(true)),
                        Some(AttributeValueIR::String(value)) => {
                            StaticValue::Json(Value::String(value.to_string().map_err(|_| {
                                self.error(
                                    "KV1101",
                                    attr_source.clone(),
                                    "属性包含无效 Unicode",
                                    "使用有效字符串",
                                )
                            })?))
                        }
                        Some(AttributeValueIR::Expression {
                            expression: Some(expression),
                            ..
                        }) => self.attribute_value(path, *expression, custom)?,
                        Some(AttributeValueIR::Expression {
                            expression: None, ..
                        }) => {
                            return Err(self.error(
                                "KV1101",
                                attr_source,
                                "属性表达式不能为空",
                                "提供明确的值",
                            ));
                        }
                        Some(AttributeValueIR::Element(_) | AttributeValueIR::Fragment(_)) => {
                            return Err(self.error(
                                "KV1101",
                                attr_source,
                                "首版属性不直接接受 JSX 值",
                                "将结构放入 children 或普通 React 组件内部",
                            ));
                        }
                    };
                    if attributes.insert(name.clone(), value).is_some() {
                        return Err(self.error(
                            "KV1105",
                            attr_source,
                            format!("重复属性 {name}"),
                            "去除重复声明和 spread 冲突，不能依赖覆盖顺序修改 Schema",
                        ));
                    }
                }
                AttributeKind::Spread { expression } => {
                    let Value::Object(values) = self.evaluate_value(path, *expression)? else {
                        return Err(self.error(
                            "KV1105",
                            attr_source,
                            "JSX 属性展开必须是静态对象",
                            "使用静态 const 对象",
                        ));
                    };
                    for (name, value) in values {
                        if attributes
                            .insert(name.clone(), StaticValue::Json(value))
                            .is_some()
                        {
                            return Err(self.error(
                                "KV1105",
                                attr_source,
                                format!("展开属性 {name} 与现有属性冲突"),
                                "明确每个属性的唯一来源",
                            ));
                        }
                    }
                }
            }
            self.check_collection(attributes.len(), &source)?;
        }
        let children = self.expand_children(path, &element.children)?;
        Ok(vec![StaticNode::Element(StaticElement {
            name: element.name.text(),
            attributes,
            children,
            source,
            module_ref: reference.map(|(_, reference)| reference),
        })])
    }

    fn attribute_value(
        &mut self,
        path: &Path,
        expression: ExpressionId,
        custom: bool,
    ) -> Result<StaticValue, DslDiagnostic> {
        match self.eval_expr(path, expression) {
            Ok(Evaluated::Json(value)) => Ok(StaticValue::Json(value)),
            Ok(Evaluated::Reference(reference)) => Ok(StaticValue::Reference(reference)),
            Ok(Evaluated::Nodes(_)) => Err(self.error(
                "KV1101",
                self.expression_location(path, expression)?,
                "属性值不能包含 Schema 节点",
                "使用 children 或普通 React 组件实现",
            )),
            Err(error) if custom && error.code == "KV1101" => {
                Ok(StaticValue::Expression(ExpressionReference {
                    module: path.into(),
                    expression,
                    span: self.expression_location(path, expression)?.span,
                }))
            }
            Err(error) => Err(error),
        }
    }
}
