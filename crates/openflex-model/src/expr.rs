//! A small, safe expression language for triggers, routing conditions and
//! parameter overrides.
//!
//! FlexSim uses a full scripting language (FlexScript / C++) for object logic.
//! OpenFlex deliberately keeps the *semantics* (the same variables and the same
//! arithmetic/comparison behaviour) while using a tiny expression language that
//! cannot run arbitrary code. Expressions are parsed when the model is loaded,
//! so syntax errors surface in `openflex validate` instead of mid-run.

use serde::de::{self, Deserializer, Visitor};
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};
use std::fmt;

/// A parsed expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Number(f64),
    Bool(bool),
    Str(String),
    /// Variable path such as `time`, `item.type` or `Queue1.content`.
    Path(String),
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Number(f64),
    Bool(bool),
    Str(String),
}

impl Value {
    pub fn as_number(&self) -> Result<f64, String> {
        match self {
            Value::Number(n) => Ok(*n),
            Value::Bool(b) => Ok(u8::from(*b) as f64),
            Value::Str(s) => s
                .parse::<f64>()
                .map_err(|_| format!("expected a number, found string \"{s}\"")),
        }
    }

    pub fn as_bool(&self) -> bool {
        match self {
            Value::Number(n) => *n != 0.0 && !n.is_nan(),
            Value::Bool(b) => *b,
            Value::Str(s) => !s.is_empty(),
        }
    }

    pub fn as_str(&self) -> String {
        match self {
            Value::Number(n) => format_number(*n),
            Value::Bool(b) => b.to_string(),
            Value::Str(s) => s.clone(),
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Number(n) => write!(f, "{}", format_number(*n)),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Str(s) => write!(f, "{s}"),
        }
    }
}

fn format_number(n: f64) -> String {
    if n.is_finite() && n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{n:.0}")
    } else {
        format!("{n}")
    }
}

/// Everything the evaluator needs to resolve variables.
pub trait EvalContext {
    /// Resolve a dotted path (`time`, `item.type`, `Queue1.content`).
    fn lookup(&self, path: &str) -> Option<Value>;
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ExprError {
    #[error("unexpected end of expression")]
    UnexpectedEnd,
    #[error("unexpected character '{0}' at position {1}")]
    UnexpectedChar(char, usize),
    #[error("expected '{expected}' at position {pos}, found {found:?}")]
    Expected { expected: String, pos: usize, found: String },
    #[error("unknown function '{0}'")]
    UnknownFunction(String),
    #[error("wrong number of arguments for '{name}': expected {expected}, found {found}")]
    Arity { name: String, expected: usize, found: usize },
    #[error("cannot divide by zero")]
    DivideByZero,
    #[error("unknown variable '{0}'")]
    UnknownVariable(String),
    #[error("{0}")]
    TypeError(String),
}

impl From<String> for ExprError {
    fn from(message: String) -> Self {
        ExprError::TypeError(message)
    }
}

impl Expr {
    /// Parse an expression from source text.
    pub fn parse(source: &str) -> Result<Expr, ExprError> {
        let tokens = Lexer::new(source).tokenize()?;
        let mut parser = Parser { tokens, pos: 0 };
        let expr = parser.parse_expr(0)?;
        if parser.pos != parser.tokens.len() {
            return Err(ExprError::Expected {
                expected: "end of expression".into(),
                pos: parser.current_pos(),
                found: format!("{:?}", parser.peek()),
            });
        }
        Ok(expr)
    }

    /// Evaluate against a context. Arithmetic follows the usual rules; a
    /// comparison yields a boolean; `&&`/`||` short-circuit.
    pub fn eval(&self, ctx: &dyn EvalContext) -> Result<Value, ExprError> {
        match self {
            Expr::Number(n) => Ok(Value::Number(*n)),
            Expr::Bool(b) => Ok(Value::Bool(*b)),
            Expr::Str(s) => Ok(Value::Str(s.clone())),
            Expr::Path(path) => ctx
                .lookup(path)
                .ok_or_else(|| ExprError::UnknownVariable(path.clone())),
            Expr::Unary(op, inner) => {
                let value = inner.eval(ctx)?;
                match op {
                    UnOp::Neg => Ok(Value::Number(-value.as_number()?)),
                    UnOp::Not => Ok(Value::Bool(!value.as_bool())),
                }
            }
            Expr::Binary(op, left, right) => match op {
                BinOp::And => {
                    if !left.eval(ctx)?.as_bool() {
                        return Ok(Value::Bool(false));
                    }
                    Ok(Value::Bool(right.eval(ctx)?.as_bool()))
                }
                BinOp::Or => {
                    if left.eval(ctx)?.as_bool() {
                        return Ok(Value::Bool(true));
                    }
                    Ok(Value::Bool(right.eval(ctx)?.as_bool()))
                }
                _ => {
                    let a = left.eval(ctx)?;
                    let b = right.eval(ctx)?;
                    eval_binary(*op, &a, &b)
                }
            },
            Expr::Call(name, args) => eval_call(name, args, ctx),
        }
    }

