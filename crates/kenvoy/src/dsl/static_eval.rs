//! 声明专用的静态解释器，只消费 KEngine 公共 IR，不执行 JavaScript。

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use kengine::*;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Number, Value};

use super::{DslDiagnostic, SourceLocation};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedReference {
    pub module: PathBuf,
    pub export: String,
    pub external: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpressionReference {
    pub module: PathBuf,
    pub expression: ExpressionId,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StaticValue {
    Json(Value),
    Reference(ResolvedReference),
    Expression(ExpressionReference),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StaticElement {
    pub name: String,
    pub attributes: BTreeMap<String, StaticValue>,
    pub children: Vec<StaticNode>,
    pub source: SourceLocation,
    pub module_ref: Option<ResolvedReference>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StaticNode {
    Element(StaticElement),
    Text {
        value: String,
        source: SourceLocation,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct EvalLimits {
    pub depth: usize,
    pub nodes: usize,
    pub collection: usize,
    pub steps: usize,
}

impl Default for EvalLimits {
    fn default() -> Self {
        Self {
            depth: 128,
            nodes: 20_000,
            collection: 10_000,
            steps: 100_000,
        }
    }
}

#[derive(Debug, Clone)]
enum Evaluated {
    Json(Value),
    Nodes(Vec<StaticNode>),
    Reference(ResolvedReference),
}

/// 每次入口求值使用独立预算和循环栈；错误结果不能用于生成 Schema。
pub struct Evaluator<'a> {
    program: &'a ProgramIR,
    limits: EvalLimits,
    steps: usize,
    nodes: usize,
    node_depth: usize,
    expressions: Vec<(PathBuf, ExpressionId)>,
    schemas: Vec<(PathBuf, String, SourceLocation)>,
    checked_modules: BTreeSet<PathBuf>,
}

impl<'a> Evaluator<'a> {
    pub fn new(program: &'a ProgramIR) -> Self {
        Self::with_limits(program, EvalLimits::default())
    }

    pub fn with_limits(program: &'a ProgramIR, limits: EvalLimits) -> Self {
        Self {
            program,
            limits,
            steps: 0,
            nodes: 0,
            node_depth: 0,
            expressions: vec![],
            schemas: vec![],
            checked_modules: BTreeSet::new(),
        }
    }

    pub fn entry(&mut self, entry: &Path) -> Result<Vec<StaticNode>, DslDiagnostic> {
        let module = self.module(entry)?;
        let location = location(module, module.source_span());
        if self.program.has_errors() {
            return Err(self.error(
                "KV1104",
                location,
                "源项目存在解析、模块或绑定错误",
                "先修复 KEngine 诊断，再进行声明分析",
            ));
        }
        self.ensure_static_module(entry)?;
        let target = self.export_target(entry, "default", &location)?;
        match self.eval_target(target, &location)? {
            Evaluated::Nodes(nodes) => Ok(nodes),
            _ => Err(self.error(
                "KV1101",
                location,
                "默认导出必须是静态 JSX 声明树",
                "直接导出 JSX、Fragment 或引用具有静态 JSX 初始值的 const",
            )),
        }
    }

    pub fn evaluate_value(
        &mut self,
        module: &Path,
        expression: ExpressionId,
    ) -> Result<Value, DslDiagnostic> {
        let source = self.expression_location(module, expression)?;
        match self.eval_expr(module, expression)? {
            Evaluated::Json(value) => Ok(value),
            _ => Err(self.error(
                "KV1101",
                source,
                "此处需要静态 JSON 值",
                "使用字面量或可确定的 const，不能用函数、组件或动态表达式",
            )),
        }
    }

    fn module(&self, path: &Path) -> Result<&'a ModuleIR, DslDiagnostic> {
        self.program
            .modules
            .iter()
            .find(|module| module.path == path)
            .ok_or_else(|| {
                self.error(
                    "KV1104",
                    SourceLocation {
                        path: path.into(),
                        span: empty_span(),
                    },
                    "声明引用的模块不在分析结果中",
                    "使用项目内可解析模块，检查入口和模块边界",
                )
            })
    }

    fn expression_location(
        &self,
        path: &Path,
        id: ExpressionId,
    ) -> Result<SourceLocation, DslDiagnostic> {
        let module = self.module(path)?;
        module
            .expression(id)
            .map(|expression| location(module, expression.span.clone()))
            .ok_or_else(|| {
                self.error(
                    "KV1104",
                    location(module, module.source_span()),
                    "表达式引用无效",
                    "重新分析项目，不能混用不同版本的 Generic IR 标识",
                )
            })
    }

    fn error(
        &self,
        code: &str,
        source: SourceLocation,
        message: impl Into<String>,
        suggestion: impl Into<String>,
    ) -> DslDiagnostic {
        let mut diagnostic = DslDiagnostic::new(code, source, message, suggestion);
        diagnostic.related = self
            .schemas
            .iter()
            .map(|(_, _, source)| source.clone())
            .collect();
        diagnostic
    }

    fn tick(&mut self, source: &SourceLocation) -> Result<(), DslDiagnostic> {
        self.steps += 1;
        if self.steps > self.limits.steps
            || self.expressions.len() + self.schemas.len() >= self.limits.depth
        {
            return Err(self.error(
                "KV1103",
                source.clone(),
                "静态分析超过深度或运算预算",
                "拆分过深声明，减少递归组合和巨大常量",
            ));
        }
        Ok(())
    }

    fn check_collection(&self, size: usize, source: &SourceLocation) -> Result<(), DslDiagnostic> {
        if size > self.limits.collection {
            Err(self.error(
                "KV1103",
                source.clone(),
                "静态集合超过大小限制",
                "减少数组、对象或展开属性的数量",
            ))
        } else {
            Ok(())
        }
    }

    fn export_target(
        &self,
        module: &Path,
        name: &str,
        source: &SourceLocation,
    ) -> Result<ModuleBindingTarget, DslDiagnostic> {
        let targets = self
            .program
            .module_bindings
            .exports
            .iter()
            .find(|export| {
                export.module == module
                    && export.name == name
                    && export.namespace == SymbolNamespace::Value
            })
            .map(|export| export.targets.as_slice())
            .unwrap_or_default();
        match targets {
            [target] => Ok(target.clone()),
            _ => Err(self.error(
                "KV1104",
                source.clone(),
                format!("导出 {name} 缺失或存在歧义"),
                "使用唯一的值导出，检查 type-only 和星号转导",
            )),
        }
    }

    fn identifier_target(
        &self,
        path: &Path,
        identifier: &IdentifierIR,
    ) -> Result<ModuleBindingTarget, DslDiagnostic> {
        let module = self.module(path)?;
        let source = location(module, identifier.span.clone());
        let symbol = module
            .semantic
            .references_at(identifier.span.start, identifier.span.end)
            .find(|reference| reference.namespace == SymbolNamespace::Value)
            .and_then(|reference| reference.symbol)
            .and_then(|id| module.semantic.symbol(id))
            .ok_or_else(|| {
                self.error(
                    "KV1104",
                    source.clone(),
                    format!("无法确定 {} 的值绑定", identifier.name),
                    "使用已声明的 const 或明确导入的模块导出",
                )
            })?;
        if symbol.import.is_some() {
            let targets = self
                .program
                .module_bindings
                .imports
                .iter()
                .find(|import| import.module == path && import.symbol == symbol.id)
                .map(|import| import.targets.as_slice())
                .unwrap_or_default();
            return match targets {
                [target] => Ok(target.clone()),
                _ => Err(self.error(
                    "KV1104",
                    source,
                    "导入没有唯一的值目标",
                    "检查模块导出、type-only 声明和转导歧义",
                )),
            };
        }
        Ok(ModuleBindingTarget::Symbol {
            module: path.into(),
            symbol: symbol.id,
        })
    }

    fn imported_reference(
        &self,
        path: &Path,
        identifier: &IdentifierIR,
        member: Option<&str>,
        source: &SourceLocation,
    ) -> Result<Option<ResolvedReference>, DslDiagnostic> {
        let module = self.module(path)?;
        let import = module
            .semantic
            .references_at(identifier.span.start, identifier.span.end)
            .find(|reference| reference.namespace == SymbolNamespace::Value)
            .and_then(|reference| reference.symbol)
            .and_then(|id| module.semantic.symbol(id))
            .and_then(|symbol| symbol.import.as_ref());
        let Some(import) = import else {
            return Ok(None);
        };
        if member.is_some() && !import.namespace {
            return Err(self.error(
                "KV1104",
                source.clone(),
                "嵌套模块成员不能作为运行时引用",
                "请直接导入可调用导出，或使用一层 import * as namespace 成员",
            ));
        }
        let specifier = String::from_utf16(&import.source_utf16).map_err(|_| {
            self.error(
                "KV1104",
                source.clone(),
                "导入模块说明符包含无效 Unicode",
                "修复模块说明符",
            )
        })?;
        let target = self
            .program
            .module_graph
            .edges
            .iter()
            .find(|edge| {
                edge.importer == path
                    && edge.specifier == specifier
                    && edge.kind == DependencyKind::Import
            })
            .and_then(|edge| edge.target.as_ref())
            .ok_or_else(|| {
                self.error(
                    "KV1104",
                    source.clone(),
                    "无法定位运行时导入路径",
                    "修复对应的值导入，不能仅使用 type-only 依赖",
                )
            })?;
        let export = if import.namespace {
            member.unwrap_or("*").to_owned()
        } else {
            import.imported.clone().ok_or_else(|| {
                self.error(
                    "KV1104",
                    source.clone(),
                    "导入缺少有效导出名",
                    "使用明确且有效的导出名称",
                )
            })?
        };
        let (module, external) = match target {
            ResolvedModule::File { path } => (path.clone(), false),
            ResolvedModule::External { specifier } => (specifier.into(), true),
        };
        Ok(Some(ResolvedReference {
            module,
            export,
            external,
        }))
    }

    fn eval_expr(&mut self, path: &Path, id: ExpressionId) -> Result<Evaluated, DslDiagnostic> {
        let module = self.module(path)?;
        let expression = module.expression(id).cloned().ok_or_else(|| {
            self.error(
                "KV1104",
                location(module, module.source_span()),
                "表达式引用无效",
                "重新分析源码",
            )
        })?;
        let source = location(module, expression.span.clone());
        self.tick(&source)?;
        if self
            .expressions
            .iter()
            .any(|(module, expression)| module == path && *expression == id)
        {
            return Err(self.error(
                "KV1102",
                source,
                "静态常量存在循环引用",
                "解除常量或模块默认导出的循环引用",
            ));
        }
        self.expressions.push((path.into(), id));
        let result = self.eval_shape(path, &expression, &source);
        self.expressions.pop();
        result
    }

    fn eval_shape(
        &mut self,
        path: &Path,
        expression: &ExpressionIR,
        source: &SourceLocation,
    ) -> Result<Evaluated, DslDiagnostic> {
        let json = match &expression.shape {
            ExpressionShape::Literal => match &expression.literal {
                Some(LiteralIR::Null) => Value::Null,
                Some(LiteralIR::Boolean(value)) => Value::Bool(*value),
                Some(LiteralIR::Number { value: Some(value) }) => {
                    number(*value).ok_or_else(|| {
                        self.error(
                            "KV1101",
                            source.clone(),
                            "静态数值必须有限",
                            "不要使用 NaN 或 Infinity",
                        )
                    })?
                }
                Some(LiteralIR::String(value)) => {
                    let value = value.to_string().map_err(|_| {
                        self.error(
                            "KV1101",
                            source.clone(),
                            "字符串包含无法表示为 Unicode 的 UTF-16 单元",
                            "使用有效 Unicode 字符串",
                        )
                    })?;
                    self.check_collection(value.chars().count(), source)?;
                    Value::String(value)
                }
                _ => {
                    return Err(self.error(
                        "KV1101",
                        source.clone(),
                        "此字面量不能作为 JSON 声明值",
                        "不要在声明属性中使用 BigInt、正则或非有限数值",
                    ));
                }
            },
            ExpressionShape::Identifier => {
                let identifier = expression.direct_identifier.as_ref().ok_or_else(|| {
                    self.error(
                        "KV1104",
                        source.clone(),
                        "标识符缺少通用结构",
                        "重新分析源码",
                    )
                })?;
                let value = self.eval_target(self.identifier_target(path, identifier)?, source)?;
                return Ok(match value {
                    Evaluated::Reference(reference) => Evaluated::Reference(
                        self.imported_reference(path, identifier, None, source)?
                            .unwrap_or(reference),
                    ),
                    other => other,
                });
            }
            ExpressionShape::Wrapper { expression, .. } => {
                return self.eval_expr(path, *expression);
            }
            ExpressionShape::Jsx { node } => {
                return self.expand_node(path, *node).map(Evaluated::Nodes);
            }
            ExpressionShape::Array { elements } => {
                self.check_collection(elements.len(), source)?;
                let mut values = Vec::new();
                for element in elements {
                    let Some(element) = element else {
                        return Err(self.error(
                            "KV1101",
                            source.clone(),
                            "静态 JSON 数组不能包含空洞",
                            "显式使用 null，不能将空洞静默转为 null",
                        ));
                    };
                    let value = self.evaluate_value(path, element.expression)?;
                    if element.spread {
                        let Value::Array(items) = value else {
                            return Err(self.error(
                                "KV1105",
                                source.clone(),
                                "数组展开要求静态数组",
                                "将 spread 目标改为可确定的数组",
                            ));
                        };
                        values.extend(items);
                    } else {
                        values.push(value);
                    }
                    self.check_collection(values.len(), source)?;
                }
                Value::Array(values)
            }
            ExpressionShape::Object { properties } => {
                self.check_collection(properties.len(), source)?;
                let mut values = Map::new();
                for property in properties {
                    match &property.kind {
                        ObjectPropertyKind::Value { key, value, .. } => {
                            let key = self.property_name(path, key, source)?;
                            if key == "__proto__" {
                                return Err(self.error(
                                    "KV1101",
                                    source.clone(),
                                    "静态对象不支持原型设置属性",
                                    "使用普通数据键，避免 __proto__",
                                ));
                            }
                            values.insert(key, self.evaluate_value(path, *value)?);
                        }
                        ObjectPropertyKind::Spread { expression } => {
                            let Value::Object(items) = self.evaluate_value(path, *expression)?
                            else {
                                return Err(self.error(
                                    "KV1105",
                                    source.clone(),
                                    "对象展开要求静态对象",
                                    "将 spread 目标改为可确定的数据对象",
                                ));
                            };
                            values.extend(items);
                        }
                        _ => {
                            return Err(self.error(
                                "KV1101",
                                source.clone(),
                                "声明对象不能包含方法、getter 或 setter",
                                "使用静态数据对象，不能通过函数产生声明",
                            ));
                        }
                    }
                    self.check_collection(values.len(), source)?;
                }
                Value::Object(values)
            }
            ExpressionShape::Unary { operator, argument } => {
                let value = self.evaluate_value(path, *argument)?;
                match operator.as_str() {
                    "!" => Value::Bool(!truthy(&value)),
                    "+" | "-" => {
                        let value = value.as_f64().ok_or_else(|| {
                            self.error(
                                "KV1101",
                                source.clone(),
                                "数值一元运算要求数字",
                                "显式声明数字，不依赖 JavaScript 隐式类型转换",
                            )
                        })?;
                        number(if operator == "-" { -value } else { value }).ok_or_else(|| {
                            self.error(
                                "KV1101",
                                source.clone(),
                                "一元运算结果不是有限数字",
                                "调整静态数值",
                            )
                        })?
                    }
                    _ => {
                        return Err(self.error(
                            "KV1101",
                            source.clone(),
                            "此一元运算不在静态声明范围内",
                            "使用 !、+、- 或直接字面量",
                        ));
                    }
                }
            }
            ExpressionShape::Binary {
                operator,
                left,
                right,
            } => {
                let left = self.evaluate_value(path, *left)?;
                match operator.as_str() {
                    "&&" if !truthy(&left) => left,
                    "||" if truthy(&left) => left,
                    "??" if !left.is_null() => left,
                    "&&" | "||" | "??" => return self.eval_expr(path, *right),
                    _ => {
                        let right = self.evaluate_value(path, *right)?;
                        self.binary(operator, left, right, source)?
                    }
                }
            }
            ExpressionShape::Conditional {
                test,
                consequent,
                alternate,
            } => {
                let value = self.evaluate_value(path, *test)?;
                return self.eval_expr(
                    path,
                    if truthy(&value) {
                        *consequent
                    } else {
                        *alternate
                    },
                );
            }
            ExpressionShape::Member { object, property } => {
                if let Some(target) =
                    self.namespace_member_target(path, *object, property, source)?
                {
                    let value = self.eval_target(target, source)?;
                    if let Evaluated::Reference(reference) = value {
                        let identifier = self
                            .module(path)?
                            .expression(*object)
                            .and_then(|expression| expression.direct_identifier.as_ref())
                            .cloned()
                            .ok_or_else(|| {
                                self.error(
                                    "KV1104",
                                    source.clone(),
                                    "模块成员缺少直接绑定",
                                    "请直接导入可调用导出",
                                )
                            })?;
                        let name = match property {
                            MemberPropertyIR::Identifier(name) => name.name.clone(),
                            MemberPropertyIR::Computed { expression, .. } => {
                                let name = self.evaluate_value(path, *expression)?;
                                self.key_value(name, source)?
                            }
                            MemberPropertyIR::Private(_) => {
                                return Err(self.error(
                                    "KV1104",
                                    source.clone(),
                                    "私有模块成员不可导入",
                                    "请直接导入可调用导出",
                                ));
                            }
                        };
                        return Ok(Evaluated::Reference(
                            self.imported_reference(path, &identifier, Some(&name), source)?
                                .unwrap_or(reference),
                        ));
                    }
                    return Ok(value);
                }
                let value = self.evaluate_value(path, *object)?;
                let key = match property {
                    MemberPropertyIR::Identifier(value) => value.name.clone(),
                    MemberPropertyIR::Computed { expression, .. } => {
                        let value = self.evaluate_value(path, *expression)?;
                        self.key_value(value, source)?
                    }
                    MemberPropertyIR::Private(_) => {
                        return Err(self.error(
                            "KV1101",
                            source.clone(),
                            "私有成员不是静态声明值",
                            "使用数据对象属性",
                        ));
                    }
                };
                match value {
                    Value::Object(object) => object.get(&key).cloned().ok_or_else(|| {
                        self.error(
                            "KV1101",
                            source.clone(),
                            format!("静态对象没有属性 {key}"),
                            "检查属性名称，缺失不等同 null",
                        )
                    })?,
                    Value::Array(array) if key == "length" => Value::Number(array.len().into()),
                    Value::Array(array) => key
                        .parse::<usize>()
                        .ok()
                        .filter(|index| index.to_string() == key)
                        .and_then(|index| array.get(index))
                        .cloned()
                        .ok_or_else(|| {
                            self.error(
                                "KV1101",
                                source.clone(),
                                "静态数组下标无效",
                                "使用已存在的非负整数下标",
                            )
                        })?,
                    _ => {
                        return Err(self.error(
                            "KV1101",
                            source.clone(),
                            "静态成员读取只支持数据对象和数组",
                            "不要依赖原型方法或动态对象",
                        ));
                    }
                }
            }
            ExpressionShape::Template {
                quasis,
                expressions,
            } => {
                let mut result = String::new();
                for (index, part) in quasis.iter().enumerate() {
                    let value = part
                        .cooked_utf16
                        .as_ref()
                        .and_then(|value| String::from_utf16(value).ok())
                        .ok_or_else(|| {
                            self.error(
                                "KV1101",
                                source.clone(),
                                "模板字符串包含无效 Unicode",
                                "修复转义字符串",
                            )
                        })?;
                    result.push_str(&value);
                    if let Some(expression) = expressions.get(index) {
                        let value = self.evaluate_value(path, *expression)?;
                        result.push_str(&primitive_text(&value).ok_or_else(|| {
                            self.error(
                                "KV1101",
                                source.clone(),
                                "模板插值只允许基本数据值",
                                "不要隐式转换数组或对象",
                            )
                        })?);
                    }
                    self.check_collection(result.chars().count(), source)?;
                }
                Value::String(result)
            }
            ExpressionShape::Opaque => {
                return Err(self.error(
                    "KV1101",
                    source.clone(),
                    format!("表达式 {} 无法静态求值", expression.kind),
                    "使用静态值；函数调用、map 和任意执行不能生成 Schema",
                ));
            }
        };
        Ok(Evaluated::Json(json))
    }

    fn binary(
        &self,
        operator: &str,
        left: Value,
        right: Value,
        source: &SourceLocation,
    ) -> Result<Value, DslDiagnostic> {
        if matches!(operator, "===" | "!==") {
            if left.is_object() || right.is_object() || left.is_array() || right.is_array() {
                return Err(self.error(
                    "KV1101",
                    source.clone(),
                    "静态比较不模拟对象身份",
                    "仅比较基本数据值",
                ));
            }
            let equal = if left.is_number() && right.is_number() {
                left.as_f64() == right.as_f64()
            } else {
                left == right
            };
            return Ok(Value::Bool(if operator == "!==" { !equal } else { equal }));
        }
        if operator == "+" && left.is_string() && right.is_string() {
            self.check_collection(
                left.as_str()
                    .unwrap_or_default()
                    .chars()
                    .count()
                    .saturating_add(right.as_str().unwrap_or_default().chars().count()),
                source,
            )?;
            return Ok(Value::String(format!(
                "{}{}",
                left.as_str().unwrap_or_default(),
                right.as_str().unwrap_or_default()
            )));
        }
        let (Some(left), Some(right)) = (left.as_f64(), right.as_f64()) else {
            return Err(self.error(
                "KV1101",
                source.clone(),
                "静态运算不支持隐式类型转换",
                "运算两侧使用数字，字符串仅支持同类型拼接",
            ));
        };
        let result = match operator {
            "<" => return Ok(Value::Bool(left < right)),
            "<=" => return Ok(Value::Bool(left <= right)),
            ">" => return Ok(Value::Bool(left > right)),
            ">=" => return Ok(Value::Bool(left >= right)),
            "+" => left + right,
            "-" => left - right,
            "*" => left * right,
            "/" => left / right,
            "%" => left % right,
            "**" => left.powf(right),
            _ => {
                return Err(self.error(
                    "KV1101",
                    source.clone(),
                    format!("静态运算不支持 {operator}"),
                    "使用基本算术、严格比较或显式字面量",
                ));
            }
        };
        number(result).ok_or_else(|| {
            self.error(
                "KV1101",
                source.clone(),
                "静态运算产生非有限数字",
                "检查除零、溢出或指数运算",
            )
        })
    }

    fn property_name(
        &mut self,
        path: &Path,
        key: &PropertyKeyIR,
        source: &SourceLocation,
    ) -> Result<String, DslDiagnostic> {
        match key {
            PropertyKeyIR::Identifier(identifier) => Ok(identifier.name.clone()),
            PropertyKeyIR::String(value) => value.to_string().map_err(|_| {
                self.error(
                    "KV1101",
                    source.clone(),
                    "属性名包含无效 Unicode",
                    "使用有效字符串",
                )
            }),
            PropertyKeyIR::Number {
                value: Some(value), ..
            } => Ok(value.to_string()),
            PropertyKeyIR::Computed { expression, .. } => {
                let value = self.evaluate_value(path, *expression)?;
                self.key_value(value, source)
            }
            _ => Err(self.error(
                "KV1101",
                source.clone(),
                "此属性名不能用于静态数据对象",
                "使用字符串、数字或可确定的计算属性",
            )),
        }
    }

    fn key_value(&self, value: Value, source: &SourceLocation) -> Result<String, DslDiagnostic> {
        match value {
            Value::String(value) => Ok(value),
            Value::Number(value) => Ok(value.to_string()),
            _ => Err(self.error(
                "KV1101",
                source.clone(),
                "计算属性名必须是字符串或数字",
                "显式提供有效属性名",
            )),
        }
    }

    fn eval_target(
        &mut self,
        target: ModuleBindingTarget,
        source: &SourceLocation,
    ) -> Result<Evaluated, DslDiagnostic> {
        match target {
            ModuleBindingTarget::Expression { module, expression } => {
                if self
                    .module(&module)?
                    .expression(expression)
                    .is_some_and(|value| {
                        matches!(value.kind.as_str(), "ArrowFunction" | "Function" | "Class")
                    })
                {
                    return self
                        .reference_for_target(
                            &ModuleBindingTarget::Expression { module, expression },
                            source,
                        )
                        .map(Evaluated::Reference);
                }
                self.ensure_static_module(&module)?;
                self.eval_expr(&module, expression)
            }
            ModuleBindingTarget::Symbol {
                module: path,
                symbol,
            } => {
                let module = self.module(&path)?;
                let symbol = module.semantic.symbol(symbol).ok_or_else(|| {
                    self.error("KV1104", source.clone(), "符号引用无效", "重新分析源码")
                })?;
                if symbol.kind == SymbolKind::Const {
                    if symbol.scope != ScopeId(0) {
                        return Err(self.error(
                            "KV1101",
                            source.clone(),
                            "Schema 常量必须位于模块顶层",
                            "将静态数据声明为模块顶层 const",
                        ));
                    }
                    let declarator = module
                        .declarations
                        .iter()
                        .flat_map(|declaration| &declaration.declarators)
                        .find(|declarator| match &declarator.pattern.kind {
                            BindingPatternKind::Identifier(identifier) => symbol
                                .declarations
                                .iter()
                                .any(|span| span == &identifier.span),
                            _ => false,
                        });
                    if let Some(initializer) =
                        declarator.and_then(|declarator| declarator.initializer)
                    {
                        if module.expression(initializer).is_some_and(|value| {
                            matches!(value.kind.as_str(), "ArrowFunction" | "Function" | "Class")
                        }) {
                            return self
                                .reference_for_target(
                                    &ModuleBindingTarget::Symbol {
                                        module: path,
                                        symbol: symbol.id,
                                    },
                                    source,
                                )
                                .map(Evaluated::Reference);
                        }
                        self.ensure_static_module(&path)?;
                        return self.eval_expr(&path, initializer);
                    }
                    return Err(self.error(
                        "KV1101",
                        source.clone(),
                        "此 const 没有可直接定位的静态初始值",
                        "首版声明常量使用独立标识符绑定；解构结构保留但不进行值推导",
                    ));
                }
                if matches!(symbol.kind, SymbolKind::Function | SymbolKind::Class) {
                    return self
                        .reference_for_target(
                            &ModuleBindingTarget::Symbol {
                                module: path,
                                symbol: symbol.id,
                            },
                            source,
                        )
                        .map(Evaluated::Reference);
                }
                Err(self.error(
                    "KV1101",
                    source.clone(),
                    "只有模块顶层 const 可以提供静态声明值",
                    "不要使用 let、var、参数、namespace 或运行时值推导 Schema",
                ))
            }
            target @ (ModuleBindingTarget::Declaration { .. }
            | ModuleBindingTarget::External { .. }) => self
                .reference_for_target(&target, source)
                .map(Evaluated::Reference),
            ModuleBindingTarget::Namespace { .. } => Err(self.error(
                "KV1101",
                source.clone(),
                "模块 namespace 本身不是 JSON 值",
                "选择明确的模块导出成员",
            )),
        }
    }

    fn reference_for_target(
        &self,
        target: &ModuleBindingTarget,
        source: &SourceLocation,
    ) -> Result<ResolvedReference, DslDiagnostic> {
        if let ModuleBindingTarget::External {
            specifier,
            imported,
        } = target
        {
            return Ok(ResolvedReference {
                module: specifier.into(),
                export: imported.clone(),
                external: true,
            });
        }
        let path = match target {
            ModuleBindingTarget::Symbol { module, .. }
            | ModuleBindingTarget::Expression { module, .. }
            | ModuleBindingTarget::Declaration { module, .. }
            | ModuleBindingTarget::Namespace { module } => module,
            _ => unreachable!(),
        };
        let export = self.program.module_bindings.exports.iter().find(|export| {
            export.module == *path
                && export.namespace == SymbolNamespace::Value
                && export.targets.as_slice() == [target.clone()]
        });
        let Some(export) = export else {
            return Err(self.error(
                "KV1104",
                source.clone(),
                "运行时组件或函数必须具有可导入的导出",
                "给函数或组件添加 export，并使用明确的模块引用",
            ));
        };
        Ok(ResolvedReference {
            module: path.clone(),
            export: export.name.clone(),
            external: false,
        })
    }

    fn namespace_member_target(
        &mut self,
        path: &Path,
        object: ExpressionId,
        property: &MemberPropertyIR,
        source: &SourceLocation,
    ) -> Result<Option<ModuleBindingTarget>, DslDiagnostic> {
        let expression = self.module(path)?.expression(object).cloned();
        let Some(ExpressionIR {
            direct_identifier: Some(identifier),
            ..
        }) = expression
        else {
            return Ok(None);
        };
        let target = self.identifier_target(path, &identifier)?;
        let name = match property {
            MemberPropertyIR::Identifier(name) => name.name.clone(),
            MemberPropertyIR::Computed { expression, .. } => {
                let value = self.evaluate_value(path, *expression)?;
                self.key_value(value, source)?
            }
            MemberPropertyIR::Private(_) => return Ok(None),
        };
        match target {
            ModuleBindingTarget::Namespace { module } => {
                self.export_target(&module, &name, source).map(Some)
            }
            ModuleBindingTarget::External {
                specifier,
                imported,
            } if imported == "*" => Ok(Some(ModuleBindingTarget::External {
                specifier,
                imported: name,
            })),
            _ => Ok(None),
        }
    }

    fn ensure_static_module(&mut self, path: &Path) -> Result<(), DslDiagnostic> {
        if self.checked_modules.contains(path) {
            return Ok(());
        }
        let module = self.module(path)?;
        for expression in &module.expressions {
            if !matches!(
                expression.kind.as_str(),
                "Call" | "New" | "Assignment" | "Await" | "Yield"
            ) && !(expression.kind == "OtherExpression"
                && expression.shape == ExpressionShape::Opaque)
            {
                continue;
            }
            let inside_function = module.semantic.scopes.iter().any(|scope| {
                matches!(
                    scope.kind,
                    ScopeKind::Function | ScopeKind::FunctionParameters
                ) && scope.span.start <= expression.span.start
                    && scope.span.end >= expression.span.end
            });
            // JSX 内的表达式由对应位置处理：Schema 要求静态值，UI Props 保留客户端引用。
            let inside_jsx = module.nodes.iter().any(|node| {
                matches!(node.kind, NodeKind::Element(_) | NodeKind::Fragment { .. })
                    && node.span.start <= expression.span.start
                    && node.span.end >= expression.span.end
            });
            if !inside_function && !inside_jsx {
                return Err(self.error(
                    "KV1101",
                    location(module, expression.span.clone()),
                    "静态声明模块包含顶层调用、赋值或其他运行时副作用",
                    "把计算移入明确注册的函数或普通 UI 模块；Schema 模块只保留静态声明",
                ));
            }
        }
        self.checked_modules.insert(path.into());
        Ok(())
    }

    fn expand_node(&mut self, path: &Path, id: NodeId) -> Result<Vec<StaticNode>, DslDiagnostic> {
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

    fn element_reference(
        &mut self,
        path: &Path,
        name: &ElementNameIR,
        source: &SourceLocation,
    ) -> Result<Option<(ModuleBindingTarget, ResolvedReference)>, DslDiagnostic> {
        let identifier = match name {
            ElementNameIR::Identifier(identifier) => {
                let bound = self
                    .module(path)?
                    .semantic
                    .references_at(identifier.span.start, identifier.span.end)
                    .any(|reference| {
                        reference.namespace == SymbolNamespace::Value && reference.symbol.is_some()
                    });
                if !bound && is_intrinsic(&identifier.name) {
                    return Ok(None);
                }
                identifier.clone()
            }
            ElementNameIR::Member { parts, .. } => parts.first().cloned().ok_or_else(|| {
                self.error(
                    "KV1104",
                    source.clone(),
                    "组件成员名称为空",
                    "使用有效组件引用",
                )
            })?,
            _ => return Ok(None),
        };
        let mut target = self.identifier_target(path, &identifier)?;
        if let ElementNameIR::Member { parts, .. } = name {
            for part in parts.iter().skip(1) {
                target = match target {
                    ModuleBindingTarget::Namespace { module } => {
                        self.export_target(&module, &part.name, source)?
                    }
                    ModuleBindingTarget::External {
                        specifier,
                        imported,
                    } if imported == "*" => ModuleBindingTarget::External {
                        specifier,
                        imported: part.name.clone(),
                    },
                    _ => {
                        return Err(self.error(
                            "KV1104",
                            source.clone(),
                            "组件成员只支持模块 namespace 的导出",
                            "直接导入组件，不能通过运行时对象构造组件注册表",
                        ));
                    }
                };
            }
        }
        let leaf = self.reference_for_target(&target, source)?;
        // Schema 组合编译时展开，仍以最终声明模块判断；运行时引用保留导入门面，避免丢失其 CSS 和依赖边界。
        if !leaf.external && leaf.module.to_string_lossy().ends_with(".diu.tsx") {
            return Ok(Some((target, leaf)));
        }
        let member = if let ElementNameIR::Member { parts, .. } = name {
            if parts.len() != 2 {
                return Err(self.error(
                    "KV1104",
                    source.clone(),
                    "嵌套模块成员不能作为运行时组件引用",
                    "请直接导入组件，或使用一层 import * as namespace 成员",
                ));
            }
            Some(parts[1].name.as_str())
        } else {
            None
        };
        let reference = self
            .imported_reference(path, &identifier, member, source)?
            .unwrap_or(leaf);
        Ok(Some((target, reference)))
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

fn is_intrinsic(name: &str) -> bool {
    name.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
}
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64().is_some_and(|value| value != 0.0),
        Value::String(value) => !value.is_empty(),
        _ => true,
    }
}
fn number(value: f64) -> Option<Value> {
    if value.is_finite() && value.fract() == 0.0 && value.abs() <= 9_007_199_254_740_991.0 {
        Some(Value::Number((value as i64).into()))
    } else {
        Number::from_f64(value).map(Value::Number)
    }
}
fn primitive_text(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Null => Some("null".into()),
        _ => None,
    }
}
fn empty_span() -> SourceSpan {
    SourceSpan {
        start: 0,
        end: 0,
        start_position: Position { line: 1, column: 1 },
        end_position: Position { line: 1, column: 1 },
    }
}
fn location(module: &ModuleIR, span: SourceSpan) -> SourceLocation {
    SourceLocation {
        path: module.path.clone(),
        span,
    }
}

trait ModuleSpan {
    fn source_span(&self) -> SourceSpan;
}
impl ModuleSpan for ModuleIR {
    fn source_span(&self) -> SourceSpan {
        self.semantic
            .scopes
            .first()
            .map(|scope| scope.span.clone())
            .unwrap_or_else(empty_span)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        error::Error,
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    type TestResult<T = ()> = Result<T, Box<dyn Error>>;
    struct Project(PathBuf);
    impl Project {
        fn new(files: &[(&str, &str)]) -> TestResult<Self> {
            let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
            let project = Self(
                std::env::temp_dir().join(format!("kenvoy-static-{}-{stamp}", std::process::id())),
            );
            fs::create_dir(&project.0)?;
            for (name, source) in files {
                fs::write(project.0.join(name), source)?;
            }
            Ok(project)
        }
        fn program(&self) -> TestResult<ProgramIR> {
            let program = Engine::new().analyze_project(
                &self.0,
                AnalyzeOptions {
                    entry_points: vec!["app.tsx".into()],
                    resolve: ResolveOptions {
                        externals: ["react".into()].into(),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )?;
            assert!(
                !program.has_errors(),
                "通用解析诊断：{:?}",
                program.diagnostics
            );
            Ok(program)
        }
        fn entry(&self) -> TestResult<PathBuf> {
            Ok(self.0.join("app.tsx").canonicalize()?)
        }
    }
    impl Drop for Project {
        fn drop(&mut self) {
            if self.0.parent() == Some(std::env::temp_dir().as_path()) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
    }
    fn element(node: &StaticNode) -> &StaticElement {
        match node {
            StaticNode::Element(element) => element,
            _ => panic!("预期 Element"),
        }
    }

    #[test]
    fn literal_tree_and_const_operators_are_evaluated_without_scanning_other_jsx() -> TestResult {
        let project = Project::new(&[(
            "app.tsx",
            r#"
            const unrelated = () => <resource name="wrong" />;
            const limit = 20 * 5, properties = { required: true, maxLength: limit };
            const choice = true ? 'customers' : unknown;
            export default (<app name={`系统-${choice}`}>{false && <resource name="unused"/>}{true && <page path="/customers"><text name="name" {...properties}/><number name="age" min={-0} integer /></page>}</app>) satisfies unknown;
        "#,
        )])?;
        let program = project.program()?;
        let nodes = Evaluator::new(&program).entry(&project.entry()?)?;
        assert_eq!(nodes.len(), 1);
        let app = element(&nodes[0]);
        assert_eq!(
            app.attributes["name"],
            StaticValue::Json(Value::String("系统-customers".into()))
        );
        let page = element(&app.children[0]);
        let field = element(&page.children[0]);
        assert_eq!(
            field.attributes["maxLength"],
            StaticValue::Json(Value::from(100))
        );
        assert_eq!(page.children.len(), 2);
        Ok(())
    }

    #[test]
    fn cross_module_constants_reexports_schema_fragments_and_ui_references_are_distinct()
    -> TestResult {
        let project = Project::new(&[
            (
                "app.tsx",
                "import Fields from './barrel'; import { Card, label } from './ui'; import * as constants from './constants'; export default <app name={constants.title}><resource name='customers'><Fields /></resource><page path='/customers'><Card value={3} caption={label()} onClick={() => 2} /></page></app>;",
            ),
            ("barrel.ts", "export { default } from './Fields.diu';"),
            (
                "Fields.diu.tsx",
                "import { limit } from './constants'; export default <><text name='name' maxLength={limit}/><text name='phone'/></>;",
            ),
            (
                "constants.ts",
                "export const title = '客户管理', limit = 100;",
            ),
            (
                "ui.tsx",
                "import { useState } from 'react'; export function label(){ return '名称'; } export function Card() { const [count] = useState(0); return <text name='not_schema'>{count}</text>; }",
            ),
        ])?;
        let program = project.program()?;
        let nodes = Evaluator::new(&program).entry(&project.entry()?)?;
        let app = element(&nodes[0]);
        let resource = element(&app.children[0]);
        assert_eq!(resource.children.len(), 2);
        assert!(
            element(&resource.children[0])
                .source
                .path
                .ends_with("Fields.diu.tsx")
        );
        let card = element(&element(&app.children[1]).children[0]);
        assert!(card.children.is_empty(), "普通 UI 函数体不能进入 Schema");
        assert!(card.module_ref.as_ref().is_some_and(|reference| {
            reference.module.ends_with("ui.tsx") && reference.export == "Card"
        }));
        assert!(matches!(
            card.attributes["onClick"],
            StaticValue::Expression(_)
        ));
        assert!(matches!(
            card.attributes["caption"],
            StaticValue::Expression(_)
        ));
        Ok(())
    }

    #[test]
    fn exported_function_and_arrow_handlers_remain_module_references() -> TestResult {
        let project = Project::new(&[
            (
                "app.tsx",
                "import { calculate, asyncCalc } from './logic.server'; export default <app><action handler={calculate}/><action handler={asyncCalc}/></app>;",
            ),
            (
                "logic.server.ts",
                "export function calculate(value: number) { return value * 2; } export const asyncCalc = async (value: number) => value + 1;",
            ),
        ])?;
        let program = project.program()?;
        let nodes = Evaluator::new(&program).entry(&project.entry()?)?;
        for child in &element(&nodes[0]).children {
            assert!(matches!(
                element(child).attributes["handler"],
                StaticValue::Reference(_)
            ));
        }
        Ok(())
    }

    #[test]
    fn runtime_imports_preserve_barrels_aliases_and_namespace_export_names() -> TestResult {
        let project = Project::new(&[
            (
                "app.tsx",
                "import First, { View as Alias, run as handle, label } from './barrel'; import * as all from './barrel'; const local=handle; export default <app name={label}><page><First onClick={handle}/><Alias/><all.View/></page><action handler={all['run']}/><action handler={local}/></app>;",
            ),
            (
                "barrel.ts",
                "import './side-effects'; export { default, Card as View } from './ui'; export { calculate as run } from './logic'; export { label } from './constants';",
            ),
            ("side-effects.ts", "console.log('保留门面运行依赖');"),
            (
                "ui.tsx",
                "export function Card(){ return <div/>; } export default Card;",
            ),
            (
                "logic.ts",
                "export function calculate(value: number){ return value * 2; }",
            ),
            (
                "constants.ts",
                "export const label='静态常量仍追踪最终声明';",
            ),
        ])?;
        let program = project.program()?;
        let nodes = Evaluator::new(&program).entry(&project.entry()?)?;
        let app = element(&nodes[0]);
        assert_eq!(
            app.attributes["name"],
            StaticValue::Json(Value::String("静态常量仍追踪最终声明".into()))
        );
        let page = element(&app.children[0]);
        for (node, name) in page.children.iter().zip(["default", "View", "View"]) {
            let reference = element(node).module_ref.as_ref().ok_or("组件引用缺失")?;
            assert_eq!(
                reference.module,
                project.0.join("barrel.ts").canonicalize()?
            );
            assert_eq!(reference.export, name);
            assert!(!reference.external);
        }
        let callback = &element(&page.children[0]).attributes["onClick"];
        for value in [
            callback,
            &element(&app.children[1]).attributes["handler"],
            &element(&app.children[2]).attributes["handler"],
        ] {
            let StaticValue::Reference(reference) = value else {
                panic!("预期函数引用")
            };
            assert!(reference.module.ends_with("barrel.ts"));
            assert_eq!(reference.export, "run");
        }
        Ok(())
    }

    #[test]
    fn external_reexports_keep_local_facade_and_local_exports_keep_their_module() -> TestResult {
        let project = Project::new(&[
            (
                "app.tsx",
                "import { Card } from './barrel'; export function Local(){ return <div/>; } export function onClick(){} export default <app><Card callback={Card}/><Local onClick={onClick}/></app>;",
            ),
            (
                "barrel.ts",
                "export { createElement as Card } from 'react';",
            ),
        ])?;
        let program = project.program()?;
        let nodes = Evaluator::new(&program).entry(&project.entry()?)?;
        let app = element(&nodes[0]);
        let first = element(&app.children[0]);
        let reference = first.module_ref.as_ref().ok_or("缺少组件引用")?;
        assert!(reference.module.ends_with("barrel.ts"));
        assert_eq!(reference.export, "Card");
        assert!(!reference.external);
        assert_eq!(
            first.attributes["callback"],
            StaticValue::Reference(reference.clone())
        );
        let local = element(&app.children[1]);
        assert!(local.module_ref.as_ref().is_some_and(|reference| {
            reference.module.ends_with("app.tsx") && reference.export == "Local"
        }));
        assert!(
            matches!(&local.attributes["onClick"], StaticValue::Reference(reference) if reference.module.ends_with("app.tsx") && reference.export == "onClick")
        );
        Ok(())
    }

    #[test]
    fn nested_namespace_runtime_references_are_rejected_instead_of_losing_the_facade() -> TestResult
    {
        for source in [
            "import * as all from './barrel'; export default <app><all.ui.Card/></app>;",
            "import { ui } from './barrel'; export default <app><ui.Card/></app>;",
            "import { ui } from './barrel'; export default <app><action handler={ui.Card}/></app>;",
        ] {
            let project = Project::new(&[
                ("app.tsx", source),
                ("barrel.ts", "export * as ui from './ui';"),
                ("ui.tsx", "export function Card(){ return <div/>; }"),
            ])?;
            let program = project.program()?;
            let error = Evaluator::new(&program)
                .entry(&project.entry()?)
                .expect_err("应诊断未支持的成员链");
            assert_eq!(error.code, "KV1104");
            assert!(error.message.contains("嵌套模块成员"));
        }
        Ok(())
    }

    #[test]
    fn static_arrays_objects_members_and_ordered_object_spread_are_supported() -> TestResult {
        let project = Project::new(&[(
            "app.tsx",
            "const defaults={label:'旧', required:true}; const config={...defaults,label:'名称'}; const values=[1,...[2,3]]; export default <app name={config.label}><page path='/test'><number name='value' min={values[0]} max={values.length}/></page></app>;",
        )])?;
        let program = project.program()?;
        let nodes = Evaluator::new(&program).entry(&project.entry()?)?;
        let app = element(&nodes[0]);
        assert_eq!(
            app.attributes["name"],
            StaticValue::Json(Value::String("名称".into()))
        );
        let number = element(&element(&app.children[0]).children[0]);
        assert_eq!(number.attributes["min"], StaticValue::Json(Value::from(1)));
        assert_eq!(number.attributes["max"], StaticValue::Json(Value::from(3)));
        Ok(())
    }

    #[test]
    fn duplicate_attributes_and_conflicting_jsx_spreads_are_rejected() -> TestResult {
        for source in [
            "export default <app name='one' name='two'/>;",
            "const defaults={name:'one'}; export default <app {...defaults} name='two'/>;",
        ] {
            let project = Project::new(&[("app.tsx", source)])?;
            let program = project.program()?;
            let error = Evaluator::new(&program)
                .entry(&project.entry()?)
                .expect_err("应拒绝重复属性");
            assert_eq!(error.code, "KV1105");
        }
        Ok(())
    }

    #[test]
    fn constant_cycles_and_schema_cycles_report_their_locations() -> TestResult {
        let project =
            Project::new(&[("app.tsx", "const a=b, b=a; export default <app name={a}/>;")])?;
        let program = project.program()?;
        assert_eq!(
            Evaluator::new(&program)
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
        let error = Evaluator::new(&program)
            .entry(&project.entry()?)
            .expect_err("Schema 应循环");
        assert_eq!(error.code, "KV1102");
        assert!(!error.related.is_empty());
        Ok(())
    }

    #[test]
    fn dynamic_schema_mutation_getters_array_holes_and_function_roots_are_rejected() -> TestResult {
        for source in [
            "const config={name:'first'}; const alias=config; alias.name='changed'; export default <app name={config.name}/>;",
            "const values=[1]; values.push(2); export default <app name='test'/>;",
            "const config={get name(){return 'hidden'}}; export default <app name={config.name}/>;",
            "const values=[1,,3]; export default <app name={values[0]}/>;",
            "const field=()=> <text name='x'/>; export default <app>{field()}</app>;",
            "export default () => <app/>;",
            "let name='mutable'; export default <app name={name}/>;",
            "export default <app name={1/0}/>;",
        ] {
            let project = Project::new(&[("app.tsx", source)])?;
            let program = project.program()?;
            assert_eq!(
                Evaluator::new(&program)
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
            Evaluator::new(&program)
                .entry(&project.entry()?)
                .expect_err("不应忽略参数")
                .code,
            "KV1106"
        );
        Ok(())
    }

    #[test]
    fn imported_lowercase_components_cannot_masquerade_as_builtin_fields() -> TestResult {
        let project = Project::new(&[
            (
                "app.tsx",
                "import { text } from './ui'; export default <app><resource name='items'><text name='x'/></resource></app>;",
            ),
            ("ui.tsx", "export function text(){return <input/>;}"),
        ])?;
        let program = project.program()?;
        let nodes = Evaluator::new(&program).entry(&project.entry()?)?;
        let field = element(&element(&element(&nodes[0]).children[0]).children[0]);
        assert_eq!(field.name, "text");
        assert!(field.module_ref.as_ref().is_some_and(
            |reference| reference.export == "text" && reference.module.ends_with("ui.tsx")
        ));
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
                Evaluator::with_limits(&program, limits)
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
}
