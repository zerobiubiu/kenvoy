//! 只补充 Generic IR 尚未建模的调用边；词法绑定复用 KEngine 的 SWC 作用域分析。
use std::collections::{BTreeMap, BTreeSet};

use kengine::{
    BindingPatternIR, BindingPatternKind, BindingPropertyIR, DeclarationKind, ExpressionId,
    ExpressionShape, ModuleIR, PropertyKeyIR, ReferenceKind, SymbolId, SymbolNamespace,
};
use swc_common::{Span, Spanned};
use swc_ecma_ast as ast;
use swc_ecma_visit::{Visit, VisitWith};

#[derive(Clone, Copy)]
pub(super) struct Argument {
    pub expression: Option<ExpressionId>,
    pub spread: bool,
}

pub(super) struct Call {
    pub expression: ExpressionId,
    pub callee: Option<ExpressionId>,
    pub arguments: Vec<Argument>,
}

pub(super) struct Definition<'a> {
    pub expression: ExpressionId,
    pub properties: Vec<&'a PropertyKeyIR>,
    pub constant: bool,
}

pub(super) struct Member {
    pub object: Option<ExpressionId>,
    pub name: Option<String>,
    pub computed: Option<ExpressionId>,
}

pub(super) struct Source<'a> {
    pub module: &'a ModuleIR,
    pub expressions: BTreeMap<(usize, usize), ExpressionId>,
    pub references: BTreeMap<(usize, usize), SymbolId>,
    pub declarations: BTreeMap<(usize, usize), SymbolId>,
    pub calls: BTreeMap<ExpressionId, Call>,
    pub definitions: BTreeMap<SymbolId, Definition<'a>>,
    pub members: BTreeMap<ExpressionId, Member>,
    pub sequences: BTreeMap<ExpressionId, ExpressionId>,
    parents: BTreeMap<ExpressionId, ExpressionId>,
    initializers: BTreeSet<ExpressionId>,
    current: Option<ExpressionId>,
    pub writes: Vec<bool>,
    pub base: u32,
    pub prefix: usize,
}

impl<'a> Source<'a> {
    pub fn new(module: &'a ModuleIR, base: u32) -> Self {
        let mut references = BTreeMap::new();
        let mut writes = vec![false; module.semantic.symbols.len()];
        for reference in &module.semantic.references {
            if reference.namespace != SymbolNamespace::Value {
                continue;
            }
            if let Some(symbol) = reference.symbol {
                references.insert((reference.span.start, reference.span.end), symbol);
                if matches!(
                    reference.kind,
                    ReferenceKind::Write | ReferenceKind::ReadWrite
                ) {
                    writes[symbol.0] = true;
                }
            }
        }
        let mut source = Self {
            module,
            expressions: module
                .expressions
                .iter()
                .map(|expression| ((expression.span.start, expression.span.end), expression.id))
                .collect(),
            references,
            declarations: module
                .semantic
                .symbols
                .iter()
                .filter(|symbol| symbol.namespace == SymbolNamespace::Value)
                .flat_map(|symbol| {
                    symbol
                        .declarations
                        .iter()
                        .map(move |span| ((span.start, span.end), symbol.id))
                })
                .collect(),
            calls: BTreeMap::new(),
            definitions: BTreeMap::new(),
            members: BTreeMap::new(),
            sequences: BTreeMap::new(),
            parents: BTreeMap::new(),
            initializers: BTreeSet::new(),
            current: None,
            writes,
            base,
            prefix: usize::from(module.source.starts_with('\u{feff}')) * 3,
        };
        for declaration in &module.declarations {
            for declarator in &declaration.declarators {
                if let Some(expression) = declarator.initializer {
                    source.bind(
                        &declarator.pattern,
                        expression,
                        &mut Vec::new(),
                        declaration.kind == DeclarationKind::Const,
                    );
                }
            }
        }
        source.initializers = source
            .definitions
            .values()
            .map(|definition| definition.expression)
            .collect();
        source
    }