    /// True when the expression only references the given paths (plus literals).
    /// Used by validation to spot misspelled variables early.
    pub fn referenced_paths(&self) -> Vec<String> {
        let mut out = Vec::new();
        self.collect_paths(&mut out);
        out
    }

    fn collect_paths(&self, out: &mut Vec<String>) {
        match self {
            Expr::Path(path) => out.push(path.clone()),
            Expr::Unary(_, inner) => inner.collect_paths(out),
            Expr::Binary(_, a, b) => {
                a.collect_paths(out);
                b.collect_paths(out);
            }
            Expr::Call(_, args) => {
                for arg in args {
                    arg.collect_paths(out);
                }
            }
            Expr::Number(_) | Expr::Bool(_) | Expr::Str(_) => {}
        }
    }
}

fn eval_binary(op: BinOp, a: &Value, b: &Value) -> Result<Value, ExprError> {
    match op {
        BinOp::Add => Ok(Value::Number(a.as_number()? + b.as_number()?)),
        BinOp::Sub => Ok(Value::Number(a.as_number()? - b.as_number()?)),
        BinOp::Mul => Ok(Value::Number(a.as_number()? * b.as_number()?)),
        BinOp::Div => {
            let divisor = b.as_number()?;
            if divisor == 0.0 {
                return Err(ExprError::DivideByZero);
            }
            Ok(Value::Number(a.as_number()? / divisor))
        }
        BinOp::Rem => {
            let divisor = b.as_number()?;
            if divisor == 0.0 {
                return Err(ExprError::DivideByZero);
            }
            Ok(Value::Number(a.as_number()? % divisor))
        }
        BinOp::Eq => Ok(Value::Bool(values_equal(a, b))),
        BinOp::Ne => Ok(Value::Bool(!values_equal(a, b))),
        BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
            let (x, y) = (a.as_number()?, b.as_number()?);
            Ok(Value::Bool(match op {
                BinOp::Lt => x < y,
                BinOp::Le => x <= y,
                BinOp::Gt => x > y,
                _ => x >= y,
            }))
        }
        BinOp::And | BinOp::Or => unreachable!("short-circuit operators are handled earlier"),
    }
}

fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        _ => match (a.as_number(), b.as_number()) {
            (Ok(x), Ok(y)) => (x - y).abs() < 1e-12,
            _ => false,
        },
    }
}

fn eval_call(name: &str, args: &[Expr], ctx: &dyn EvalContext) -> Result<Value, ExprError> {
    match name {
        "if" => {
            if args.len() != 3 {
                return Err(ExprError::Arity { name: "if".into(), expected: 3, found: args.len() });
            }
            if args[0].eval(ctx)?.as_bool() { args[1].eval(ctx) } else { args[2].eval(ctx) }
        }
        "min" | "max" => {
            if args.is_empty() {
                return Err(ExprError::Arity { name: name.into(), expected: 1, found: 0 });
            }
            let mut acc: Option<f64> = None;
            for arg in args {
                let value = arg.eval(ctx)?.as_number()?;
                acc = Some(match acc {
                    None => value,
                    Some(current) if name == "min" => current.min(value),
                    Some(current) => current.max(value),
                });
            }
            Ok(Value::Number(acc.unwrap_or(0.0)))
        }
        "abs" | "round" | "floor" | "ceil" | "sqrt" => {
            if args.len() != 1 {
                return Err(ExprError::Arity { name: name.into(), expected: 1, found: args.len() });
            }
            let value = args[0].eval(ctx)?.as_number()?;
            Ok(Value::Number(match name {
                "abs" => value.abs(),
                "round" => value.round(),
                "floor" => value.floor(),
                "ceil" => value.ceil(),
                _ => value.sqrt(),
            }))
        }
        "len" => {
            if args.len() != 1 {
                return Err(ExprError::Arity { name: "len".into(), expected: 1, found: args.len() });
            }
            Ok(Value::Number(args[0].eval(ctx)?.as_str().chars().count() as f64))
        }
        _ => Err(ExprError::UnknownFunction(name.to_string())),
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expr::Number(n) => write!(f, "{n}"),
            Expr::Bool(b) => write!(f, "{b}"),
            Expr::Str(s) => write!(f, "\"{s}\""),
            Expr::Path(p) => write!(f, "{p}"),
            Expr::Unary(op, inner) => {
                let sym = match op {
                    UnOp::Neg => "-",
                    UnOp::Not => "!",
                };
                write!(f, "{sym}{inner}")
            }
            Expr::Binary(op, a, b) => {
                let sym = match op {
                    BinOp::Add => "+",
                    BinOp::Sub => "-",
                    BinOp::Mul => "*",
                    BinOp::Div => "/",
                    BinOp::Rem => "%",
                    BinOp::Eq => "==",
                    BinOp::Ne => "!=",
                    BinOp::Lt => "<",
                    BinOp::Le => "<=",
                    BinOp::Gt => ">",
                    BinOp::Ge => ">=",
                    BinOp::And => "&&",
                    BinOp::Or => "||",
                };
                write!(f, "({a} {sym} {b})")
            }
            Expr::Call(name, args) => {
                write!(f, "{name}(")?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{arg}")?;
                }
                write!(f, ")")
            }
        }
    }
}

