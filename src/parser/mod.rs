// src/parser/mod.rs
//
// Shrimpl v0.5 parser (line-based).
// Features:
// - server <port> [tls]
// - endpoint METHOD "/path"[: <body>]
//   Body can be on same line after colon or next non-empty line.
//   Body is either:
//     - Text expression (variables, +, -, *, /, calls, class.method calls,
//       including OpenAI helpers like `openai_chat("...")` and
//       `openai_set_system_prompt("...")`)
//     - JSON: json { "message": "Hello" }  (treated as raw JSON string)
// - func name(a, b): expr
// - class Name:
//     method(a, b): expr
// - secret NAME = "ENV_VAR_NAME"
//   (logical secret mapping used by the `secret(...)` builtin)
// - @rate_limit(max, window_secs) before an endpoint
//   (or `@rate_limit max window_secs`)
// - test "name":
//     assert <expr>
//     assert <expr>
//   (built-in test cases)
// - model Name:
//     field: type [pk]
//     field?: type [pk]
//
// Path parameters are written as "/hello/:name" (converted later in interpreter).
// Lines starting with '#' (after trimming) are comments and are ignored.

pub mod ast;
pub mod expr;

use self::ast::{
    Body, ClassDef, EndpointDecl, FunctionDef, Method, ModelDef, ModelField, Program, RateLimit,
    SecretDecl, ServerDecl, TestCase,
};
use self::expr::parse_expr;

use std::collections::HashMap;

// Public entry point
pub fn parse_program(source: &str) -> Result<Program, String> {
    let lines: Vec<&str> = source.lines().collect();
    let mut i: usize = 0;

    let mut server: Option<ServerDecl> = None;
    let mut endpoints: Vec<EndpointDecl> = Vec::new();
    let mut functions: HashMap<String, FunctionDef> = HashMap::new();
    let mut classes: HashMap<String, ClassDef> = HashMap::new();
    let mut secrets: Vec<SecretDecl> = Vec::new();
    let mut tests: Vec<TestCase> = Vec::new();
    let mut models: HashMap<String, ModelDef> = HashMap::new();

    // Pending attributes that apply to the *next* endpoint encountered.
    let mut pending_rate_limit: Option<RateLimit> = None;

    while i < lines.len() {
        let raw_line = lines[i];
        let trimmed = raw_line.trim();

        // Skip blank lines and comments
        if trimmed.is_empty() || trimmed.starts_with('#') {
            i += 1;
            continue;
        }

        if trimmed.starts_with("server") {
            if server.is_some() {
                return Err(format!(
                    "Line {}: only one 'server' declaration is allowed",
                    i + 1
                ));
            }
            if pending_rate_limit.is_some() {
                return Err(format!(
                    "Line {}: @rate_limit attribute can only be applied to an 'endpoint' declaration",
                    i + 1
                ));
            }
            server = Some(parse_server_line(trimmed, i + 1)?);
            i += 1;
        } else if trimmed.starts_with("@rate_limit") {
            // Attribute that decorates the next endpoint.
            let rl = parse_rate_limit_line(trimmed, i + 1)?;
            if pending_rate_limit.is_some() {
                return Err(format!(
                    "Line {}: multiple @rate_limit attributes before a single endpoint",
                    i + 1
                ));
            }
            pending_rate_limit = Some(rl);
            i += 1;
        } else if trimmed.starts_with("endpoint") {
            let (mut ep, next_index) = parse_endpoint(&lines, i)?;
            ep.rate_limit = pending_rate_limit.take();
            endpoints.push(ep);
            i = next_index;
        } else if trimmed.starts_with("func ") {
            if pending_rate_limit.is_some() {
                return Err(format!(
                    "Line {}: @rate_limit can only precede an 'endpoint' declaration",
                    i + 1
                ));
            }
            let func = parse_func_line(trimmed, i + 1)?;
            if functions.contains_key(&func.name) {
                return Err(format!(
                    "Line {}: function '{}' already defined",
                    i + 1,
                    func.name
                ));
            }
            functions.insert(func.name.clone(), func);
            i += 1;
        } else if trimmed.starts_with("class ") {
            if pending_rate_limit.is_some() {
                return Err(format!(
                    "Line {}: @rate_limit can only precede an 'endpoint' declaration",
                    i + 1
                ));
            }
            let (class_def, next_index) = parse_class(&lines, i)?;
            if classes.contains_key(&class_def.name) {
                return Err(format!(
                    "Line {}: class '{}' already defined",
                    i + 1,
                    class_def.name
                ));
            }
            classes.insert(class_def.name.clone(), class_def);
            i = next_index;
        } else if trimmed.starts_with("secret ") {
            if pending_rate_limit.is_some() {
                return Err(format!(
                    "Line {}: @rate_limit can only precede an 'endpoint' declaration",
                    i + 1
                ));
            }
            let secret = parse_secret_line(trimmed, i + 1)?;
            secrets.push(secret);
            i += 1;
        } else if trimmed.starts_with("test ") {
            if pending_rate_limit.is_some() {
                return Err(format!(
                    "Line {}: @rate_limit cannot be applied to a 'test' block; only endpoints are supported",
                    i + 1
                ));
            }
            let (test_case, next_index) = parse_test(&lines, i)?;
            tests.push(test_case);
            i = next_index;
        } else if trimmed.starts_with("model ") {
            if pending_rate_limit.is_some() {
                return Err(format!(
                    "Line {}: @rate_limit can only precede an 'endpoint' declaration",
                    i + 1
                ));
            }
            let (model_def, next_index) = parse_model(&lines, i)?;
            if models.contains_key(&model_def.name) {
                return Err(format!(
                    "Line {}: model '{}' already defined",
                    i + 1,
                    model_def.name
                ));
            }
            models.insert(model_def.name.clone(), model_def);
            i = next_index;
        } else {
            return Err(format!(
                "Line {}: unrecognized statement (expected 'server', 'endpoint', 'func', 'class', 'secret', 'test', 'model', or '@rate_limit')",
                i + 1
            ));
        }
    }

    if pending_rate_limit.is_some() {
        return Err("Dangling @rate_limit with no following 'endpoint' declaration".to_string());
    }

    let server = server.ok_or_else(|| "Program must have a 'server' declaration".to_string())?;

    Ok(Program {
        server,
        endpoints,
        functions,
        classes,
        secrets,
        tests,
        models,
    })
}

