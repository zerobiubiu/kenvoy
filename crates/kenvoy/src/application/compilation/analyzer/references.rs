use std::collections::BTreeSet;

use kengine::ModuleBindingTarget;

use crate::domain::{
    model::{
        ActionDefinition, ExecutionEnvironment, ModuleReference, ModuleReferenceKind, Operation,
        PermissionDefinition, PermissionTarget,
    },
    source::SourceLocation,
};

use super::super::{
    client_slice::ClientCodeTransformer,
    static_eval::{ExpressionReference, ResolvedReference, StaticElement, StaticValue},
};
use super::{
    Analyzer, Result,
    attributes::{attrs, error, intrinsic, no_children, required_string, role},
};

impl<T: ClientCodeTransformer> Analyzer<'_, T> {
    pub(super) fn register_module(
        &mut self,
        reference: &ResolvedReference,
        kind: ModuleReferenceKind,
        source: &SourceLocation,
        expression: Option<kengine::SourceSpan>,
    ) -> String {
        if let Some(existing) = self.modules.iter().find(|module| {
            module.module == reference.module
                && module.export == reference.export
                && module.kind == kind
                && module.expression == expression
        }) {
            return existing.id.clone();
        }
        let id = format!("module_{}", self.modules.len() + 1);
        self.modules.push(ModuleReference {
            id: id.clone(),
            module: reference.module.clone(),
            export: reference.export.clone(),
            external: reference.external,
            kind,
            expression,
            source: source.clone(),
        });
        id
    }

    pub(super) fn register_expression(
        &mut self,
        reference: &ExpressionReference,
        source: &SourceLocation,
    ) -> String {
        self.register_module(
            &ResolvedReference {
                module: reference.module.clone(),
                export: String::new(),
                external: false,
            },
            ModuleReferenceKind::ClientExpression,
            source,
            Some(reference.span.clone()),
        )
    }

    pub(super) fn override_definition(&mut self, element: &StaticElement) -> Result<()> {
        intrinsic(element)?;
        attrs(element, &["name", "component"])?;
        no_children(element)?;
        let name = required_string(element, "name")?;
        if !matches!(
            name.as_str(),
            "text"
                | "textarea"
                | "number"
                | "boolean"
                | "date"
                | "datetime"
                | "select"
                | "file"
                | "image"
                | "form"
                | "table"
                | "group"
                | "page"
        ) {
            return Err(error(
                element,
                "KV2016",
                "不支持的系统组件重载目标",
                "使用字段类型或 image/form/table/group/page",
            ));
        }
        if self.component_overrides.contains_key(&name) {
            return Err(error(
                element,
                "KV2005",
                format!("重复组件重载 {name}"),
                "每个系统组件只注册一个重载",
            ));
        }
        let reference = match element.attributes.get("component") {
            Some(StaticValue::Reference(reference))
                if reference.external || self.is_function(reference) =>
            {
                reference
            }
            _ => {
                return Err(error(
                    element,
                    "KV2016",
                    "重载 component 必须引用导出的 React 函数组件",
                    "传入组件引用，不传元素、对象、字符串或调用结果",
                ));
            }
        };
        let id = self.register_module(
            reference,
            ModuleReferenceKind::ClientComponent,
            &element.source,
            None,
        );
        self.component_overrides.insert(name, id);
        Ok(())
    }

    pub(super) fn action_definition(&mut self, element: &StaticElement) -> Result<()> {
        intrinsic(element)?;
        attrs(element, &["name", "handler", "runtime", "permission"])?;
        no_children(element)?;
        let name = required_string(element, "name")?;
        if !action_identifier(&name) {
            return Err(error(
                element,
                "KV2006",
                format!("Action 名称 {name} 非法"),
                "使用字母开头的 ASCII 字母、数字或下划线",
            ));
        }
        if let Some(previous) = self.actions.iter().find(|action| action.name == name) {
            let mut failure = error(
                element,
                "KV2005",
                format!("重复 Action {name}"),
                "每个 Action 使用唯一名称",
            );
            failure.related.push(previous.source.clone());
            return Err(failure);
        }
        let runtime = match required_string(element, "runtime")?.as_str() {
            "server" => ExecutionEnvironment::Server,
            "client" => ExecutionEnvironment::Client,
            "shared" => ExecutionEnvironment::Shared,
            _ => {
                return Err(error(
                    element,
                    "KV2004",
                    "Action runtime 必须为 server/client/shared",
                    "明确函数执行位置",
                ));
            }
        };
        let permission = role(element, true)?;
        let reference = match element.attributes.get("handler") {
            Some(StaticValue::Reference(reference)) => reference,
            _ => {
                return Err(error(
                    element,
                    "KV2012",
                    "Action handler 必须是可定位的模块导出函数引用",
                    "导入独立模块导出的纯函数，不能传字符串或执行函数调用",
                ));
            }
        };
        if reference.external && runtime != ExecutionEnvironment::Client {
            return Err(error(
                element,
                "KV2012",
                "服务端或共享 Action 不直接执行外部包函数",
                "使用受控本地模块导出可信纯函数",
            ));
        }
        if !reference.external && !self.is_function(reference) {
            return Err(error(
                element,
                "KV2012",
                "Action handler 导出不是函数声明或函数初始值",
                "导出普通函数或 const 箭头函数；不使用类、对象或运行时工厂",
            ));
        }
        let kind = match runtime {
            ExecutionEnvironment::Client => ModuleReferenceKind::ClientFunction,
            ExecutionEnvironment::Server => ModuleReferenceKind::ServerFunction,
            ExecutionEnvironment::Shared => ModuleReferenceKind::SharedFunction,
        };
        let handler = self.register_module(reference, kind, &element.source, None);
        self.permissions.push(PermissionDefinition {
            target: PermissionTarget::Action(name.clone()),
            operation: Operation::Execute,
            role: permission,
            fields: vec![],
            form: None,
        });
        self.actions.push(ActionDefinition {
            name,
            runtime,
            permission,
            handler,
            source: element.source.clone(),
        });
        Ok(())
    }

    fn is_function(&self, reference: &ResolvedReference) -> bool {
        let targets = self
            .program
            .module_bindings
            .exports
            .iter()
            .find(|export| {
                export.module == reference.module
                    && export.name == reference.export
                    && export.namespace == kengine::SymbolNamespace::Value
            })
            .map(|export| export.targets.as_slice())
            .unwrap_or_default();
        let [target] = targets else {
            return false;
        };
        match target {
            ModuleBindingTarget::Symbol { module, symbol } => self
                .program
                .modules
                .iter()
                .find(|item| &item.path == module)
                .is_some_and(|module| {
                    module.semantic.symbol(*symbol).is_some_and(|symbol| {
                        symbol.kind == kengine::SymbolKind::Function
                            || (symbol.kind == kengine::SymbolKind::Const
                                && module
                                    .declarations
                                    .iter()
                                    .flat_map(|decl| &decl.declarators)
                                    .any(|decl| {
                                        symbol.declarations.iter().any(|span| {
                                            decl.pattern.span.start <= span.start
                                                && decl.pattern.span.end >= span.end
                                        }) && decl
                                            .initializer
                                            .is_some_and(|id| function_expression(module, id))
                                    }))
                    })
                }),
            ModuleBindingTarget::Expression { module, expression } => self
                .program
                .modules
                .iter()
                .find(|item| &item.path == module)
                .is_some_and(|module| function_expression(module, *expression)),
            ModuleBindingTarget::Declaration { module, span } => self
                .program
                .modules
                .iter()
                .find(|item| &item.path == module)
                .is_some_and(|module| {
                    module.declarations.iter().any(|decl| {
                        decl.kind == kengine::DeclarationKind::Function
                            && decl.span.start >= span.start
                            && decl.span.end <= span.end
                    })
                }),
            _ => false,
        }
    }
}

fn function_expression(module: &kengine::ModuleIR, mut id: kengine::ExpressionId) -> bool {
    let mut seen = BTreeSet::new();
    while seen.insert(id) {
        let Some(expression) = module.expression(id) else {
            return false;
        };
        if matches!(expression.kind.as_str(), "ArrowFunction" | "Function") {
            return true;
        }
        if let kengine::ExpressionShape::Wrapper { expression, .. } = expression.shape {
            id = expression;
        } else {
            return false;
        }
    }
    false
}

fn action_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|ch| ch.is_ascii_alphabetic())
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}
