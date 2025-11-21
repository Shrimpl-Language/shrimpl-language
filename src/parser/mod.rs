// src/parser/mod.rs
//
// Shrimpl v0.2+ parser.
// Features:
// - server <port>
// - endpoint METHOD "/path"[: <body>]
//   Body can be on same line after colon or next non-empty line.
//   Body is either:
//     - Text expression (variables, +, -, *, /, calls, class.method calls)
//     - JSON: json { "message": "Hello" }  (treated as raw JSON string)
// - func name(a, b): expr
// - class Name:
//     method(a, b): expr
//
// Path parameters are written as "/hello/:name" (converted later in interpreter).
// Lines starting with '#' (after trimming) are comments and are ignored.

pub mod ast;
pub mod expr;

use self::ast::{Body, ClassDef, EndpointDecl, FunctionDef, Method, Program, ServerDecl};
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
            server = Some(parse_server_line(trimmed, i + 1)?);
            i += 1;
        } else if trimmed.starts_with("endpoint") {
            let (ep, next_index) = parse_endpoint(&lines, i)?;
            endpoints.push(ep);
            i = next_index;
        } else if trimmed.starts_with("func ") {
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
        } else {
            return Err(format!(
                "Line {}: unrecognized statement (expected 'server', 'endpoint', 'func', or 'class')",
                i + 1
            ));
        }
    }

    let server = server.ok_or_else(|| "Program must have a 'server' declaration".to_string())?;

    Ok(Program {
        server,
        endpoints,
        functions,
        classes,
    })
}

// ---------- server ----------

fn parse_server_line(line: &str, line_no: usize) -> Result<ServerDecl, String> {
    // Expected: server 3000
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() != 2 {
        return Err(format!(
            "Line {}: invalid server declaration; expected 'server <port>'",
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

    Ok(ServerDecl { port })
}

// ---------- endpoint ----------

// Parse an endpoint starting at line index `start`.
// Returns (endpoint, index_of_next_line_to_process)
fn parse_endpoint(lines: &[&str], start: usize) -> Result<(EndpointDecl, usize), String> {
    let raw_line = lines[start];
    let line_no = start + 1;
    let line = raw_line.trim();

    // 1. Strip the leading "endpoint"
    let rest = line
        .strip_prefix("endpoint")
        .ok_or_else(|| format!("Line {}: endpoint line must start with 'endpoint'", line_no))?
        .trim_start();

    // 2. Method is the next word
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

    // 3. Extract the path in quotes from the remainder of this line
    let (path, rest_after_path) = extract_quoted(rest_after_method, line_no, "path")?;

    // 4. After the path we expect a colon on this same line
    let rest_after_path = rest_after_path.trim_start();
    let colon_pos = rest_after_path.find(':').ok_or_else(|| {
        format!(
            "Line {}: expected ':' after path in endpoint declaration",
            line_no
        )
    })?;
    let after_colon = rest_after_path[colon_pos + 1..].trim_start();

    // 5. The body may be on the same line after the colon...
    if !after_colon.is_empty() {
        let body = parse_body_spec(after_colon, line_no)?;
        let ep = EndpointDecl { method, path, body };
        return Ok((ep, start + 1));
    }

    // 6. ...or on the next non-empty, non-comment line, possibly indented
    let mut j = start + 1;
    while j < lines.len() {
        let body_line_raw = lines[j];
        let body_trimmed = body_line_raw.trim();

        if body_trimmed.is_empty() || body_trimmed.starts_with('#') {
            j += 1;
            continue;
        }

        let body = parse_body_spec(body_trimmed, j + 1)?;
        let ep = EndpointDecl { method, path, body };
        // We consumed line j as the body, so the next line to process is j + 1
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

    if trimmed.starts_with("json") {
        let rest = trimmed["json".len()..].trim_start();
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

    // Expect colon
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

        // Skip blank lines and comments inside class
        if trimmed.is_empty() || trimmed.starts_with('#') {
            i += 1;
            continue;
        }

        // If the line is not indented, we are out of the class body
        if !raw.starts_with(' ') && !raw.starts_with('\t') {
            break;
        }

        // Method line: indent + name(params): expr
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

    // Expect colon
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

// ---------- helpers ----------

/// Extract a double-quoted string from somewhere in `s`.
/// Returns (content_without_quotes, rest_after_the_closing_quote).
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