// ---------- server ----------

fn parse_server_line(line: &str, line_no: usize) -> Result<ServerDecl, String> {
    // Expected:
    //   server 3000
    //   server 443 tls
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 2 || parts.len() > 3 {
        return Err(format!(
            "Line {}: invalid server declaration; expected 'server <port>' or 'server <port> tls'",
            line_no
        ));
    }

    let port_str = parts[1];
    let port: u16 = port_str.parse().map_err(|_| {
        format!(
            "Line {}: invalid port number '{}'; expected an integer between 0 and 65535",
            line_no, port_str
        )
    })?;

    let mut tls = false;
    if parts.len() == 3 {
        if parts[2] != "tls" {
            return Err(format!(
                "Line {}: expected 'tls' keyword after port or nothing",
                line_no
            ));
        }
        tls = true;
    }

    Ok(ServerDecl { port, tls })
}

// ---------- secret ----------

fn parse_secret_line(line: &str, line_no: usize) -> Result<SecretDecl, String> {
    // secret NAME = "ENV_VAR_NAME"
    let rest = line
        .strip_prefix("secret")
        .ok_or_else(|| format!("Line {}: secret line must start with 'secret'", line_no))?
        .trim_start();

    let parts: Vec<&str> = rest.splitn(2, '=').collect();
    if parts.len() != 2 {
        return Err(format!(
            "Line {}: invalid secret declaration; expected 'secret NAME = \"ENV_VAR\"'",
            line_no
        ));
    }

    let name = parts[0].trim();
    if name.is_empty() {
        return Err(format!("Line {}: secret name cannot be empty", line_no));
    }

    let rhs = parts[1].trim();
    let (key, _) = extract_quoted(rhs, line_no, "secret env var")?;

    Ok(SecretDecl {
        name: name.to_string(),
        key,
    })
}

// ---------- @rate_limit attribute ----------

