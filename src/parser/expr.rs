// src/parser/expr.rs
//
// Expression tokenizer and parser for Shrimpl.
// Supports:
// - numbers: 1, 2.5
// - strings: "hi"
// - booleans: true, false
// - variables: name, x1
// - binary ops: +, -, *, /, ==, !=, <, <=, >, >=, and, or
// - function calls: foo(a, b)
// - class method calls: Class.method(a, b)
// - if / elif / else expressions
// - repeat N times: expr loop expressions
// - list literals: [1, 2, "x"]
// - map literals: { key: 1, "other": 2 }
// - try / catch / finally expressions (single-line style)

use super::ast::{BinOp, Expr};

// Token kinds for expression parsing
#[derive(Debug, Clone)]
enum TokKind {
    Number(f64),
    Str(String),
    Ident(String),

    Plus,
    Minus,
    Star,
    Slash,

    LParen,
    RParen,
    Comma,
    Dot,
    Colon,

    EqEq,
    BangEq,
    Lt,
    Le,
    Gt,
    Ge,

    // list/map literals
    LBracket,
    RBracket,
    LBrace,
    RBrace,
}

#[derive(Debug, Clone)]
struct Token {
    kind: TokKind,
}

fn tokenize_expr(s: &str) -> Result<Vec<Token>, String> {
    let chars: Vec<char> = s.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        if c.is_whitespace() {
            i += 1;
            continue;
        }

        if c.is_ascii_digit() {
            let start = i;
            i += 1;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            let value: f64 = text
                .parse()
                .map_err(|_| format!("Invalid number literal '{}'", text))?;
            tokens.push(Token {
                kind: TokKind::Number(value),
            });
            continue;
        }

        match c {
            '"' => {
                i += 1;
                let start = i;
                while i < chars.len() && chars[i] != '"' {
                    i += 1;
                }
                if i >= chars.len() {
                    return Err("Unterminated string literal".to_string());
                }
                let text: String = chars[start..i].iter().collect();
                i += 1;
                tokens.push(Token {
                    kind: TokKind::Str(text),
                });
            }
            '+' => {
                tokens.push(Token {
                    kind: TokKind::Plus,
                });
                i += 1;
            }
            '-' => {
                tokens.push(Token {
                    kind: TokKind::Minus,
                });
                i += 1;
            }
            '*' => {
                tokens.push(Token {
                    kind: TokKind::Star,
                });
                i += 1;
            }
            '/' => {
                tokens.push(Token {
                    kind: TokKind::Slash,
                });
                i += 1;
            }
            '(' => {
                tokens.push(Token {
                    kind: TokKind::LParen,
                });
                i += 1;
            }
            ')' => {
                tokens.push(Token {
                    kind: TokKind::RParen,
                });
                i += 1;
            }
            ',' => {
                tokens.push(Token {
                    kind: TokKind::Comma,
                });
                i += 1;
            }
            '.' => {
                tokens.push(Token { kind: TokKind::Dot });
                i += 1;
            }
            ':' => {
                tokens.push(Token {
                    kind: TokKind::Colon,
                });
                i += 1;
            }
            '=' => {
                if i + 1 < chars.len() && chars[i + 1] == '=' {
                    tokens.push(Token {
                        kind: TokKind::EqEq,
                    });
                    i += 2;
                } else {
                    return Err(
                        "Unexpected '=' in expression; use '==' for equality comparisons"
                            .to_string(),
                    );
                }
            }
            '!' => {
                if i + 1 < chars.len() && chars[i + 1] == '=' {
                    tokens.push(Token {
                        kind: TokKind::BangEq,
                    });
                    i += 2;
                } else {
                    return Err(
                        "Unexpected '!' in expression; use '!=' for inequality comparisons"
                            .to_string(),
                    );
                }
            }
            '<' => {
                if i + 1 < chars.len() && chars[i + 1] == '=' {
                    tokens.push(Token { kind: TokKind::Le });
                    i += 2;
                } else {
                    tokens.push(Token { kind: TokKind::Lt });
                    i += 1;
                }
            }
            '>' => {
                if i + 1 < chars.len() && chars[i + 1] == '=' {
                    tokens.push(Token { kind: TokKind::Ge });
                    i += 2;
                } else {
                    tokens.push(Token { kind: TokKind::Gt });
                    i += 1;
                }
            }
            '[' => {
                tokens.push(Token {
                    kind: TokKind::LBracket,
                });
                i += 1;
            }
            ']' => {
                tokens.push(Token {
                    kind: TokKind::RBracket,
                });
                i += 1;
            }
            '{' => {
                tokens.push(Token {
                    kind: TokKind::LBrace,
                });
                i += 1;
            }
            '}' => {
                tokens.push(Token {
                    kind: TokKind::RBrace,
                });
                i += 1;
            }
            _ => {
                if c.is_ascii_alphabetic() || c == '_' {
                    let start = i;
                    i += 1;
                    while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                        i += 1;
                    }
                    let ident: String = chars[start..i].iter().collect();
                    tokens.push(Token {
                        kind: TokKind::Ident(ident),
                    });
                } else {
                    return Err(format!("Unexpected character '{}' in expression", c));
                }
            }
        }
    }

    Ok(tokens)
}