impl Serialize for Expr {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Expr {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ExprVisitor;
        impl Visitor<'_> for ExprVisitor {
            type Value = Expr;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an expression string")
            }
            fn visit_str<E: de::Error>(self, value: &str) -> Result<Expr, E> {
                Expr::parse(value).map_err(|err| E::custom(format!("invalid expression: {err}")))
            }
        }
        deserializer.deserialize_str(ExprVisitor)
    }
}

// ---------------------------------------------------------------------------
// Lexer
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f64),
    Str(String),
    Ident(String),
    True,
    False,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    EqEq,
    NotEq,
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
}

struct Lexer<'a> {
    chars: Vec<char>,
    pos: usize,
    _source: &'a str,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a str) -> Self {
        Self { chars: source.chars().collect(), pos: 0, _source: source }
    }

    fn tokenize(&mut self) -> Result<Vec<Token>, ExprError> {
        let mut tokens = Vec::new();
        while let Some(&c) = self.chars.get(self.pos) {
            match c {
                ' ' | '\t' | '\n' | '\r' => self.pos += 1,
                '0'..='9' => tokens.push(Token::Number(self.number()?)),
                '"' | '\'' => tokens.push(Token::Str(self.string(c)?)),
                '(' => {
                    tokens.push(Token::LParen);
                    self.pos += 1;
                }
                ')' => {
                    tokens.push(Token::RParen);
                    self.pos += 1;
                }
                ',' => {
                    tokens.push(Token::Comma);
                    self.pos += 1;
                }
                '+' => {
                    tokens.push(Token::Plus);
                    self.pos += 1;
                }
                '-' => {
                    tokens.push(Token::Minus);
                    self.pos += 1;
                }
                '*' => {
                    tokens.push(Token::Star);
                    self.pos += 1;
                }
                '/' => {
                    tokens.push(Token::Slash);
                    self.pos += 1;
                }
                '%' => {
                    tokens.push(Token::Percent);
                    self.pos += 1;
                }
                '=' => {
                    self.pos += 1;
                    if self.peek_char() == Some('=') {
                        tokens.push(Token::EqEq);
                        self.pos += 1;
                    } else {
                        return Err(ExprError::UnexpectedChar('=', self.pos - 1));
                    }
                }
                '!' => {
                    self.pos += 1;
                    if self.peek_char() == Some('=') {
                        tokens.push(Token::NotEq);
                        self.pos += 1;
                    } else {
                        tokens.push(Token::Not);
                    }
                }
                '<' => {
                    self.pos += 1;
                    if self.peek_char() == Some('=') {
                        tokens.push(Token::Le);
                        self.pos += 1;
                    } else {
                        tokens.push(Token::Lt);
                    }
                }
                '>' => {
                    self.pos += 1;
                    if self.peek_char() == Some('=') {
                        tokens.push(Token::Ge);
                        self.pos += 1;
                    } else {
                        tokens.push(Token::Gt);
                    }
                }
                '&' => {
                    self.pos += 1;
                    if self.peek_char() == Some('&') {
                        tokens.push(Token::And);
                        self.pos += 1;
                    } else {
                        return Err(ExprError::UnexpectedChar('&', self.pos - 1));
                    }
                }
                '|' => {
                    self.pos += 1;
                    if self.peek_char() == Some('|') {
                        tokens.push(Token::Or);
                        self.pos += 1;
                    } else {
                        return Err(ExprError::UnexpectedChar('|', self.pos - 1));
                    }
                }
                c if c.is_alphabetic() || c == '_' => {
                    let ident = self.identifier();
                    tokens.push(match ident.as_str() {
                        "true" => Token::True,
                        "false" => Token::False,
                        _ => Token::Ident(ident),
                    });
                }
                other => return Err(ExprError::UnexpectedChar(other, self.pos)),
            }
        }
        Ok(tokens)
    }

    fn peek_char(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn number(&mut self) -> Result<f64, ExprError> {
        let start = self.pos;
        while matches!(self.peek_char(), Some(c) if c.is_ascii_digit()) {
            self.pos += 1;
        }
        if self.peek_char() == Some('.') {
            self.pos += 1;
            while matches!(self.peek_char(), Some(c) if c.is_ascii_digit()) {
                self.pos += 1;
            }
        }
        if matches!(self.peek_char(), Some('e' | 'E')) {
            let mark = self.pos;
            self.pos += 1;
            if matches!(self.peek_char(), Some('+' | '-')) {
                self.pos += 1;
            }
            let digits = self.pos;
            while matches!(self.peek_char(), Some(c) if c.is_ascii_digit()) {
                self.pos += 1;
            }
            if digits == self.pos {
                self.pos = mark;
            }
        }
        let text: String = self.chars[start..self.pos].iter().collect();
        text.parse::<f64>().map_err(|_| ExprError::UnexpectedChar('?', start))
    }

    fn string(&mut self, quote: char) -> Result<String, ExprError> {
        self.pos += 1;
        let mut out = String::new();
        while let Some(&c) = self.chars.get(self.pos) {
            if c == quote {
                self.pos += 1;
                return Ok(out);
            }
            if c == '\\' {
                self.pos += 1;
                match self.chars.get(self.pos) {
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some('"') => out.push('"'),
                    Some('\'') => out.push('\''),
                    Some('\\') => out.push('\\'),
                    Some(other) => out.push(*other),
                    None => return Err(ExprError::UnexpectedEnd),
                }
                self.pos += 1;
            } else {
                out.push(c);
                self.pos += 1;
            }
        }
        Err(ExprError::UnexpectedEnd)
    }

    fn identifier(&mut self) -> String {
        let start = self.pos;
        while let Some(&c) = self.chars.get(self.pos) {
            if c.is_alphanumeric() || c == '_' || c == '.' {
                self.pos += 1;
            } else {
                break;
            }
        }
        self.chars[start..self.pos].iter().collect()
    }
}