fn parse_rate_limit_line(line: &str, line_no: usize) -> Result<RateLimit, String> {
    // Supports:
    //   @rate_limit(5, 60)
    //   @rate_limit 5 60
    let rest = line
        .strip_prefix("@rate_limit")
        .ok_or_else(|| {
            format!(
                "Line {}: rate-limit line must start with '@rate_limit'",
                line_no
            )
        })?
        .trim_start();

    let (max_str, window_str) = if rest.starts_with('(') {
        let close = rest.find(')').ok_or_else(|| {
            format!(
                "Line {}: expected ')' to close @rate_limit(max, window_secs)",
                line_no
            )
        })?;
        let inner = &rest[1..close];
        let parts: Vec<&str> = inner
            .split(|c| c == ',' || c == ' ')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();
        if parts.len() != 2 {
            return Err(format!(
                "Line {}: @rate_limit(max, window_secs) expects exactly two numeric arguments",
                line_no
            ));
        }
        (parts[0], parts[1])
    } else {
        let parts: Vec<&str> = rest.split_whitespace().collect();
        if parts.len() != 2 {
            return Err(format!(
                "Line {}: @rate_limit expects two numbers, e.g. '@rate_limit 5 60'",
                line_no
            ));
        }
        (parts[0], parts[1])
    };

    let max_requests: u32 = max_str.parse().map_err(|_| {
        format!(
            "Line {}: invalid max_requests '{}' in @rate_limit",
            line_no, max_str
        )
    })?;
    let window_secs: u32 = window_str.parse().map_err(|_| {
        format!(
            "Line {}: invalid window_secs '{}' in @rate_limit",
            line_no, window_str
        )
    })?;

    Ok(RateLimit {
        max_requests,
        window_secs,
    })
}

// ---------- endpoint ----------

// Parse an endpoint starting at line index `start`.
fn parse_endpoint(lines: &[&str], start: usize) -> Result<(EndpointDecl, usize), String> {
    let raw_line = lines[start];
    let line_no = start + 1;
    let line = raw_line.trim();

    let rest = line
        .strip_prefix("endpoint")
        .ok_or_else(|| format!("Line {}: endpoint line must start with 'endpoint'", line_no))?
        .trim_start();

    let mut parts = rest.splitn(2, ' ');
    let method_str = parts
        .next()
        .ok_or_else(|| format!("Line {}: missing HTTP method (GET/POST)", line_no))?;
    let rest_after_method = parts
        .next()
        .ok_or_else(|| format!("Line {}: missing path after method", line_no))?
        .trim_start();

    let method = match method_str {
        "GET" => Method::Get,
        "POST" => Method::Post,
        other => {
            return Err(format!(
                "Line {}: unsupported method '{}'; only GET and POST are supported for now",
                line_no, other
            ))
        }
    };

    let (path, rest_after_path) = extract_quoted(rest_after_method, line_no, "path")?;

    let rest_after_path = rest_after_path.trim_start();
    let colon_pos = rest_after_path.find(':').ok_or_else(|| {
        format!(
            "Line {}: expected ':' after path in endpoint declaration",
            line_no
        )
    })?;
    let after_colon = rest_after_path[colon_pos + 1..].trim_start();

    if !after_colon.is_empty() {
        let body = parse_body_spec(after_colon, line_no)?;
        let ep = EndpointDecl {
            method,
            path,
            body,
            rate_limit: None,
        };
        return Ok((ep, start + 1));
    }

    let mut j = start + 1;
    while j < lines.len() {
        let body_line_raw = lines[j];
        let body_trimmed = body_line_raw.trim();

        if body_trimmed.is_empty() || body_trimmed.starts_with('#') {
            j += 1;
            continue;
        }

        let body = parse_body_spec(body_trimmed, j + 1)?;
        let ep = EndpointDecl {
            method,
            path,
            body,
            rate_limit: None,
        };
        return Ok((ep, j + 1));
    }

    Err(format!(
        "Line {}: endpoint declaration missing body expression after ':'",
        line_no
    ))
}

// Decide whether the body is text expression or JSON.
// - JSON:   json { "message": "Hello" }
// - Text:   any expression, e.g. "Hello " + name
fn parse_body_spec(s: &str, line_no: usize) -> Result<Body, String> {
    let trimmed = s.trim();

    if let Some(rest) = trimmed.strip_prefix("json") {
        let rest = rest.trim_start();
        if rest.is_empty() {
            return Err(format!(
                "Line {}: expected JSON expression after 'json'",
                line_no
            ));
        }
        Ok(Body::JsonRaw(rest.to_string()))
    } else {
        let expr = parse_expr(trimmed)
            .map_err(|e| format!("Line {} (body expression): {}", line_no, e))?;
        Ok(Body::TextExpr(expr))
    }
}

// ---------- functions ----------

