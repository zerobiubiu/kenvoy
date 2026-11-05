//! 翻译能力沿词法 const 与已链接 ESM 目标传递；普通同名函数不获得翻译身份。
use std::collections::{BTreeMap, BTreeSet};

use kengine::{ExpressionId, ExpressionShape, MemberPropertyIR, PropertyKeyIR, SymbolId};

use super::source::Source;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Expression {
    pub module: usize,
    pub id: ExpressionId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Origin {
    Other,
    Sdk,
    Module(usize),
    Hook,
    Context,
    Translate,
    Unresolved,
}

impl Origin {
    pub fn capability(self) -> bool {
        matches!(
            self,
            Self::Sdk | Self::Hook | Self::Context | Self::Translate | Self::Unresolved
        )
    }
}

#[derive(Clone, Copy)]
pub(super) enum Target {
    Other,
    Sdk,
    Hook,
    Module(usize),
    Symbol(usize, SymbolId),
    Expression(Expression),
}

pub(super) struct Resolver<'a> {
    pub sources: &'a [Source<'a>],
    pub imports: BTreeMap<(usize, SymbolId), Target>,
    pub exports: BTreeMap<(usize, &'a str), Target>,
    pub(super) origins: BTreeMap<Expression, Origin>,
    pub(super) pending_origins: BTreeSet<Expression>,
    pub pending_values: BTreeSet<Expression>,
}

impl<'a> Resolver<'a> {
    pub fn origin(&mut self, expression: Expression) -> Origin {
        self.origin_inner(expression, &mut Vec::new())
    }

    fn origin_inner(&mut self, expression: Expression, stack: &mut Vec<Expression>) -> Origin {
        if let Some(origin) = self.origins.get(&expression) {
            return *origin;
        }
        if stack.len() >= 128 || !self.pending_origins.insert(expression) {
            return Origin::Unresolved;
        }
        stack.push(expression);
        let origin = self.expression_origin(expression, stack);
        stack.pop();
        self.origins.insert(expression, origin);
        self.pending_origins.remove(&expression);
        origin
    }

    fn expression_origin(&mut self, expression: Expression, stack: &mut Vec<Expression>) -> Origin {
        let source = &self.sources[expression.module];
        let Some(node) = source.module.expression(expression.id) else {
            return Origin::Unresolved;
        };
        let child = |id| Expression { id, ..expression };
        match &node.shape {
            ExpressionShape::Identifier => source
                .symbol(expression.id)
                .map_or(Origin::Other, |symbol| {
                    self.symbol_origin(expression.module, symbol, stack)
                }),
            ExpressionShape::Wrapper {
                expression: inner, ..
            } => self.origin_inner(child(*inner), stack),
            ExpressionShape::Member { object, property } => {
                let origin = self.origin_inner(child(*object), stack);
                let name = self.member_name(expression.module, property);
                self.member_origin(origin, name.as_deref(), stack)
            }
            ExpressionShape::Conditional {
                test,
                consequent,
                alternate,
            } => {
                let branches = self.branches(child(*test));
                match branches {
                    Some(true) => self.origin_inner(child(*consequent), stack),
                    Some(false) => self.origin_inner(child(*alternate), stack),
                    None => {
                        let left = self.origin_inner(child(*consequent), stack);
                        let right = self.origin_inner(child(*alternate), stack);
                        if left == right {
                            left
                        } else if self.member_origin(left, None, stack).capability()
                            || self.member_origin(right, None, stack).capability()
                        {
                            Origin::Unresolved
                        } else {
                            Origin::Other
                        }
                    }
                }
            }
            _ => {
                if let Some(last) = source.sequences.get(&expression.id) {
                    return self.origin_inner(child(*last), stack);
                }
                if let Some(member) = source.members.get(&expression.id) {
                    let origin = member
                        .object
                        .map_or(Origin::Other, |id| self.origin_inner(child(id), stack));
                    let name = member
                        .name
                        .clone()
                        .or_else(|| member.computed.and_then(|id| self.single_string(child(id))));
                    return self.member_origin(origin, name.as_deref(), stack);
                }
                if let Some(call) = source.calls.get(&expression.id) {
                    let origin = call
                        .callee
                        .map_or(Origin::Other, |id| self.origin_inner(child(id), stack));
                    return match origin {
                        Origin::Hook => Origin::Context,
                        Origin::Unresolved => Origin::Unresolved,
                        _ if call.arguments.iter().any(|argument| {
                            argument
                                .expression
                                .is_some_and(|id| self.origin_inner(child(id), stack).capability())
                        }) =>
                        {
                            Origin::Unresolved
                        }
                        _ => Origin::Other,
                    };
                }
                Origin::Other
            }
        }
    }