// ---------------------------------------------------------------------------
// Parser (precedence climbing)
// ---------------------------------------------------------------------------

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn current_pos(&self) -> usize {
        self.pos
    }

    fn next(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.pos).cloned();
        if token.is_some() {
            self.pos += 1;
        }
        token
    }

    fn parse_expr(&mut self, min_precedence: u8) -> Result<Expr, ExprError> {
        let mut left = self.parse_unary()?;
        while let Some(op) = self.peek_binary_op() {
            let precedence = precedence(op);
            if precedence < min_precedence {
                break;
            }
            self.next();
            let right = self.parse_expr(precedence + 1)?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn peek_binary_op(&self) -> Option<BinOp> {
        Some(match self.peek()? {
            Token::Or => BinOp::Or,
            Token::And => BinOp::And,
            Token::EqEq => BinOp::Eq,
            Token::NotEq => BinOp::Ne,
            Token::Lt => BinOp::Lt,
            Token::Le => BinOp::Le,
            Token::Gt => BinOp::Gt,
            Token::Ge => BinOp::Ge,
            Token::Plus => BinOp::Add,
            Token::Minus => BinOp::Sub,
            Token::Star => BinOp::Mul,
            Token::Slash => BinOp::Div,
            Token::Percent => BinOp::Rem,
            _ => return None,
        })
    }

    fn parse_unary(&mut self) -> Result<Expr, ExprError> {
        match self.peek() {
            Some(Token::Minus) => {
                self.next();
                Ok(Expr::Unary(UnOp::Neg, Box::new(self.parse_unary()?)))
            }
            Some(Token::Not) => {
                self.next();
                Ok(Expr::Unary(UnOp::Not, Box::new(self.parse_unary()?)))
            }
            _ => self.parse_primary(),
        }
    }

    fn parse_primary(&mut self) -> Result<Expr, ExprError> {
        match self.next() {
            Some(Token::Number(n)) => Ok(Expr::Number(n)),
            Some(Token::Str(s)) => Ok(Expr::Str(s)),
            Some(Token::True) => Ok(Expr::Bool(true)),
            Some(Token::False) => Ok(Expr::Bool(false)),
            Some(Token::Ident(name)) => {
                if matches!(self.peek(), Some(Token::LParen)) {
                    self.next();
                    let mut args = Vec::new();
                    if !matches!(self.peek(), Some(Token::RParen)) {
                        loop {
                            args.push(self.parse_expr(0)?);
                            match self.next() {
                                Some(Token::Comma) => continue,
                                Some(Token::RParen) => break,
                                other => {
                                    return Err(ExprError::Expected {
                                        expected: "')'".into(),
                                        pos: self.pos,
                                        found: format!("{other:?}"),
                                    });
                                }
                            }
                        }
                    } else {
                        self.next();
                    }
                    Ok(Expr::Call(name, args))
                } else {
                    Ok(Expr::Path(name))
                }
            }
            Some(Token::LParen) => {
                let inner = self.parse_expr(0)?;
                match self.next() {
                    Some(Token::RParen) => Ok(inner),
                    other => Err(ExprError::Expected {
                        expected: "')'".into(),
                        pos: self.pos,
                        found: format!("{other:?}"),
                    }),
                }
            }
            None => Err(ExprError::UnexpectedEnd),
            Some(other) => Err(ExprError::Expected {
                expected: "an expression".into(),
                pos: self.pos,
                found: format!("{other:?}"),
            }),
        }
    }
}

fn precedence(op: BinOp) -> u8 {
    match op {
        BinOp::Or => 1,
        BinOp::And => 2,
        BinOp::Eq | BinOp::Ne => 3,
        BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => 4,
        BinOp::Add | BinOp::Sub => 5,
        BinOp::Mul | BinOp::Div | BinOp::Rem => 6,
    }
}

/// Convenience: parse a boolean condition such as `item.type == 1 && time > 10`.
pub fn condition(source: &str) -> Result<Expr, ExprError> {
    Expr::parse(source)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    struct Ctx(BTreeMap<String, Value>);
    impl EvalContext for Ctx {
        fn lookup(&self, path: &str) -> Option<Value> {
            self.0.get(path).cloned()
        }
    }

    fn ctx() -> Ctx {
        let mut map = BTreeMap::new();
        map.insert("time".into(), Value::Number(12.5));
        map.insert("item.type".into(), Value::Number(2.0));
        map.insert("Queue1.content".into(), Value::Number(3.0));
        Ctx(map)
    }

    #[test]
    fn parses_and_evaluates_arithmetic() {
        let expr = Expr::parse("1 + 2 * 3 - 4 / 2").unwrap();
        assert_eq!(expr.eval(&ctx()).unwrap(), Value::Number(5.0));
    }

    #[test]
    fn comparisons_and_short_circuit() {
        let expr = Expr::parse("item.type == 2 && (time > 10 || false)").unwrap();
        assert_eq!(expr.eval(&ctx()).unwrap(), Value::Bool(true));
        let expr = Expr::parse("Queue1.content >= 3 && item.type != 3").unwrap();
        assert_eq!(expr.eval(&ctx()).unwrap(), Value::Bool(true));
    }

    #[test]
    fn functions() {
        let expr = Expr::parse("if(item.type == 2, max(1, 5), min(2, 3))").unwrap();
        assert_eq!(expr.eval(&ctx()).unwrap(), Value::Number(5.0));
    }

    #[test]
    fn roundtrips_through_serde() {
        let expr = Expr::parse("time > 10 && item.type == 2").unwrap();
        let json = serde_json::to_string(&expr).unwrap();
        let back: Expr = serde_json::from_str(&json).unwrap();
        assert_eq!(expr, back);
    }

    #[test]
    fn unknown_variable_is_an_error() {
        let expr = Expr::parse("nope + 1").unwrap();
        assert!(matches!(expr.eval(&ctx()), Err(ExprError::UnknownVariable(_))));
    }
}