fn parse_func_line(line: &str, line_no: usize) -> Result<FunctionDef, String> {
    // func name(a, b): expr
    let rest = line
        .strip_prefix("func")
        .ok_or_else(|| format!("Line {}: function line must start with 'func'", line_no))?
        .trim_start();

    let open_paren = rest
        .find('(')
        .ok_or_else(|| format!("Line {}: expected '(' in function definition", line_no))?;
    let name = rest[..open_paren].trim().to_string();

    let after_name = &rest[open_paren + 1..];
    let close_paren = after_name
        .find(')')
        .ok_or_else(|| format!("Line {}: expected ')' in parameter list", line_no))?;
    let params_str = &after_name[..close_paren];
    let after_parens = after_name[close_paren + 1..].trim_start();

    let colon_pos = after_parens
        .find(':')
        .ok_or_else(|| format!("Line {}: expected ':' after parameter list", line_no))?;
    let body_str = after_parens[colon_pos + 1..].trim_start();

    let params = parse_param_list(params_str);

    let body_expr =
        parse_expr(body_str).map_err(|e| format!("Line {} (function body): {}", line_no, e))?;

    Ok(FunctionDef {
        name,
        params,
        body: body_expr,
    })
}

fn parse_param_list(s: &str) -> Vec<String> {
    let s = s.trim();
    if s.is_empty() {
        return Vec::new();
    }
    s.split(',')
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

// ---------- classes ----------

fn parse_class(lines: &[&str], start: usize) -> Result<(ClassDef, usize), String> {
    // class Name:
    let raw_line = lines[start];
    let line_no = start + 1;
    let line = raw_line.trim();

    let rest = line
        .strip_prefix("class")
        .ok_or_else(|| format!("Line {}: class line must start with 'class'", line_no))?
        .trim_start();

    let colon_pos = rest
        .find(':')
        .ok_or_else(|| format!("Line {}: expected ':' in class declaration", line_no))?;
    let name = rest[..colon_pos].trim().to_string();

    let mut methods: HashMap<String, FunctionDef> = HashMap::new();

    let mut i = start + 1;
    while i < lines.len() {
        let raw = lines[i];
        let trimmed = raw.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            i += 1;
            continue;
        }

        if !raw.starts_with(' ') && !raw.starts_with('\t') {
            break;
        }

        let line_no = i + 1;
        let method_def = parse_method_line(trimmed, line_no)?;
        if methods.contains_key(&method_def.name) {
            return Err(format!(
                "Line {}: method '{}' already defined in class '{}'",
                line_no, method_def.name, name
            ));
        }
        methods.insert(method_def.name.clone(), method_def);
        i += 1;
    }

    Ok((ClassDef { name, methods }, i))
}

fn parse_method_line(line: &str, line_no: usize) -> Result<FunctionDef, String> {
    // name(a, b): expr   (no 'func' keyword, inside class)
    let open_paren = line
        .find('(')
        .ok_or_else(|| format!("Line {}: expected '(' in method definition", line_no))?;
    let name = line[..open_paren].trim().to_string();

    let after_name = &line[open_paren + 1..];
    let close_paren = after_name
        .find(')')
        .ok_or_else(|| format!("Line {}: expected ')' in method parameter list", line_no))?;
    let params_str = &after_name[..close_paren];
    let after_parens = after_name[close_paren + 1..].trim_start();

    let colon_pos = after_parens
        .find(':')
        .ok_or_else(|| format!("Line {}: expected ':' after method parameter list", line_no))?;
    let body_str = after_parens[colon_pos + 1..].trim_start();

    let params = parse_param_list(params_str);

    let body_expr =
        parse_expr(body_str).map_err(|e| format!("Line {} (method body): {}", line_no, e))?;

    Ok(FunctionDef {
        name,
        params,
        body: body_expr,
    })
}

// ---------- models (ORM) ----------

fn parse_model(lines: &[&str], start: usize) -> Result<(ModelDef, usize), String> {
    // model Name:
    let raw_line = lines[start];
    let line_no = start + 1;
    let line = raw_line.trim();

    let rest = line
        .strip_prefix("model")
        .ok_or_else(|| format!("Line {}: model line must start with 'model'", line_no))?
        .trim_start();

    let colon_pos = rest
        .find(':')
        .ok_or_else(|| format!("Line {}: expected ':' in model declaration", line_no))?;
    let name = rest[..colon_pos].trim().to_string();

    if name.is_empty() {
        return Err(format!("Line {}: model name cannot be empty", line_no));
    }

    let mut fields: Vec<ModelField> = Vec::new();
    let mut i = start + 1;

    while i < lines.len() {
        let raw = lines[i];
        let trimmed = raw.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            i += 1;
            continue;
        }

        // Model body must be indented.
        if !raw.starts_with(' ') && !raw.starts_with('\t') {
            break;
        }

        let field_line_no = i + 1;
        let field = parse_model_field(trimmed, field_line_no)?;
        fields.push(field);

        i += 1;
    }

    if fields.is_empty() {
        return Err(format!(
            "Line {}: model '{}' must declare at least one field",
            line_no, name
        ));
    }

    let table_name = name.clone(); // for now; caller may normalize

    Ok((
        ModelDef {
            name,
            table_name,
            fields,
        },
        i,
    ))
}

