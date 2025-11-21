// src/parser/expr.rs
//
// Expression tokenizer and parser for Shrimpl v0.2.
// Supports:
// - numbers: 1, 2.5
// - strings: "hi"
// - variables: name, x1
// - binary ops: +, -, *, /
// - function calls: foo(a, b)
// - class method calls: Class.method(a, b)

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
                // string literal
                i += 1;
                let start = i;
                while i < chars.len() && chars[i] != '"' {
                    i += 1;
                }
                if i >= chars.len() {
                    return Err("Unterminated string literal".to_string());
                }
                let text: String = chars[start..i].iter().collect();
                i += 1; // consume closing quote
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

    pub fn parse_expr(&mut self) -> Result<Expr, String> {
        self.parse_add_sub()
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
                // Could be var, func call, or class.method call
                match self.peek() {
                    Some(TokKind::Dot) => {
                        // class method call: ClassName.method(args)
                        self.bump(); // consume '.'
                        let method_name = match self.bump() {
                            Some(TokKind::Ident(m)) => m,
                            other => {
                                return Err(format!(
                                    "Expected method name after '.', found {:?}",
                                    other
                                ))
                            }
                        };
                        // optional arg list
                        match self.peek() {
                            Some(TokKind::LParen) => {
                                self.bump(); // '('
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
                        // function call: name(args)
                        self.bump(); // '('
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
            other => Err(format!("Unexpected token in expression: {:?}", other)),
        }
    }

    fn parse_arg_list(&mut self) -> Result<Vec<Expr>, String> {
        let mut args = Vec::new();

        // empty arg list
        if matches!(self.peek(), Some(TokKind::RParen)) {
            self.bump(); // consume ')'
            return Ok(args);
        }

        loop {
            let expr = self.parse_expr()?;
            args.push(expr);

            match self.peek() {
                Some(TokKind::Comma) => {
                    self.bump(); // ','
                }
                Some(TokKind::RParen) => {
                    self.bump(); // ')'
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
