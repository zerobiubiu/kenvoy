//! 表达式引擎：对 DSL 中的条件与绑定表达式求值。
//!
//! 支持：中文标识符、点路径变量、比较/算术/逻辑运算、`empty()` 函数、`非空` 后缀。
//! 用于表单校验 `expr`、审批流条件 `expr` 与 `{{ }}` 绑定求值。
//! 纯函数、无 I/O，可 WASM 化。

use std::collections::HashMap;
use std::fmt;

/// 求值过程中的值。
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// 空值。
    Null,
    /// 布尔。
    Bool(bool),
    /// 数字。
    Number(f64),
    /// 字符串。
    String(String),
    /// 嵌套对象（点路径）。
    Object(HashMap<String, Value>),
    /// 数组。
    Array(Vec<Value>),
}

impl Value {
    /// 判断真值：`Null` / `false` / `0` / 空串 / 空数组 / 空对象 为假，其余为真。
    #[must_use]
    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Null => false,
            Value::Bool(b) => *b,
            Value::Number(n) => *n != 0.0,
            Value::String(s) => !s.is_empty(),
            Value::Object(m) => !m.is_empty(),
            Value::Array(a) => !a.is_empty(),
        }
    }

    /// 是否为空：`empty()` 语义。
    #[must_use]
    pub fn is_empty_value(&self) -> bool {
        match self {
            Value::Null => true,
            Value::String(s) => s.is_empty(),
            Value::Array(a) => a.is_empty(),
            Value::Object(m) => m.is_empty(),
            _ => false,
        }
    }

    /// 转字符串（用于渲染绑定、更新值）。整数数字输出不带小数点。
    #[must_use]
    pub fn to_display_string(&self) -> String {
        match self {
            Value::Null | Value::Object(_) | Value::Array(_) => String::new(),
            Value::Bool(b) => b.to_string(),
            Value::Number(n) => n.to_string(),
            Value::String(s) => s.clone(),
        }
    }

    /// 尝试转为数字（数字、数字字符串、布尔）。
    fn as_number(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            Value::String(s) => s.trim().parse::<f64>().ok(),
            Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
            _ => None,
        }
    }
}

/// 求值上下文：变量表，键支持点路径。
#[derive(Debug, Clone, Default)]
pub struct Context {
    vars: HashMap<String, Value>,
}

impl Context {
    /// 创建空上下文。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 插入变量；`key` 支持点路径（如 `currentUser.name` 存为嵌套对象）。
    pub fn insert(&mut self, key: &str, value: Value) {
        Self::insert_path(&mut self.vars, key, value);
    }

    /// 从扁平映射构建上下文（表单提交数据等）。
    #[must_use]
    pub fn from_map(map: &HashMap<String, Value>) -> Self {
        let mut ctx = Self::new();
        for (k, v) in map {
            ctx.insert(k, v.clone());
        }
        ctx
    }

    /// 按点路径取值；缺失返回 [`Value::Null`]。
    #[must_use]
    pub fn get(&self, path: &str) -> Value {
        if let Some(v) = self.vars.get(path) {
            return v.clone();
        }
        let mut parts = path.split('.');
        let first = parts.next().unwrap_or_default();
        let mut cur = self.vars.get(first).cloned().unwrap_or(Value::Null);
        for part in parts {
            cur = match cur {
                Value::Object(m) => m.get(part).cloned().unwrap_or(Value::Null),
                _ => Value::Null,
            };
        }
        cur
    }

    fn insert_path(vars: &mut HashMap<String, Value>, key: &str, value: Value) {
        let mut parts = key.split('.');
        let first = parts.next().unwrap_or_default();
        if first.is_empty() {
            return;
        }
        let rest: Vec<&str> = parts.collect();
        if rest.is_empty() {
            vars.insert(first.to_string(), value);
            return;
        }
        let entry = vars
            .entry(first.to_string())
            .or_insert_with(|| Value::Object(HashMap::new()));
        if let Value::Object(m) = entry {
            Self::insert_path(m, &rest.join("."), value);
        } else {
            let mut m = HashMap::new();
            Self::insert_path(&mut m, &rest.join("."), value);
            *entry = Value::Object(m);
        }
    }
}