fn parse_model_field(line: &str, line_no: usize) -> Result<ModelField, String> {
    // Grammar:
    //   field: type [pk]
    //   field?: type [pk]
    //
    // Examples:
    //   id: int pk
    //   name: string
    //   age?: int
    let colon_pos = line
        .find(':')
        .ok_or_else(|| format!("Line {}: expected ':' in model field", line_no))?;

    let raw_name = line[..colon_pos].trim();
    if raw_name.is_empty() {
        return Err(format!(
            "Line {}: model field name cannot be empty",
            line_no
        ));
    }

    let (name, is_optional) = if let Some(stripped) = raw_name.strip_suffix('?') {
        (stripped.trim().to_string(), true)
    } else {
        (raw_name.to_string(), false)
    };

    let rest = line[colon_pos + 1..].trim();
    if rest.is_empty() {
        return Err(format!(
            "Line {}: expected type after ':' in model field '{}'",
            line_no, name
        ));
    }

    let mut parts = rest.split_whitespace();
    let ty = parts
        .next()
        .ok_or_else(|| format!("Line {}: missing type for model field '{}'", line_no, name))?
        .to_string();

    let mut is_primary_key = false;
    for token in parts {
        if token.eq_ignore_ascii_case("pk")
            || token.eq_ignore_ascii_case("primary")
            || token.eq_ignore_ascii_case("primary_key")
        {
            is_primary_key = true;
        } else {
            return Err(format!(
                "Line {}: unrecognized modifier '{}' in model field '{}'",
                line_no, token, name
            ));
        }
    }

    Ok(ModelField {
        name,
        ty,
        is_primary_key,
        is_optional,
    })
}

// ---------- tests ----------

fn parse_test(lines: &[&str], start: usize) -> Result<(TestCase, usize), String> {
    // test "name":
    //   assert <expr>
    //   assert <expr>
    let raw_line = lines[start];
    let line_no = start + 1;
    let line = raw_line.trim();

    let rest = line
        .strip_prefix("test")
        .ok_or_else(|| format!("Line {}: test line must start with 'test'", line_no))?
        .trim_start();

    let (name, after_name) = extract_quoted(rest, line_no, "test name")?;
    let after_name = after_name.trim_start();
    if !after_name.starts_with(':') {
        return Err(format!(
            "Line {}: expected ':' after test name, e.g. test \"name\":",
            line_no
        ));
    }

    let mut assertions: Vec<ast::Expr> = Vec::new();
    let mut i = start + 1;

    while i < lines.len() {
        let raw = lines[i];
        let trimmed = raw.trim();

        // Skip blank lines and comments inside the test body.
        if trimmed.is_empty() || trimmed.starts_with('#') {
            i += 1;
            continue;
        }

        // Dedent ends the test block.
        if !raw.starts_with(' ') && !raw.starts_with('\t') {
            break;
        }

        let body_line_no = i + 1;
        let assert_rest = trimmed
            .strip_prefix("assert")
            .ok_or_else(|| {
                format!(
                    "Line {}: expected 'assert <expr>' inside test '{}'",
                    body_line_no, name
                )
            })?
            .trim_start();

        if assert_rest.is_empty() {
            return Err(format!(
                "Line {}: missing expression after 'assert' in test '{}'",
                body_line_no, name
            ));
        }

        let expr = parse_expr(assert_rest)
            .map_err(|e| format!("Line {} (assert expression): {}", body_line_no, e))?;
        assertions.push(expr);

        i += 1;
    }

    if assertions.is_empty() {
        return Err(format!(
            "Line {}: test '{}' must contain at least one 'assert' line",
            line_no, name
        ));
    }

    Ok((TestCase { name, assertions }, i))
}

// ---------- helpers ----------

fn extract_quoted<'a>(s: &'a str, line_no: usize, what: &str) -> Result<(String, &'a str), String> {
    let start = s.find('"').ok_or_else(|| {
        format!(
            "Line {}: expected opening '\"' for {} string",
            line_no, what
        )
    })?;
    let after_start = &s[start + 1..];
    let end_rel = after_start.find('"').ok_or_else(|| {
        format!(
            "Line {}: expected closing '\"' for {} string",
            line_no, what
        )
    })?;
    let content = &after_start[..end_rel];
    let rest = &after_start[end_rel + 1..];
    Ok((content.to_string(), rest))
}