struct ExprParser {
    tokens: Vec<Token>,
    pos: usize,
}

impl ExprParser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> Option<&TokKind> {
        self.tokens.get(self.pos).map(|t| &t.kind)
    }

    fn bump(&mut self) -> Option<TokKind> {
        if self.pos < self.tokens.len() {
            let kind = self.tokens[self.pos].kind.clone();
            self.pos += 1;
            Some(kind)
        } else {
            None
        }
    }

    fn peek_ident(&self) -> Option<&str> {
        match self.peek() {
            Some(TokKind::Ident(name)) => Some(name.as_str()),
            _ => None,
        }
    }

    fn expect_colon(&mut self, ctx: &str) -> Result<(), String> {
        match self.bump() {
            Some(TokKind::Colon) => Ok(()),
            other => Err(format!(
                "Expected ':' after {} condition, found {:?}",
                ctx, other
            )),
        }
    }

    pub fn parse_expr(&mut self) -> Result<Expr, String> {
        // Top-level entry: special-case if / repeat / try when they lead the expression.
        if let Some(name) = self.peek_ident() {
            if name == "if" {
                return self.parse_if_expr();
            } else if name == "repeat" {
                return self.parse_repeat_expr();
            } else if name == "try" {
                return self.parse_try_expr();
            }
        }

        self.parse_or()
    }

    // or-expression:  a or b or c
    fn parse_or(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_and()?;

        loop {
            let is_or = matches!(self.peek(), Some(TokKind::Ident(name)) if name == "or");

            if !is_or {
                break;
            }

            self.bump(); // 'or'
            let right = self.parse_and()?;
            expr = Expr::Binary {
                left: Box::new(expr),
                op: BinOp::Or,
                right: Box::new(right),
            };
        }

        Ok(expr)
    }

    // and-expression:  a and b and c
    fn parse_and(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_comparison()?;

        loop {
            let is_and = matches!(self.peek(), Some(TokKind::Ident(name)) if name == "and");

            if !is_and {
                break;
            }

            self.bump(); // 'and'
            let right = self.parse_comparison()?;
            expr = Expr::Binary {
                left: Box::new(expr),
                op: BinOp::And,
                right: Box::new(right),
            };
        }

        Ok(expr)
    }

    // comparison-expression:  a == b, a != b, a < b, etc.
    fn parse_comparison(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_add_sub()?;

        loop {
            let op = match self.peek() {
                Some(TokKind::EqEq) => BinOp::Eq,
                Some(TokKind::BangEq) => BinOp::Ne,
                Some(TokKind::Lt) => BinOp::Lt,
                Some(TokKind::Le) => BinOp::Le,
                Some(TokKind::Gt) => BinOp::Gt,
                Some(TokKind::Ge) => BinOp::Ge,
                _ => break,
            };

            self.bump();

            let right = self.parse_add_sub()?;
            expr = Expr::Binary {
                left: Box::new(expr),
                op,
                right: Box::new(right),
            };
        }

        Ok(expr)
    }

    fn parse_add_sub(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_mul_div()?;

        loop {
            match self.peek() {
                Some(TokKind::Plus) => {
                    self.bump();
                    let right = self.parse_mul_div()?;
                    expr = Expr::Binary {
                        left: Box::new(expr),
                        op: BinOp::Add,
                        right: Box::new(right),
                    };
                }
                Some(TokKind::Minus) => {
                    self.bump();
                    let right = self.parse_mul_div()?;
                    expr = Expr::Binary {
                        left: Box::new(expr),
                        op: BinOp::Sub,
                        right: Box::new(right),
                    };
                }
                _ => break,
            }
        }

        Ok(expr)
    }

    fn parse_mul_div(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_factor()?;

        loop {
            match self.peek() {
                Some(TokKind::Star) => {
                    self.bump();
                    let right = self.parse_factor()?;
                    expr = Expr::Binary {
                        left: Box::new(expr),
                        op: BinOp::Mul,
                        right: Box::new(right),
                    };
                }
                Some(TokKind::Slash) => {
                    self.bump();
                    let right = self.parse_factor()?;
                    expr = Expr::Binary {
                        left: Box::new(expr),
                        op: BinOp::Div,
                        right: Box::new(right),
                    };
                }
                _ => break,
            }
        }

        Ok(expr)
    }

    fn parse_factor(&mut self) -> Result<Expr, String> {
        match self.bump() {
            Some(TokKind::Number(n)) => Ok(Expr::Number(n)),
            Some(TokKind::Str(s)) => Ok(Expr::Str(s)),
            Some(TokKind::Ident(name)) => {
                if name == "true" {
                    return Ok(Expr::Bool(true));
                } else if name == "false" {
                    return Ok(Expr::Bool(false));
                }

                match self.peek() {
                    Some(TokKind::Dot) => {
                        // ClassName.method(args)
                        self.bump(); // '.'
                        let method_name = match self.bump() {
                            Some(TokKind::Ident(m)) => m,
                            other => {
                                return Err(format!(
                                    "Expected method name after '.', found {:?}",
                                    other
                                ))
                            }
                        };
                        match self.peek() {
                            Some(TokKind::LParen) => {
                                self.bump();
                                let args = self.parse_arg_list()?;
                                Ok(Expr::MethodCall {
                                    class_name: name,
                                    method_name,
                                    args,
                                })
                            }
                            _ => Err("Expected '(' after method name".to_string()),
                        }
                    }
                    Some(TokKind::LParen) => {
                        self.bump();
                        let args = self.parse_arg_list()?;
                        Ok(Expr::Call { name, args })
                    }
                    _ => Ok(Expr::Var(name)),
                }
            }
            Some(TokKind::LParen) => {
                let expr = self.parse_expr()?;
                match self.bump() {
                    Some(TokKind::RParen) => Ok(expr),
                    other => Err(format!("Expected ')', found {:?}", other)),
                }
            }
            Some(TokKind::LBracket) => self.parse_list_literal(),
            Some(TokKind::LBrace) => self.parse_map_literal(),
            other => Err(format!("Unexpected token in expression: {:?}", other)),
        }
    }

    fn parse_arg_list(&mut self) -> Result<Vec<Expr>, String> {
        let mut args = Vec::new();

        if matches!(self.peek(), Some(TokKind::RParen)) {
            self.bump();
            return Ok(args);
        }

        loop {
            let expr = self.parse_expr()?;
            args.push(expr);

            match self.peek() {
                Some(TokKind::Comma) => {
                    self.bump();
                }
                Some(TokKind::RParen) => {
                    self.bump();
                    break;
                }
                other => {
                    return Err(format!(
                        "Expected ',' or ')' in argument list, found {:?}",
                        other
                    ))
                }
            }
        }

        Ok(args)
    }

    /// Parse list literal: [expr1, expr2, ...]
    fn parse_list_literal(&mut self) -> Result<Expr, String> {
        let mut items = Vec::new();

        if matches!(self.peek(), Some(TokKind::RBracket)) {
            self.bump();
            return Ok(Expr::List(items));
        }

        loop {
            let expr = self.parse_expr()?;
            items.push(expr);

            match self.peek() {
                Some(TokKind::Comma) => {
                    self.bump();
                }
                Some(TokKind::RBracket) => {
                    self.bump();
                    break;
                }
                other => {
                    return Err(format!(
                        "Expected ',' or ']' in list literal, found {:?}",
                        other
                    ))
                }
            }
        }

        Ok(Expr::List(items))
    }

    /// Parse map literal: { key: expr, "other": expr2, ... }
    fn parse_map_literal(&mut self) -> Result<Expr, String> {
        let mut entries: Vec<(String, Expr)> = Vec::new();

        if matches!(self.peek(), Some(TokKind::RBrace)) {
            self.bump();
            return Ok(Expr::Map(entries));
        }

        loop {
            let key = match self.bump() {
                Some(TokKind::Ident(name)) => name,
                Some(TokKind::Str(s)) => s,
                other => {
                    return Err(format!(
                        "Expected identifier or string as map key, found {:?}",
                        other
                    ))
                }
            };

            match self.bump() {
                Some(TokKind::Colon) => {}
                other => {
                    return Err(format!(
                        "Expected ':' after map key '{}', found {:?}",
                        key, other
                    ))
                }
            }

            let value_expr = self.parse_expr()?;
            entries.push((key, value_expr));

            match self.peek() {
                Some(TokKind::Comma) => {
                    self.bump();
                }
                Some(TokKind::RBrace) => {
                    self.bump();
                    break;
                }
                other => {
                    return Err(format!(
                        "Expected ',' or '}}' in map literal, found {:?}",
                        other
                    ))
                }
            }
        }

        Ok(Expr::Map(entries))
    }

    /// Parse `if` / `elif` / `else` expression.
    fn parse_if_expr(&mut self) -> Result<Expr, String> {
        match self.bump() {
            Some(TokKind::Ident(name)) if name == "if" => {}
            other => {
                return Err(format!(
                    "Internal parser error: expected 'if', found {:?}",
                    other
                ))
            }
        }

        let first_cond = self.parse_or()?;
        self.expect_colon("if")?;
        let first_expr = self.parse_expr()?;

        let mut branches = Vec::new();
        branches.push((first_cond, first_expr));

        let mut else_branch: Option<Box<Expr>> = None;

        loop {
            match self.peek_ident() {
                Some("elif") => {
                    self.bump();
                    let cond = self.parse_or()?;
                    self.expect_colon("elif")?;
                    let body = self.parse_expr()?;
                    branches.push((cond, body));
                }
                Some("else") => {
                    self.bump();
                    self.expect_colon("else")?;
                    let body = self.parse_expr()?;
                    else_branch = Some(Box::new(body));
                    break;
                }
                _ => break,
            }
        }

        Ok(Expr::If {
            branches,
            else_branch,
        })
    }

    /// Parse `repeat N times: expr`.
    fn parse_repeat_expr(&mut self) -> Result<Expr, String> {
        match self.bump() {
            Some(TokKind::Ident(name)) if name == "repeat" => {}
            other => {
                return Err(format!(
                    "Internal parser error: expected 'repeat', found {:?}",
                    other
                ))
            }
        }

        let count_expr = self.parse_or()?;

        match self.peek() {
            Some(TokKind::Ident(name)) if name == "times" => {
                self.bump();
            }
            other => {
                return Err(format!(
                    "Expected 'times' after repeat-count expression, found {:?}",
                    other
                ))
            }
        }

        self.expect_colon("repeat")?;
        let body_expr = self.parse_expr()?;

        Ok(Expr::Repeat {
            count: Box::new(count_expr),
            body: Box::new(body_expr),
        })
    }

    /// Parse `try`/`catch`/`finally` expression (single-line Shrimpl style):
    ///
    ///   try:
    ///     expr1
    ///   catch err:
    ///     expr2
    ///   finally:
    ///     expr3
    fn parse_try_expr(&mut self) -> Result<Expr, String> {
        match self.bump() {
            Some(TokKind::Ident(name)) if name == "try" => {}
            other => {
                return Err(format!(
                    "Internal parser error: expected 'try', found {:?}",
                    other
                ))
            }
        }

        self.expect_colon("try")?;
        let try_body = self.parse_expr()?;

        let mut catch_var: Option<String> = None;
        let mut catch_body: Option<Box<Expr>> = None;
        let mut finally_body: Option<Box<Expr>> = None;

        loop {
            match self.peek_ident() {
                Some("catch") => {
                    self.bump();

                    let var_name = match self.peek() {
                        Some(TokKind::Ident(_)) => {
                            if let Some(TokKind::Ident(n)) = self.bump() {
                                Some(n)
                            } else {
                                None
                            }
                        }
                        _ => None,
                    };

                    self.expect_colon("catch")?;
                    let body = self.parse_expr()?;
                    catch_var = var_name;
                    catch_body = Some(Box::new(body));
                }
                Some("finally") => {
                    self.bump();
                    self.expect_colon("finally")?;
                    let body = self.parse_expr()?;
                    finally_body = Some(Box::new(body));
                    break;
                }
                _ => break,
            }
        }

        Ok(Expr::Try {
            try_body: Box::new(try_body),
            catch_var,
            catch_body,
            finally_body,
        })
    }
}

pub fn parse_expr(s: &str) -> Result<Expr, String> {
    let tokens = tokenize_expr(s)?;
    let mut parser = ExprParser::new(tokens);
    let expr = parser.parse_expr()?;
    if parser.pos != parser.tokens.len() {
        return Err("Unexpected tokens after end of expression".to_string());
    }
    Ok(expr)
}