    fn bind(
        &mut self,
        pattern: &'a BindingPatternIR,
        expression: ExpressionId,
        properties: &mut Vec<&'a PropertyKeyIR>,
        constant: bool,
    ) {
        match &pattern.kind {
            BindingPatternKind::Identifier(identifier) => {
                if let Some(symbol) = self
                    .declarations
                    .get(&(identifier.span.start, identifier.span.end))
                {
                    self.definitions.insert(
                        *symbol,
                        Definition {
                            expression,
                            properties: properties.clone(),
                            constant,
                        },
                    );
                }
            }
            BindingPatternKind::Object { properties: fields } => {
                for field in fields {
                    if let BindingPropertyIR::Property { key, value, .. } = field {
                        properties.push(key);
                        self.bind(value, expression, properties, constant);
                        properties.pop();
                    }
                }
            }
            BindingPatternKind::Assignment { target, .. } => {
                self.bind(target, expression, properties, constant);
            }
            _ => {}
        }
    }

    pub fn expression(&self, span: Span) -> Option<ExpressionId> {
        let start = span.lo.0.saturating_sub(self.base) as usize + self.prefix;
        let end = span.hi.0.saturating_sub(self.base) as usize + self.prefix;
        self.expressions.get(&(start, end)).copied()
    }

    pub fn symbol(&self, expression: ExpressionId) -> Option<SymbolId> {
        let span = &self
            .module
            .expression(expression)?
            .direct_identifier
            .as_ref()?
            .span;
        self.references.get(&(span.start, span.end)).copied()
    }

    pub fn permitted_use(&self, expression: ExpressionId) -> bool {
        if self.initializers.contains(&expression) {
            return true;
        }
        let Some(parent) = self.parents.get(&expression).copied() else {
            return false;
        };
        if self
            .calls
            .get(&parent)
            .is_some_and(|call| call.callee == Some(expression))
            || self
                .members
                .get(&parent)
                .is_some_and(|member| member.object == Some(expression))
            || self.sequences.contains_key(&parent)
        {
            return true;
        }
        self.module
            .expression(parent)
            .is_some_and(|parent| match &parent.shape {
                ExpressionShape::Wrapper {
                    expression: inner, ..
                } => *inner == expression,
                ExpressionShape::Member { object, .. } => *object == expression,
                ExpressionShape::Conditional {
                    consequent,
                    alternate,
                    ..
                } => *consequent == expression || *alternate == expression,
                _ => false,
            })
    }

    fn record(&mut self, span: Span, callee: Option<ExpressionId>, args: &[ast::ExprOrSpread]) {
        if let Some(expression) = self.expression(span) {
            let arguments = args
                .iter()
                .map(|argument| Argument {
                    expression: self.expression(argument.expr.span()),
                    spread: argument.spread.is_some(),
                })
                .collect();
            self.calls.insert(
                expression,
                Call {
                    expression,
                    callee,
                    arguments,
                },
            );
        }
    }
}

impl Visit for Source<'_> {
    fn visit_expr(&mut self, node: &ast::Expr) {
        let previous = self.current;
        if let Some(expression) = self.expression(node.span()) {
            if let Some(parent) = previous
                && parent != expression
            {
                self.parents.insert(expression, parent);
            }
            self.current = Some(expression);
        }
        node.visit_children_with(self);
        self.current = previous;
    }

    fn visit_call_expr(&mut self, node: &ast::CallExpr) {
        let callee = match &node.callee {
            ast::Callee::Expr(expression) => self.expression(expression.span()),
            _ => None,
        };
        self.record(node.span, callee, &node.args);
        node.visit_children_with(self);
    }

    fn visit_opt_chain_expr(&mut self, node: &ast::OptChainExpr) {
        if let ast::OptChainBase::Call(call) = node.base.as_ref() {
            self.record(node.span, self.expression(call.callee.span()), &call.args);
        }
        if let ast::OptChainBase::Member(member) = node.base.as_ref()
            && let Some(expression) = self.expression(node.span)
        {
            let (name, computed) = match &member.prop {
                ast::MemberProp::Ident(name) => (Some(name.sym.to_string()), None),
                ast::MemberProp::Computed(value) => (None, self.expression(value.expr.span())),
                ast::MemberProp::PrivateName(_) => (None, None),
            };
            self.members.insert(
                expression,
                Member {
                    object: self.expression(member.obj.span()),
                    name,
                    computed,
                },
            );
        }
        node.visit_children_with(self);
    }

    fn visit_seq_expr(&mut self, node: &ast::SeqExpr) {
        if let Some(expression) = self.expression(node.span)
            && let Some(last) = node
                .exprs
                .last()
                .and_then(|value| self.expression(value.span()))
        {
            self.sequences.insert(expression, last);
        }
        node.visit_children_with(self);
    }
}