    pub fn symbol_origin(
        &mut self,
        module: usize,
        symbol: SymbolId,
        stack: &mut Vec<Expression>,
    ) -> Origin {
        if let Some(target) = self.imports.get(&(module, symbol)).copied() {
            return self.target_origin(target, stack);
        }
        let source = &self.sources[module];
        let Some(definition) = source.definitions.get(&symbol) else {
            return Origin::Other;
        };
        let mut origin = self.origin_inner(
            Expression {
                module,
                id: definition.expression,
            },
            stack,
        );
        for property in &definition.properties {
            let name = self.property_name(module, property);
            origin = self.member_origin(origin, name.as_deref(), stack);
        }
        if source.writes[symbol.0] && self.member_origin(origin, None, stack).capability() {
            Origin::Unresolved
        } else {
            origin
        }
    }

    fn target_origin(&mut self, target: Target, stack: &mut Vec<Expression>) -> Origin {
        match target {
            Target::Other => Origin::Other,
            Target::Hook => Origin::Hook,
            Target::Sdk => Origin::Sdk,
            Target::Module(module) => Origin::Module(module),
            Target::Symbol(module, symbol) => self.symbol_origin(module, symbol, stack),
            Target::Expression(expression) => self.origin_inner(expression, stack),
        }
    }

    fn member_origin(
        &mut self,
        origin: Origin,
        name: Option<&str>,
        stack: &mut Vec<Expression>,
    ) -> Origin {
        match (origin, name) {
            (Origin::Sdk, Some("useI18n")) => Origin::Hook,
            (Origin::Context, Some("t")) => Origin::Translate,
            (Origin::Sdk | Origin::Context, None) => Origin::Unresolved,
            (Origin::Translate | Origin::Hook | Origin::Unresolved, _) => Origin::Unresolved,
            (Origin::Module(module), Some(name)) => self
                .exports
                .get(&(module, name))
                .copied()
                .map_or(Origin::Other, |target| self.target_origin(target, stack)),
            (Origin::Module(module), None) => {
                let targets: Vec<_> = self
                    .exports
                    .iter()
                    .filter_map(|((owner, _), target)| (*owner == module).then_some(*target))
                    .collect();
                if targets
                    .into_iter()
                    .any(|target| self.target_origin(target, stack) != Origin::Other)
                {
                    Origin::Unresolved
                } else {
                    Origin::Other
                }
            }
            _ => Origin::Other,
        }
    }

    pub fn member_name(&mut self, module: usize, property: &MemberPropertyIR) -> Option<String> {
        match property {
            MemberPropertyIR::Identifier(name) => Some(name.name.clone()),
            MemberPropertyIR::Computed { expression, .. } => self.single_string(Expression {
                module,
                id: *expression,
            }),
            MemberPropertyIR::Private(_) => None,
        }
    }

    pub fn property_name(&mut self, module: usize, property: &PropertyKeyIR) -> Option<String> {
        match property {
            PropertyKeyIR::Identifier(name) => Some(name.name.clone()),
            PropertyKeyIR::String(value) => value.to_string().ok(),
            PropertyKeyIR::Computed { expression, .. } => self.single_string(Expression {
                module,
                id: *expression,
            }),
            _ => None,
        }
    }
}