/// 表达式错误。
#[derive(Debug, Clone, PartialEq)]
pub struct ExprError {
    /// 错误信息。
    pub message: String,
}

impl ExprError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ExprError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ExprError {}

/// 求值表达式，返回 [`Value`]。
///
/// # Errors
/// 表达式语法错误或类型错误时返回 [`ExprError`]。
pub fn eval(expr: &str, ctx: &Context) -> Result<Value, ExprError> {
    let tokens = Lexer::new(expr).tokenize()?;
    let mut parser = Parser { tokens, pos: 0 };
    let value = parser.parse_expr(ctx)?;
    if parser.pos != parser.tokens.len() {
        return Err(ExprError::new("表达式存在未消费的 token"));
    }
    Ok(value)
}

/// 求值表达式并判定真值。
///
/// # Errors
/// 见 [`eval`]。
pub fn eval_bool(expr: &str, ctx: &Context) -> Result<bool, ExprError> {
    Ok(eval(expr, ctx)?.is_truthy())
}

/// 将文本中的 `{{ 表达式 }}` 绑定替换为求值结果；无绑定则原样返回。
///
/// # Errors
/// 绑定表达式语法错误或未闭合时返回 [`ExprError`]。
pub fn resolve_bindings(text: &str, ctx: &Context) -> Result<String, ExprError> {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            return Err(ExprError::new("未闭合的 `{{` 绑定"));
        };
        let inner = &after[..end];
        let val = eval(inner.trim(), ctx)?;
        out.push_str(&val.to_display_string());
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    Ok(out)
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f64),
    Str(String),
    Ident(String),
    True,
    False,
    NotEmpty,
    Plus,
    Minus,
    Star,
    Slash,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    Not,
    LParen,
    RParen,
    Comma,
    Dot,
}

struct Lexer<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        Self { src, pos: 0 }
    }

    fn tokenize(&mut self) -> Result<Vec<Token>, ExprError> {
        let mut tokens = Vec::new();
        while let Some(c) = self.peek_char() {
            if c.is_whitespace() {
                self.pos += c.len_utf8();
                continue;
            }
            if c.is_ascii_digit() || (c == '.' && self.peek_next_is_digit()) {
                tokens.push(Token::Number(self.read_number()?));
                continue;
            }
            match c {
                '+' => {
                    self.pos += 1;
                    tokens.push(Token::Plus);
                }
                '-' => {
                    self.pos += 1;
                    tokens.push(Token::Minus);
                }
                '*' => {
                    self.pos += 1;
                    tokens.push(Token::Star);
                }
                '/' => {
                    self.pos += 1;
                    tokens.push(Token::Slash);
                }
                '(' => {
                    self.pos += 1;
                    tokens.push(Token::LParen);
                }
                ')' => {
                    self.pos += 1;
                    tokens.push(Token::RParen);
                }
                ',' => {
                    self.pos += 1;
                    tokens.push(Token::Comma);
                }
                '.' => {
                    self.pos += 1;
                    tokens.push(Token::Dot);
                }
                '"' | '\'' => {
                    tokens.push(Token::Str(self.read_string(c)?));
                }
                '<' => {
                    self.pos += 1;
                    tokens.push(if self.eat('=') { Token::Le } else { Token::Lt });
                }
                '>' => {
                    self.pos += 1;
                    tokens.push(if self.eat('=') { Token::Ge } else { Token::Gt });
                }
                '=' => {
                    self.pos += 1;
                    // `==` 与 `=` 都视为相等
                    self.eat('=');
                    tokens.push(Token::Eq);
                }
                '!' => {
                    self.pos += 1;
                    tokens.push(if self.eat('=') { Token::Ne } else { Token::Not });
                }
                '&' => {
                    self.pos += 1;
                    if !self.eat('&') {
                        return Err(ExprError::new("`&` 需成对出现（`&&`）"));
                    }
                    tokens.push(Token::And);
                }
                '|' => {
                    self.pos += 1;
                    if !self.eat('|') {
                        return Err(ExprError::new("`|` 需成对出现（`||`）"));
                    }
                    tokens.push(Token::Or);
                }
                _ => {
                    let ident = self.read_ident();
                    tokens.push(match ident.as_str() {
                        "true" | "真" => Token::True,
                        "false" | "假" => Token::False,
                        "非空" => Token::NotEmpty,
                        "且" => Token::And,
                        "或" => Token::Or,
                        _ => Token::Ident(ident),
                    });
                }
            }
        }
        Ok(tokens)
    }

    fn peek_char(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }

    fn peek_next_is_digit(&self) -> bool {
        self.src[self.pos + 1..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit())
    }

    fn eat(&mut self, expect: char) -> bool {
        if self.peek_char() == Some(expect) {
            self.pos += expect.len_utf8();
            true
        } else {
            false
        }
    }

    fn read_number(&mut self) -> Result<f64, ExprError> {
        let start = self.pos;
        while let Some(c) = self.peek_char() {
            if c.is_ascii_digit() || c == '.' {
                self.pos += c.len_utf8();
            } else {
                break;
            }
        }
        self.src[start..self.pos]
            .parse::<f64>()
            .map_err(|_| ExprError::new(format!("无效数字 `{}`", &self.src[start..self.pos])))
    }

    fn read_string(&mut self, quote: char) -> Result<String, ExprError> {
        self.pos += 1;
        let start = self.pos;
        while let Some(c) = self.peek_char() {
            if c == quote {
                let s = self.src[start..self.pos].to_string();
                self.pos += 1;
                return Ok(s);
            }
            self.pos += c.len_utf8();
        }
        Err(ExprError::new("字符串未闭合"))
    }

    fn read_ident(&mut self) -> String {
        let start = self.pos;
        while let Some(c) = self.peek_char() {
            if is_delim(c) {
                break;
            }
            self.pos += c.len_utf8();
        }
        self.src[start..self.pos].to_string()
    }
}

