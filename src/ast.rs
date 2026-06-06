// src/parser/ast.rs
//
// Core AST types for Shrimpl programs.

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct ServerDecl {
    /// TCP port to listen on (e.g. 3000, 443).
    pub port: u16,
    /// Whether HTTPS/TLS is enabled (server 443 tls).
    pub tls: bool,
}

#[derive(Debug, Clone)]
pub enum Method {
    Get,
    Post,
}

#[derive(Debug, Clone)]
pub enum BinOp {
    // arithmetic
    Add,
    Sub,
    Mul,
    Div,
    // comparisons
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    // boolean logic
    And,
    Or,
}

#[derive(Debug, Clone)]
pub enum Expr {
    Number(f64),
    Str(String),
    Bool(bool),

    Var(String),

    /// First-class list literal:
    ///
    ///   [1, 2, "x"]
    ///
    /// is represented as:
    ///   List([Number(1.0), Number(2.0), Str("x".into())])
    List(Vec<Expr>),

    /// First-class map/dict literal:
    ///
    ///   { name: "Shrimpl", version: 0.5 }
    ///   { "owner": "Aisen", "year": 2025 }
    ///
    /// Keys are either identifiers or string literals, both normalized
    /// to String here.
    Map(Vec<(String, Expr)>),

    Binary {
        left: Box<Expr>,
        op: BinOp,
        right: Box<Expr>,
    },

    Call {
        name: String,
        args: Vec<Expr>,
    },

    MethodCall {
        class_name: String,
        method_name: String,
        args: Vec<Expr>,
    },

    /// if / elif / else as an expression
    ///
    /// Example:
    ///
    ///   if x > 0:
    ///       "positive"
    ///   elif x == 0:
    ///       "zero"
    ///   else:
    ///       "negative"
    ///
    /// is represented as:
    ///   branches = [(x > 0, "positive"), (x == 0, "zero")]
    ///   else_branch = Some("negative")
    If {
        branches: Vec<(Expr, Expr)>,
        else_branch: Option<Box<Expr>>,
    },

    /// Safe bounded loop:
    ///
    ///   repeat N times: body_expr
    ///
    /// Evaluates `count` once, coerces to integer N (floor), and executes
    /// `body` N times. String results are concatenated so repeat can build
    /// beginner-friendly text output without mutation. Non-string results
    /// return the last value (or "" if N == 0).
    Repeat {
        count: Box<Expr>,
        body: Box<Expr>,
    },

    /// try / catch / finally as an expression:
    ///
    ///   try:
    ///       expr1
    ///   catch err:
    ///       expr2
    ///   finally:
    ///       expr3
    ///
    /// Any of `catch_var`, `catch_body`, or `finally_body` can be None.
    /// Exceptions are runtime evaluation errors that would normally
    /// bubble up as a failed endpoint.
    Try {
        try_body: Box<Expr>,
        catch_var: Option<String>,
        catch_body: Option<Box<Expr>>,
        finally_body: Option<Box<Expr>>,
    },
}

#[derive(Debug, Clone)]
pub enum Body {
    TextExpr(Expr),
    JsonRaw(String),
}

#[derive(Debug, Clone)]
pub struct FunctionDef {
    pub name: String,
    pub params: Vec<String>,
    pub body: Expr,
}

#[derive(Debug, Clone)]
pub struct ClassDef {
    pub name: String,
    pub methods: HashMap<String, FunctionDef>,
}

/// Optional rate limit decoration for an endpoint, provided via
/// @rate_limit(max_requests, window_secs) or @rate_limit max window_secs.
#[derive(Debug, Clone)]
pub struct RateLimit {
    #[allow(dead_code)]
    pub max_requests: u32,
    #[allow(dead_code)]
    pub window_secs: u32,
}

#[derive(Debug, Clone)]
pub struct EndpointDecl {
    pub method: Method,
    pub path: String,
    pub body: Body,
    /// Optional rate limit applied to this endpoint.
    pub rate_limit: Option<RateLimit>,
}

/// Secret declarations, mapping a logical name used in Shrimpl code to an
/// underlying environment variable key (or other backend key).
///
/// Example Shrimpl:
///   secret OPENAI = "SHRIMPL_OPENAI_API_KEY"
#[derive(Debug, Clone)]
pub struct SecretDecl {
    pub name: String,
    pub key: String,
}

/// A single field inside a `model` declaration.
///
/// Example Shrimpl:
///
///   model User:
///     id: int pk
///     name: string
///     age?: int
///
/// Here:
///   - `id`  -> ty="int",   is_primary_key=true,  is_optional=false
///   - `name`-> ty="string",is_primary_key=false, is_optional=false
///   - `age` -> ty="int",   is_primary_key=false, is_optional=true
#[derive(Debug, Clone)]
pub struct ModelField {
    pub name: String,
    pub ty: String,
    pub is_primary_key: bool,
    pub is_optional: bool,
}

/// A model definition describing a table/entity for the ORM layer.
#[derive(Debug, Clone)]
pub struct ModelDef {
    /// Model name as written in Shrimpl (e.g. "User").
    pub name: String,
    /// Backing SQL table name (e.g. "users"). For now we just use `name`
    /// as-is; callers may downcase or pluralize.
    pub table_name: String,
    pub fields: Vec<ModelField>,
}

/// A single test case defined in Shrimpl source:
///
///   test "name":
///     assert <expr>
///     assert <expr>
#[derive(Debug, Clone)]
pub struct TestCase {
    #[allow(dead_code)]
    pub name: String,
    #[allow(dead_code)]
    pub assertions: Vec<Expr>,
}

#[derive(Debug, Clone)]
pub struct Program {
    pub server: ServerDecl,
    pub endpoints: Vec<EndpointDecl>,
    pub functions: HashMap<String, FunctionDef>,
    pub classes: HashMap<String, ClassDef>,
    pub secrets: Vec<SecretDecl>,
    /// All tests defined via `test "name": ...`.
    #[allow(dead_code)]
    pub tests: Vec<TestCase>,
    /// All `model` declarations keyed by model name.
    pub models: HashMap<String, ModelDef>,
}