fn is_delim(c: char) -> bool {
    c.is_whitespace()
        || matches!(
            c,
            '+' | '-'
                | '*'
                | '/'
                | '<'
                | '>'
                | '='
                | '!'
                | '&'
                | '|'
                | '('
                | ')'
                | ','
                | '.'
                | '"'
                | '\''
        )
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn next(&mut self) -> Option<Token> {
        let t = self.tokens.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    fn parse_expr(&mut self, ctx: &Context) -> Result<Value, ExprError> {
        self.parse_or(ctx)
    }

    fn parse_or(&mut self, ctx: &Context) -> Result<Value, ExprError> {
        let mut left = self.parse_and(ctx)?;
        while self.peek() == Some(&Token::Or) {
            self.next();
            if left.is_truthy() {
                let _ = self.parse_and(ctx)?; // 短路：仍消费右侧
            } else {
                left = self.parse_and(ctx)?;
            }
        }
        Ok(left)
    }

    fn parse_and(&mut self, ctx: &Context) -> Result<Value, ExprError> {
        let mut left = self.parse_cmp(ctx)?;
        while self.peek() == Some(&Token::And) {
            self.next();
            if left.is_truthy() {
                left = self.parse_cmp(ctx)?;
            } else {
                let _ = self.parse_cmp(ctx)?;
            }
        }
        Ok(left)
    }

    fn parse_cmp(&mut self, ctx: &Context) -> Result<Value, ExprError> {
        let left = self.parse_add(ctx)?;
        let op = match self.peek() {
            Some(Token::Eq) => Some("=="),
            Some(Token::Ne) => Some("!="),
            Some(Token::Lt) => Some("<"),
            Some(Token::Le) => Some("<="),
            Some(Token::Gt) => Some(">"),
            Some(Token::Ge) => Some(">="),
            _ => None,
        };
        let Some(op) = op else {
            return Ok(left);
        };
        self.next();
        let right = self.parse_add(ctx)?;
        apply_cmp(op, &left, &right).map_err(ExprError::new)
    }

    fn parse_add(&mut self, ctx: &Context) -> Result<Value, ExprError> {
        let mut left = self.parse_mul(ctx)?;
        loop {
            let op = match self.peek() {
                Some(Token::Plus) => "+",
                Some(Token::Minus) => "-",
                _ => break,
            };
            self.next();
            let right = self.parse_mul(ctx)?;
            left = apply_arith(op, &left, &right).map_err(ExprError::new)?;
        }
        Ok(left)
    }

    fn parse_mul(&mut self, ctx: &Context) -> Result<Value, ExprError> {
        let mut left = self.parse_unary(ctx)?;
        loop {
            let op = match self.peek() {
                Some(Token::Star) => "*",
                Some(Token::Slash) => "/",
                _ => break,
            };
            self.next();
            let right = self.parse_unary(ctx)?;
            left = apply_arith(op, &left, &right).map_err(ExprError::new)?;
        }
        Ok(left)
    }

    fn parse_unary(&mut self, ctx: &Context) -> Result<Value, ExprError> {
        match self.peek() {
            Some(Token::Minus) => {
                self.next();
                let v = self.parse_unary(ctx)?;
                let n = v
                    .as_number()
                    .ok_or_else(|| ExprError::new("`-` 只能作用于数字"))?;
                Ok(Value::Number(-n))
            }
            Some(Token::Not) => {
                self.next();
                let v = self.parse_unary(ctx)?;
                Ok(Value::Bool(!v.is_truthy()))
            }
            _ => self.parse_primary(ctx),
        }
    }

    fn parse_primary(&mut self, ctx: &Context) -> Result<Value, ExprError> {
        let value = match self.next() {
            Some(Token::Number(n)) => Value::Number(n),
            Some(Token::Str(s)) => Value::String(s),
            Some(Token::True) => Value::Bool(true),
            Some(Token::False) => Value::Bool(false),
            Some(Token::LParen) => {
                let v = self.parse_expr(ctx)?;
                match self.next() {
                    Some(Token::RParen) => v,
                    _ => return Err(ExprError::new("括号未闭合")),
                }
            }
            Some(Token::Ident(name)) => {
                if self.peek() == Some(&Token::LParen) {
                    self.next();
                    let args = self.parse_args(ctx)?;
                    apply_function(&name, &args).map_err(ExprError::new)?
                } else {
                    let mut path = name;
                    while self.peek() == Some(&Token::Dot) {
                        self.next();
                        let Some(Token::Ident(seg)) = self.next() else {
                            return Err(ExprError::new("点路径后缺少字段名"));
                        };
                        path.push('.');
                        path.push_str(&seg);
                    }
                    ctx.get(&path)
                }
            }
            Some(Token::NotEmpty) => return Err(ExprError::new("`非空` 需跟在表达式之后")),
            other => {
                return Err(ExprError::new(format!(
                    "意外的 token：{other:?}（位置 {}）",
                    self.pos
                )));
            }
        };
        // 后缀 `非空`
        if self.peek() == Some(&Token::NotEmpty) {
            self.next();
            Ok(Value::Bool(!value.is_empty_value()))
        } else {
            Ok(value)
        }
    }

    fn parse_args(&mut self, ctx: &Context) -> Result<Vec<Value>, ExprError> {
        let mut args = Vec::new();
        if self.peek() == Some(&Token::RParen) {
            self.next();
            return Ok(args);
        }
        loop {
            args.push(self.parse_expr(ctx)?);
            match self.next() {
                Some(Token::Comma) => {}
                Some(Token::RParen) => return Ok(args),
                _ => return Err(ExprError::new("函数参数期望 `,` 或 `)`")),
            }
        }
    }
}

fn apply_cmp(op: &str, left: &Value, right: &Value) -> Result<Value, String> {
    match op {
        "==" => Ok(Value::Bool(left == right)),
        "!=" => Ok(Value::Bool(left != right)),
        "<" | "<=" | ">" | ">=" => {
            if let (Some(a), Some(b)) = (left.as_number(), right.as_number()) {
                Ok(Value::Bool(match op {
                    "<" => a < b,
                    "<=" => a <= b,
                    ">" => a > b,
                    _ => a >= b,
                }))
            } else if let (Value::String(a), Value::String(b)) = (left, right) {
                Ok(Value::Bool(match op {
                    "<" => a < b,
                    "<=" => a <= b,
                    ">" => a > b,
                    _ => a >= b,
                }))
            } else {
                Err(format!("无法比较 `{left:?}` 与 `{right:?}`"))
            }
        }
        _ => Err(format!("未知比较运算符 `{op}`")),
    }
}

fn apply_arith(op: &str, left: &Value, right: &Value) -> Result<Value, String> {
    let (Some(a), Some(b)) = (left.as_number(), right.as_number()) else {
        return Err(format!("`{op}` 只能作用于数字：`{left:?}` `{right:?}`"));
    };
    match op {
        "+" => Ok(Value::Number(a + b)),
        "-" => Ok(Value::Number(a - b)),
        "*" => Ok(Value::Number(a * b)),
        "/" => {
            if b == 0.0 {
                Err("除以零".to_string())
            } else {
                Ok(Value::Number(a / b))
            }
        }
        _ => Err(format!("未知算术运算符 `{op}`")),
    }
}

fn apply_function(name: &str, args: &[Value]) -> Result<Value, String> {
    match name {
        "empty" => {
            if args.len() != 1 {
                return Err("`empty` 需要一个参数".to_string());
            }
            Ok(Value::Bool(args[0].is_empty_value()))
        }
        _ => Err(format!("未知函数 `{name}`")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> Context {
        let mut c = Context::new();
        c.insert("请假天数", Value::Number(2.0));
        c.insert("请假类型", Value::String("年假".into()));
        c.insert("事由", Value::String("家中有事".into()));
        c.insert("审批状态", Value::String("待审批".into()));
        c.insert("currentUser.name", Value::String("张三".into()));
        c
    }

    #[test]
    fn compare_number() {
        let c = ctx();
        assert!(eval_bool("请假天数 > 0", &c).unwrap());
        assert!(eval_bool("请假天数 <= 3", &c).unwrap());
        assert!(!eval_bool("请假天数 > 3", &c).unwrap());
        assert!(eval_bool("请假天数 == 2", &c).unwrap());
        assert!(eval_bool("请假天数 != 3", &c).unwrap());
    }

    #[test]
    fn compare_string_and_boolean() {
        let c = ctx();
        assert!(eval_bool("请假类型 == \"年假\"", &c).unwrap());
        assert!(eval_bool("审批状态 == \"待审批\"", &c).unwrap());
        assert!(eval_bool("true", &c).unwrap());
        assert!(!eval_bool("假", &c).unwrap());
    }

    #[test]
    fn arithmetic_and_logic() {
        let c = ctx();
        assert_eq!(eval("请假天数 + 1", &c).unwrap(), Value::Number(3.0));
        assert!(eval_bool("请假天数 >= 1 && 请假天数 <= 3", &c).unwrap());
        assert!(eval_bool("请假天数 < 1 或 请假天数 > 0", &c).unwrap());
        assert!(!eval_bool("!true", &c).unwrap());
    }

    #[test]
    fn empty_and_not_empty() {
        let c = ctx();
        assert!(!eval_bool("empty(事由)", &c).unwrap());
        assert!(eval_bool("事由 非空", &c).unwrap());
        let mut c2 = Context::new();
        c2.insert("事由", Value::String(String::new()));
        assert!(eval_bool("empty(事由)", &c2).unwrap());
        assert!(!eval_bool("事由 非空", &c2).unwrap());
        assert!(eval_bool("empty(未知字段)", &c).unwrap());
    }

    #[test]
    fn dot_path_and_bindings() {
        let c = ctx();
        assert_eq!(
            eval("currentUser.name", &c).unwrap(),
            Value::String("张三".into())
        );
        assert_eq!(
            resolve_bindings("你好，{{currentUser.name}}", &c).unwrap(),
            "你好，张三"
        );
        assert_eq!(resolve_bindings("无绑定文本", &c).unwrap(), "无绑定文本");
    }

    #[test]
    fn malformed_expressions_error() {
        let c = ctx();
        assert!(eval("请假天数 >", &c).is_err());
        assert!(eval("", &c).is_err());
        assert!(eval("(", &c).is_err());
        assert!(eval("unknown_func(x)", &c).is_err());
        assert!(resolve_bindings("{{ 未闭合", &c).is_err());
        assert!(eval("请假天数 && 1", &c).is_ok()); // 真值逻辑
    }
}
