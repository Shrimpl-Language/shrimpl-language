// src/interpreter/eval.rs
//
// Expression evaluator for Shrimpl.
//
// Built-ins:
//
// String / basic helpers
// ----------------------
// len(x)    -> number (length of string)
// upper(x)  -> string (uppercase)
// lower(x)  -> string (lowercase)
// number(x) -> number (string/number -> number)
// string(x) -> string (anything -> string)
// type(x)   -> string ("number", "string", "bool", "list", "map", "null")
//
// Structured data helpers
// -----------------------
// json_parse(text)               -> structured JSON value
// json_stringify(value)          -> compact JSON text
// json_pretty(value)             -> pretty JSON text
// json_get(value, path, default) -> value at user.name / items[0].name path
// json_set(value, path, new)     -> value with path updated
// contains(container, value)     -> bool for strings, lists, and maps
// split(text, separator)         -> list
// join(list, separator)          -> string
// range(stop)                    -> list [0, ..., stop)
// range(start, stop, step)       -> list
// list_get(list, index, default) -> item or default
// keys(map) / values(map)        -> list
//
// Numeric helpers (analysis)
// --------------------------
// sum(a, b, ...) -> number (sum of numbers)
// avg(a, b, ...) -> number (average)
// min(a, b, ...) -> number (minimum)
// max(a, b, ...) -> number (maximum)
//
// HTTP helpers (call other APIs)
// ------------------------------
// http_get(url)      -> string (raw response body)
// http_get_json(url) -> string (pretty JSON or error)
// http_post_json(url, body) -> response JSON value or response text
//
// Vector / tensor helpers (PyTorch-ish)
// -------------------------------------
// vec(a, b, c, ...)  -> string JSON array, e.g. "[1,2,3]"
// tensor_add(a, b)   -> string JSON array, elementwise sum
// tensor_dot(a, b)   -> number dot product
//
// DataFrame helpers (pandas-ish)
// ------------------------------
// df_from_csv(url)        -> string JSON table
//                            { "columns": [...], "rows": [[...], [...], ...] }
// df_head(df_json, n)     -> string JSON table, first n rows
// df_select(df_json, cols)-> string JSON table with selected columns
//                            cols is "col1,col2"
//
// ML helpers (scikit-learn-ish, linear regression)
// -------------------------------------------------
// linreg_fit(xs_json, ys_json) -> string JSON model
//    { "kind":"linreg","a":..,"b":.. }
// xs_json, ys_json are JSON arrays, e.g. "[1,2,3]"
// linreg_predict(model_json, x) -> number prediction
//
// OpenAI helpers (Responses / Chat style)
// --------------------------------------
// openai_set_api_key(key)        -> string "ok"
// openai_set_system_prompt(text) -> string "ok"
// openai_chat(user_message)      -> string assistant text
// openai_chat_json(user_message) -> string pretty JSON
// openai_mcp_call(server_id, tool_name, args_json) -> string pretty JSON
//
// Generic config + env + secrets helpers
// --------------------------------------
// config_set(key, value)            -> string "ok"
// config_get(key)                   -> stored value or ""
// config_get(key, default)          -> stored value or default
// config_has(key)                   -> bool
// env(name)                         -> string env var value or ""
// secret(name)                      -> string secret value or error
// secret(name, default)            -> secret or default (no error)
//
// ORM helpers (SQLite via shrimpl.db)
// -----------------------------------
// orm_insert(model_name, record_json)   -> string primary key / rowid
// orm_find_by_id(model_name, id_json)  -> string JSON object or ""
//
// Complex values stay structured inside the evaluator and are rendered as
// compact JSON only at the endpoint/string boundary.

use crate::config;
use crate::orm; // <--- hook into src/orm.rs

use crate::parser::ast::{BinOp, Expr, FunctionDef, Program};

use serde_json::{json, Value};
use std::collections::HashMap;
use std::fmt;
use std::io::Cursor;
use std::{
    env,
    sync::{Mutex, OnceLock},
};
use ureq;

// ---------- Result alias ----------

type EvalResult<T> = std::result::Result<T, String>;

// ---------- runtime values ----------

#[derive(Debug, Clone)]
enum ValueRuntime {
    Number(f64),
    Str(String),
    Bool(bool),
    Json(Value),
}

impl fmt::Display for ValueRuntime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValueRuntime::Number(n) => {
                if n.fract() == 0.0 {
                    write!(f, "{}", *n as i64)
                } else {
                    write!(f, "{}", n)
                }
            }
            ValueRuntime::Str(s) => write!(f, "{}", s),
            ValueRuntime::Bool(b) => write!(f, "{}", b),
            ValueRuntime::Json(v) => write!(f, "{}", v),
        }
    }
}

#[derive(Debug, Clone)]
struct Env {
    vars: HashMap<String, ValueRuntime>,
}

impl Env {
    fn new() -> Self {
        Env {
            vars: HashMap::new(),
        }
    }

    fn with_parent(parent: &Env) -> Self {
        Env {
            vars: parent.vars.clone(),
        }
    }

    fn set(&mut self, name: String, value: ValueRuntime) {
        self.vars.insert(name, value);
    }

    fn get(&self, name: &str) -> Option<ValueRuntime> {
        self.vars.get(name).cloned()
    }
}

// ---------- OpenAI config ----------

#[derive(Debug, Clone)]
struct OpenAIConfig {
    api_key: Option<String>,
    system_prompt: Option<String>,
    model: String,
    base_url: String,
}

static OPENAI_CONFIG: OnceLock<Mutex<OpenAIConfig>> = OnceLock::new();

fn get_openai_config() -> &'static Mutex<OpenAIConfig> {
    OPENAI_CONFIG.get_or_init(|| {
        // Initial API key comes from env; can be overridden at runtime via openai_set_api_key.
        let api_key = env::var("SHRIMPL_OPENAI_API_KEY")
            .ok()
            .or_else(|| env::var("OPENAI_API_KEY").ok());

        Mutex::new(OpenAIConfig {
            api_key,
            system_prompt: None,
            model: "gpt-4.1-mini".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
        })
    })
}

fn openai_post(path: &str, body: &Value) -> EvalResult<Value> {
    let cfg_lock = get_openai_config();
    let cfg = cfg_lock
        .lock()
        .map_err(|_| "OpenAI config mutex poisoned".to_string())?;

    let api_key = cfg.api_key.clone().ok_or_else(|| {
        "OpenAI API key is not set.\n\
         Set SHRIMPL_OPENAI_API_KEY / OPENAI_API_KEY in the environment\n\
         or call openai_set_api_key(key) from Shrimpl."
            .to_string()
    })?;

    let base = cfg.base_url.clone();
    drop(cfg);

    let url = if path.starts_with("http://") || path.starts_with("https://") {
        path.to_string()
    } else {
        format!(
            "{}/{}",
            base.trim_end_matches('/'),
            path.trim_start_matches('/')
        )
    };

    let body_text =
        serde_json::to_string(body).map_err(|e| format!("OpenAI: failed to encode body: {}", e))?;

    let resp = ureq::post(&url)
        .set("Authorization", &format!("Bearer {}", api_key))
        .set("Content-Type", "application/json")
        .send_string(&body_text);

    match resp {
        Ok(r) => {
            let text = r
                .into_string()
                .map_err(|e| format!("OpenAI: failed to read body: {}", e))?;
            let json_val: Value = serde_json::from_str(&text)
                .map_err(|e| format!("OpenAI: response not valid JSON: {}", e))?;
            Ok(json_val)
        }
        Err(err) => Err(format!("OpenAI HTTP error: {}", err)),
    }
}

// ---------- public entry point for endpoint bodies ----------

pub fn eval_body_expr(
    expr: &Expr,
    program: &Program,
    vars: &HashMap<String, String>,
) -> EvalResult<String> {
    let mut env = Env::new();
    for (k, v) in vars {
        env.set(k.clone(), ValueRuntime::Str(v.clone()));
    }

    let value = eval_expr(expr, program, &env)?;
    Ok(value.to_string())
}

// ---------- expression evaluation ----------

fn eval_expr(expr: &Expr, program: &Program, env: &Env) -> EvalResult<ValueRuntime> {
    match expr {
        Expr::Number(n) => Ok(ValueRuntime::Number(*n)),
        Expr::Str(s) => Ok(ValueRuntime::Str(s.clone())),
        Expr::Bool(b) => Ok(ValueRuntime::Bool(*b)),

        Expr::Var(name) => env
            .get(name)
            .ok_or_else(|| format!("Unknown variable '{}'", name)),

        Expr::Binary { left, op, right } => eval_binary_expr(left, op, right, program, env),

        Expr::Call { name, args } => {
            if let Some(func) = program.functions.get(name) {
                let arg_vals = eval_args(args, program, env)?;
                eval_function(func, arg_vals, program, env)
            } else {
                eval_builtin(name, args, program, env)
            }
        }

        Expr::MethodCall {
            class_name,
            method_name,
            args,
        } => {
            let class = program
                .classes
                .get(class_name)
                .ok_or_else(|| format!("Undefined class '{}'", class_name))?;

            let method = class
                .methods
                .get(method_name)
                .ok_or_else(|| format!("Class '{}' has no method '{}'", class_name, method_name))?;

            let arg_vals = eval_args(args, program, env)?;
            eval_function(method, arg_vals, program, env)
        }

        Expr::List(items) => {
            let mut arr = Vec::new();
            for item in items {
                let v = eval_expr(item, program, env)?;
                arr.push(value_to_json(&v));
            }
            Ok(ValueRuntime::Json(Value::Array(arr)))
        }

        Expr::Map(pairs) => {
            let mut obj = serde_json::Map::new();
            for (k, vexpr) in pairs {
                let v = eval_expr(vexpr, program, env)?;
                obj.insert(k.clone(), value_to_json(&v));
            }
            Ok(ValueRuntime::Json(Value::Object(obj)))
        }

        Expr::If {
            branches,
            else_branch,
        } => {
            for (cond_expr, body_expr) in branches {
                let cond_val = eval_expr(cond_expr, program, env)?;
                if as_bool(&cond_val)? {
                    return eval_expr(body_expr, program, env);
                }
            }
            if let Some(else_expr) = else_branch {
                eval_expr(else_expr, program, env)
            } else {
                Ok(ValueRuntime::Str(String::new()))
            }
        }

        Expr::Repeat { count, body } => {
            let count_val = eval_expr(count, program, env)?;
            let n = as_number(&count_val)?;
            if n < 0.0 {
                return Err("repeat N times: N must be non-negative".to_string());
            }

            let steps = n.floor() as usize;
            if steps > 10_000 {
                return Err("repeat N times: N is too large (max 10_000)".to_string());
            }

            let mut last = ValueRuntime::Str(String::new());
            let mut text_output = String::new();
            let mut all_strings = true;
            for _ in 0..steps {
                last = eval_expr(body, program, env)?;
                match &last {
                    ValueRuntime::Str(s) => text_output.push_str(s),
                    _ => all_strings = false,
                }
            }

            if all_strings {
                Ok(ValueRuntime::Str(text_output))
            } else {
                Ok(last)
            }
        }

        Expr::Try {
            try_body,
            catch_var,
            catch_body,
            finally_body,
        } => eval_try_expr(try_body, catch_var, catch_body, finally_body, program, env),
    }
}

fn eval_try_expr(
    try_body: &Expr,
    catch_var: &Option<String>,
    catch_body: &Option<Box<Expr>>,
    finally_body: &Option<Box<Expr>>,
    program: &Program,
    env: &Env,
) -> EvalResult<ValueRuntime> {
    let mut local_env = Env::with_parent(env);

    let mut result: EvalResult<ValueRuntime> = match eval_expr(try_body, program, &local_env) {
        Ok(v) => Ok(v),
        Err(err) => {
            if let Some(catch_expr) = catch_body {
                if let Some(name) = catch_var {
                    local_env.set(name.clone(), ValueRuntime::Str(err.clone()));
                }
                eval_expr(catch_expr, program, &local_env)
            } else {
                Err(err)
            }
        }
    };

    if let Some(finally_expr) = finally_body {
        if let Err(finally_err) = eval_expr(finally_expr, program, &local_env) {
            result = Err(finally_err);
        }
    }

    result
}

fn eval_args(args: &[Expr], program: &Program, env: &Env) -> EvalResult<Vec<ValueRuntime>> {
    let mut out = Vec::new();
    for a in args {
        out.push(eval_expr(a, program, env)?);
    }
    Ok(out)
}

fn eval_function(
    func: &FunctionDef,
    arg_vals: Vec<ValueRuntime>,
    program: &Program,
    parent_env: &Env,
) -> EvalResult<ValueRuntime> {
    if arg_vals.len() != func.params.len() {
        return Err(format!(
            "Function '{}' expected {} arguments, got {}",
            func.name,
            func.params.len(),
            arg_vals.len()
        ));
    }

    let mut env = Env::with_parent(parent_env);
    for (name, val) in func.params.iter().zip(arg_vals.into_iter()) {
        env.set(name.clone(), val);
    }

    eval_expr(&func.body, program, &env)
}

// ---------- built-ins ----------

fn eval_builtin(
    name: &str,
    args: &[Expr],
    program: &Program,
    env: &Env,
) -> EvalResult<ValueRuntime> {
    let vals = eval_args(args, program, env)?;

    match name {
        // --- string helpers ---
        "len" => {
            if vals.len() != 1 {
                return Err("len(x) expects exactly 1 argument".to_string());
            }
            let count = match &vals[0] {
                ValueRuntime::Json(Value::Array(items)) => items.len(),
                ValueRuntime::Json(Value::Object(map)) => map.len(),
                _ => vals[0].to_string().chars().count(),
            };
            Ok(ValueRuntime::Number(count as f64))
        }

        "upper" => {
            if vals.len() != 1 {
                return Err("upper(x) expects exactly 1 argument".to_string());
            }
            Ok(ValueRuntime::Str(vals[0].to_string().to_uppercase()))
        }

        "lower" => {
            if vals.len() != 1 {
                return Err("lower(x) expects exactly 1 argument".to_string());
            }
            Ok(ValueRuntime::Str(vals[0].to_string().to_lowercase()))
        }

        "number" => {
            if vals.len() != 1 {
                return Err("number(x) expects exactly 1 argument".to_string());
            }
            let n = as_number(&vals[0])?;
            Ok(ValueRuntime::Number(n))
        }

        "string" => {
            if vals.len() != 1 {
                return Err("string(x) expects exactly 1 argument".to_string());
            }
            Ok(ValueRuntime::Str(vals[0].to_string()))
        }

        "type" => {
            if vals.len() != 1 {
                return Err("type(x) expects exactly 1 argument".to_string());
            }
            Ok(ValueRuntime::Str(runtime_type_name(&vals[0]).to_string()))
        }

        "json_parse" => {
            if vals.len() != 1 {
                return Err("json_parse(text) expects exactly 1 argument".to_string());
            }
            let text = vals[0].to_string();
            let parsed: Value = serde_json::from_str(&text)
                .map_err(|e| format!("json_parse: input is not valid JSON: {}", e))?;
            Ok(json_to_runtime_value(&parsed))
        }

        "json_stringify" => {
            if vals.len() != 1 {
                return Err("json_stringify(value) expects exactly 1 argument".to_string());
            }
            let text = serde_json::to_string(&value_to_json(&vals[0]))
                .map_err(|e| format!("json_stringify: failed to encode value: {}", e))?;
            Ok(ValueRuntime::Str(text))
        }

        "json_pretty" => {
            if vals.len() != 1 {
                return Err("json_pretty(value) expects exactly 1 argument".to_string());
            }
            let text = serde_json::to_string_pretty(&value_to_json(&vals[0]))
                .map_err(|e| format!("json_pretty: failed to encode value: {}", e))?;
            Ok(ValueRuntime::Str(text))
        }

        "json_get" => {
            if vals.len() < 2 || vals.len() > 3 {
                return Err("json_get(value, path, [default]) expects 2 or 3 arguments".to_string());
            }

            let root = value_to_json(&vals[0]);
            let path = vals[1].to_string();
            match json_get_path(&root, &path)? {
                Some(value) => Ok(json_to_runtime_value(value)),
                None if vals.len() == 3 => Ok(vals[2].clone()),
                None => Ok(ValueRuntime::Str(String::new())),
            }
        }

        "json_set" => {
            if vals.len() != 3 {
                return Err("json_set(value, path, new_value) expects 3 arguments".to_string());
            }

            let mut root = value_to_json(&vals[0]);
            let path = vals[1].to_string();
            let new_value = value_to_json(&vals[2]);
            json_set_path(&mut root, &path, new_value)?;
            Ok(json_to_runtime_value(&root))
        }

        "contains" => {
            if vals.len() != 2 {
                return Err("contains(container, value) expects 2 arguments".to_string());
            }
            Ok(ValueRuntime::Bool(runtime_contains(&vals[0], &vals[1])))
        }

        "join" => {
            if vals.len() != 2 {
                return Err("join(list, separator) expects 2 arguments".to_string());
            }
            let arr = runtime_array("join list", &vals[0])?;
            let sep = vals[1].to_string();
            let parts: Vec<String> = arr.iter().map(json_value_to_text).collect();
            Ok(ValueRuntime::Str(parts.join(&sep)))
        }

        "split" => {
            if vals.len() != 2 {
                return Err("split(text, separator) expects 2 arguments".to_string());
            }
            let text = vals[0].to_string();
            let sep = vals[1].to_string();
            if sep.is_empty() {
                return Err("split(text, separator): separator cannot be empty".to_string());
            }
            let parts: Vec<Value> = text.split(&sep).map(|part| json!(part)).collect();
            Ok(ValueRuntime::Json(Value::Array(parts)))
        }

        "range" => {
            if vals.is_empty() || vals.len() > 3 {
                return Err(
                    "range(stop) or range(start, stop, [step]) expects 1-3 arguments".to_string(),
                );
            }

            let (start, stop, step) = match vals.len() {
                1 => (0.0, as_number(&vals[0])?, 1.0),
                2 => (as_number(&vals[0])?, as_number(&vals[1])?, 1.0),
                3 => (
                    as_number(&vals[0])?,
                    as_number(&vals[1])?,
                    as_number(&vals[2])?,
                ),
                _ => unreachable!(),
            };

            let values = build_range(start, stop, step)?;
            Ok(ValueRuntime::Json(Value::Array(
                values.into_iter().map(|n| json!(n)).collect(),
            )))
        }

        "list_get" => {
            if vals.len() < 2 || vals.len() > 3 {
                return Err("list_get(list, index, [default]) expects 2 or 3 arguments".to_string());
            }
            let arr = runtime_array("list_get list", &vals[0])?;
            let index = as_number(&vals[1])?;
            if index < 0.0 {
                if vals.len() == 3 {
                    return Ok(vals[2].clone());
                }
                return Ok(ValueRuntime::Str(String::new()));
            }
            let index = index.floor() as usize;
            match arr.get(index) {
                Some(value) => Ok(json_to_runtime_value(value)),
                None if vals.len() == 3 => Ok(vals[2].clone()),
                None => Ok(ValueRuntime::Str(String::new())),
            }
        }

        "keys" => {
            if vals.len() != 1 {
                return Err("keys(map) expects exactly 1 argument".to_string());
            }
            let obj = runtime_object("keys map", &vals[0])?;
            Ok(ValueRuntime::Json(Value::Array(
                obj.keys().map(|key| json!(key)).collect(),
            )))
        }

        "values" => {
            if vals.len() != 1 {
                return Err("values(map) expects exactly 1 argument".to_string());
            }
            let obj = runtime_object("values map", &vals[0])?;
            Ok(ValueRuntime::Json(Value::Array(
                obj.values().cloned().collect(),
            )))
        }

        // --- simple numeric analysis helpers ---
        "sum" => {
            if vals.is_empty() {
                return Err("sum(...) expects at least 1 argument".to_string());
            }
            let mut total = 0.0;
            for v in &vals {
                total += as_number(v)?;
            }
            Ok(ValueRuntime::Number(total))
        }

        "avg" => {
            if vals.is_empty() {
                return Err("avg(...) expects at least 1 argument".to_string());
            }
            let mut total = 0.0;
            for v in &vals {
                total += as_number(v)?;
            }
            Ok(ValueRuntime::Number(total / (vals.len() as f64)))
        }

        "min" => {
            if vals.is_empty() {
                return Err("min(...) expects at least 1 argument".to_string());
            }
            let mut best = as_number(&vals[0])?;
            for v in &vals[1..] {
                let n = as_number(v)?;
                if n < best {
                    best = n;
                }
            }
            Ok(ValueRuntime::Number(best))
        }

        "max" => {
            if vals.is_empty() {
                return Err("max(...) expects at least 1 argument".to_string());
            }
            let mut best = as_number(&vals[0])?;
            for v in &vals[1..] {
                let n = as_number(v)?;
                if n > best {
                    best = n;
                }
            }
            Ok(ValueRuntime::Number(best))
        }

        // --- generic config + env + secrets helpers ---
        "config_set" => {
            if vals.len() != 2 {
                return Err("config_set(key, value) expects 2 arguments".to_string());
            }
            let key = vals[0].to_string();
            let value_json = value_to_json(&vals[1]);
            config::set_value(&key, value_json);
            Ok(ValueRuntime::Str("ok".to_string()))
        }

        "config_get" => {
            if vals.is_empty() || vals.len() > 2 {
                return Err("config_get(key, [default]) expects 1 or 2 arguments".to_string());
            }

            let key = vals[0].to_string();
            if let Some(raw) = config::get_value(&key) {
                Ok(json_to_runtime_value(&raw))
            } else if vals.len() == 2 {
                Ok(vals[1].clone())
            } else {
                Ok(ValueRuntime::Str(String::new()))
            }
        }

        "config_has" => {
            if vals.len() != 1 {
                return Err("config_has(key) expects exactly 1 argument".to_string());
            }
            let key = vals[0].to_string();
            let exists = config::has_value(&key);
            Ok(ValueRuntime::Bool(exists))
        }

        "env" => {
            if vals.len() != 1 {
                return Err("env(name) expects exactly 1 argument".to_string());
            }
            let name = vals[0].to_string();
            let value = env::var(&name).unwrap_or_else(|_| String::new());
            Ok(ValueRuntime::Str(value))
        }

        "secret" => {
            // secret(name) or secret(name, default)
            if vals.is_empty() || vals.len() > 2 {
                return Err("secret(name, [default]) expects 1 or 2 arguments".to_string());
            }

            let logical = vals[0].to_string();

            let env_key = config::secret_env_from_file(&logical)
                .or_else(|| {
                    program
                        .secrets
                        .iter()
                        .find(|s| s.name == logical)
                        .map(|s| s.key.clone())
                })
                .unwrap_or_else(|| logical.clone());

            let default = if vals.len() == 2 {
                Some(vals[1].clone())
            } else {
                None
            };

            match env::var(&env_key) {
                Ok(v) => Ok(ValueRuntime::Str(v)),
                Err(_) => {
                    if let Some(d) = default {
                        Ok(d)
                    } else {
                        Err(format!(
                            "Secret '{}' (env '{}') is not set",
                            logical, env_key
                        ))
                    }
                }
            }
        }

        // --- HTTP client helpers ---
        "http_get" => {
            if vals.len() != 1 {
                return Err("http_get(url) expects exactly 1 argument".to_string());
            }
            let url = vals[0].to_string();
            validate_http_url(&url)?;
            let resp = ureq::get(&url).call();
            match resp {
                Ok(r) => match r.into_string() {
                    Ok(body) => Ok(ValueRuntime::Str(body)),
                    Err(err) => Err(format!("http_get({}): failed to read body: {}", url, err)),
                },
                Err(err) => Err(format!("http_get({}): {}", url, err)),
            }
        }

        "http_get_json" => {
            if vals.len() != 1 {
                return Err("http_get_json(url) expects exactly 1 argument".to_string());
            }
            let url = vals[0].to_string();
            validate_http_url(&url)?;
            let resp = ureq::get(&url).call();
            match resp {
                Ok(r) => {
                    let text = r.into_string().map_err(|e| {
                        format!("http_get_json({}): failed to read body: {}", url, e)
                    })?;
                    let json_val: serde_json::Value = serde_json::from_str(&text).map_err(|e| {
                        format!("http_get_json({}): response was not valid JSON: {}", url, e)
                    })?;
                    Ok(ValueRuntime::Str(
                        serde_json::to_string_pretty(&json_val).unwrap_or_else(|_| text.clone()),
                    ))
                }
                Err(err) => Err(format!("http_get_json({}): {}", url, err)),
            }
        }

        "http_post_json" => {
            if vals.len() != 2 {
                return Err("http_post_json(url, body) expects 2 arguments".to_string());
            }
            let url = vals[0].to_string();
            validate_http_url(&url)?;

            let body = value_to_json(&vals[1]);
            let body_text = serde_json::to_string(&body)
                .map_err(|e| format!("http_post_json({}): failed to encode body: {}", url, e))?;

            let resp = ureq::post(&url)
                .set("Content-Type", "application/json")
                .send_string(&body_text);

            match resp {
                Ok(r) => {
                    let text = r.into_string().map_err(|e| {
                        format!("http_post_json({}): failed to read body: {}", url, e)
                    })?;
                    let json_val: serde_json::Value =
                        serde_json::from_str(&text).unwrap_or_else(|_| json!(text));
                    Ok(json_to_runtime_value(&json_val))
                }
                Err(err) => Err(format!("http_post_json({}): {}", url, err)),
            }
        }

        // --- vector / tensor helpers ---
        "vec" => {
            if vals.is_empty() {
                return Err("vec(...) expects at least 1 argument".to_string());
            }
            let mut arr: Vec<Value> = Vec::new();
            for v in &vals {
                if let Ok(n) = as_number(v) {
                    arr.push(json!(n));
                } else {
                    arr.push(json!(v.to_string()));
                }
            }
            let txt =
                serde_json::to_string(&Value::Array(arr)).unwrap_or_else(|_| "[]".to_string());
            Ok(ValueRuntime::Str(txt))
        }

        "tensor_add" => {
            if vals.len() != 2 {
                return Err("tensor_add(a, b) expects 2 arguments".to_string());
            }
            let a_txt = vals[0].to_string();
            let b_txt = vals[1].to_string();
            let arr_a = parse_json_array_numbers("tensor_add a", &a_txt)?;
            let arr_b = parse_json_array_numbers("tensor_add b", &b_txt)?;

            if arr_a.len() != arr_b.len() {
                return Err("tensor_add: arrays must have the same length".to_string());
            }

            let summed: Vec<Value> = arr_a
                .iter()
                .zip(arr_b.iter())
                .map(|(x, y)| json!(x + y))
                .collect();
            let txt =
                serde_json::to_string(&Value::Array(summed)).unwrap_or_else(|_| "[]".to_string());
            Ok(ValueRuntime::Str(txt))
        }

        "tensor_dot" => {
            if vals.len() != 2 {
                return Err("tensor_dot(a, b) expects 2 arguments".to_string());
            }
            let a_txt = vals[0].to_string();
            let b_txt = vals[1].to_string();
            let arr_a = parse_json_array_numbers("tensor_dot a", &a_txt)?;
            let arr_b = parse_json_array_numbers("tensor_dot b", &b_txt)?;

            if arr_a.len() != arr_b.len() {
                return Err("tensor_dot: arrays must have the same length".to_string());
            }

            let mut dot = 0.0;
            for (x, y) in arr_a.iter().zip(arr_b.iter()) {
                dot += x * y;
            }
            Ok(ValueRuntime::Number(dot))
        }

        // --- DataFrame helpers ---
        "df_from_csv" => {
            if vals.len() != 1 {
                return Err("df_from_csv(url) expects exactly 1 argument".to_string());
            }
            let url = vals[0].to_string();
            validate_http_url(&url)?;
            let resp = ureq::get(&url).call();
            let text = match resp {
                Ok(r) => r
                    .into_string()
                    .map_err(|e| format!("df_from_csv({}): failed to read body: {}", url, e))?,
                Err(err) => {
                    return Err(format!("df_from_csv({}): {}", url, err));
                }
            };

            let mut rdr = csv::ReaderBuilder::new()
                .has_headers(true)
                .from_reader(Cursor::new(text.into_bytes()));

            let headers_record = rdr
                .headers()
                .map_err(|e| format!("df_from_csv({}): failed to read headers: {}", url, e))?;
            let headers: Vec<String> = headers_record.iter().map(|s| s.to_string()).collect();

            let mut rows_json: Vec<Value> = Vec::new();
            for rec in rdr.records() {
                let record =
                    rec.map_err(|e| format!("df_from_csv({}): failed to read record: {}", url, e))?;
                let mut row_vals: Vec<Value> = Vec::new();
                for field in record.iter() {
                    if let Ok(n) = field.parse::<f64>() {
                        row_vals.push(json!(n));
                    } else {
                        row_vals.push(json!(field));
                    }
                }
                rows_json.push(Value::Array(row_vals));
            }

            let table = json!({
                "columns": headers,
                "rows": rows_json
            });

            let txt = serde_json::to_string(&table).unwrap_or_else(|_| "{}".to_string());
            Ok(ValueRuntime::Str(txt))
        }

        "df_head" => {
            if vals.len() != 2 {
                return Err("df_head(df_json, n) expects 2 arguments".to_string());
            }
            let df_txt = vals[0].to_string();
            let n = as_number(&vals[1])? as usize;

            let mut df = parse_df(&df_txt)?;
            if df.rows.len() > n {
                df.rows.truncate(n);
            }

            let table = json!({
                "columns": df.columns,
                "rows": df.rows,
            });

            let txt = serde_json::to_string_pretty(&table).unwrap_or(df_txt);
            Ok(ValueRuntime::Str(txt))
        }

        "df_select" => {
            if vals.len() != 2 {
                return Err("df_select(df_json, columns) expects 2 arguments".to_string());
            }
            let df_txt = vals[0].to_string();
            let cols_str = vals[1].to_string();

            let col_names: Vec<String> = cols_str
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();

            if col_names.is_empty() {
                return Err("df_select: columns string must not be empty".to_string());
            }

            let df = parse_df(&df_txt)?;

            let mut indices = Vec::new();
            for name in &col_names {
                match df.columns.iter().position(|c| c == name) {
                    Some(idx) => indices.push(idx),
                    None => {
                        return Err(format!(
                            "df_select: column '{}' not found in dataframe",
                            name
                        ));
                    }
                }
            }

            let mut new_rows: Vec<Value> = Vec::new();
            for row in df.rows {
                let mut new_vals: Vec<Value> = Vec::new();
                let row_arr = match row {
                    Value::Array(v) => v,
                    _ => {
                        return Err("df_select: row is not an array".to_string());
                    }
                };

                for &idx in &indices {
                    if idx >= row_arr.len() {
                        return Err("df_select: row shorter than expected".to_string());
                    }
                    new_vals.push(row_arr[idx].clone());
                }

                new_rows.push(Value::Array(new_vals));
            }

            let table = json!({
                "columns": col_names,
                "rows": new_rows,
            });

            let txt = serde_json::to_string_pretty(&table).unwrap_or(df_txt);
            Ok(ValueRuntime::Str(txt))
        }

        // --- ML helpers: simple linear regression ---
        "linreg_fit" => {
            if vals.len() != 2 {
                return Err("linreg_fit(xs_json, ys_json) expects 2 arguments".to_string());
            }
            let xs_txt = vals[0].to_string();
            let ys_txt = vals[1].to_string();

            let xs = parse_json_array_numbers("linreg_fit xs", &xs_txt)?;
            let ys = parse_json_array_numbers("linreg_fit ys", &ys_txt)?;

            if xs.len() != ys.len() {
                return Err("linreg_fit: xs and ys must have the same length".to_string());
            }
            if xs.len() < 2 {
                return Err("linreg_fit: need at least 2 points".to_string());
            }

            let n = xs.len() as f64;
            let mean_x: f64 = xs.iter().sum::<f64>() / n;
            let mean_y: f64 = ys.iter().sum::<f64>() / n;

            let mut num = 0.0;
            let mut den = 0.0;
            for (x, y) in xs.iter().zip(ys.iter()) {
                let dx = x - mean_x;
                let dy = y - mean_y;
                num += dx * dy;
                den += dx * dx;
            }

            if den == 0.0 {
                return Err("linreg_fit: variance of x is zero".to_string());
            }

            let a = num / den;
            let b = mean_y - a * mean_x;

            let model = json!({
                "kind": "linreg",
                "a": a,
                "b": b,
            });

            let txt = serde_json::to_string(&model).unwrap_or_else(|_| "{}".to_string());
            Ok(ValueRuntime::Str(txt))
        }

        "linreg_predict" => {
            if vals.len() != 2 {
                return Err("linreg_predict(model_json, x) expects 2 arguments".to_string());
            }
            let model_txt = vals[0].to_string();
            let x = as_number(&vals[1])?;

            let model_val: Value = serde_json::from_str(&model_txt)
                .map_err(|e| format!("linreg_predict: model_json is not valid JSON: {}", e))?;

            let a = model_val
                .get("a")
                .and_then(|v| v.as_f64())
                .ok_or_else(|| "linreg_predict: model missing numeric 'a'".to_string())?;
            let b = model_val
                .get("b")
                .and_then(|v| v.as_f64())
                .ok_or_else(|| "linreg_predict: model missing numeric 'b'".to_string())?;

            let y = a * x + b;
            Ok(ValueRuntime::Number(y))
        }

        // --- OpenAI / AI helpers ---
        "openai_set_api_key" => {
            if vals.len() != 1 {
                return Err("openai_set_api_key(key) expects exactly 1 argument".to_string());
            }
            let key = vals[0].to_string();
            let cfg_lock = get_openai_config();
            let mut cfg = cfg_lock
                .lock()
                .map_err(|_| "OpenAI config mutex poisoned".to_string())?;
            cfg.api_key = Some(key);
            Ok(ValueRuntime::Str("ok".to_string()))
        }

        "openai_set_system_prompt" => {
            if vals.len() != 1 {
                return Err("openai_set_system_prompt(prompt) expects 1 argument".to_string());
            }
            let prompt = vals[0].to_string();
            let cfg_lock = get_openai_config();
            let mut cfg = cfg_lock
                .lock()
                .map_err(|_| "OpenAI config mutex poisoned".to_string())?;
            cfg.system_prompt = Some(prompt);
            Ok(ValueRuntime::Str("ok".to_string()))
        }

        "openai_chat" => {
            if vals.len() != 1 {
                return Err("openai_chat(user_message) expects 1 argument".to_string());
            }
            let user_msg = vals[0].to_string();

            let cfg_lock = get_openai_config();
            let cfg = cfg_lock
                .lock()
                .map_err(|_| "OpenAI config mutex poisoned".to_string())?;
            let model = cfg.model.clone();
            let system_prompt = cfg.system_prompt.clone();
            drop(cfg);

            let mut messages: Vec<Value> = Vec::new();
            if let Some(sp) = system_prompt {
                messages.push(json!({ "role": "system", "content": sp }));
            }
            messages.push(json!({
                "role": "user",
                "content": user_msg
            }));

            let payload = json!({
                "model": model,
                "messages": messages,
            });

            let json_resp = openai_post("chat/completions", &payload)?;
            let text = json_resp
                .get("choices")
                .and_then(|c| c.as_array())
                .and_then(|arr| arr.first())
                .and_then(|first| first.get("message"))
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_str())
                .unwrap_or("")
                .to_string();

            Ok(ValueRuntime::Str(text))
        }

        "openai_chat_json" => {
            if vals.len() != 1 {
                return Err("openai_chat_json(user_message) expects 1 argument".to_string());
            }
            let user_msg = vals[0].to_string();

            let cfg_lock = get_openai_config();
            let cfg = cfg_lock
                .lock()
                .map_err(|_| "OpenAI config mutex poisoned".to_string())?;
            let model = cfg.model.clone();
            let system_prompt = cfg.system_prompt.clone();
            drop(cfg);

            let mut messages: Vec<Value> = Vec::new();
            if let Some(sp) = system_prompt {
                messages.push(json!({ "role": "system", "content": sp }));
            }
            messages.push(json!({
                "role": "user",
                "content": user_msg
            }));

            let payload = json!({
                "model": model,
                "messages": messages,
            });

            let json_resp = openai_post("chat/completions", &payload)?;
            let txt =
                serde_json::to_string_pretty(&json_resp).unwrap_or_else(|_| json_resp.to_string());
            Ok(ValueRuntime::Str(txt))
        }

        "openai_mcp_call" => {
            if vals.len() != 3 {
                return Err(
                    "openai_mcp_call(server_id, tool_name, args_json) expects 3 arguments"
                        .to_string(),
                );
            }

            let server_id = vals[0].to_string();
            let tool_name = vals[1].to_string();
            let args_raw = vals[2].to_string();

            let args_val: Value =
                serde_json::from_str(&args_raw).unwrap_or_else(|_| json!({ "raw": args_raw }));

            let cfg_lock = get_openai_config();
            let cfg = cfg_lock
                .lock()
                .map_err(|_| "OpenAI config mutex poisoned".to_string())?;
            let model = cfg.model.clone();
            drop(cfg);

            let payload = json!({
                "model": model,
                "input": format!(
                    "Call MCP tool '{}' on server '{}' with args: {}",
                    tool_name, server_id, args_val
                ),
            });

            let json_resp = openai_post("responses", &payload)?;
            let txt =
                serde_json::to_string_pretty(&json_resp).unwrap_or_else(|_| json_resp.to_string());
            Ok(ValueRuntime::Str(txt))
        }

        // --- ORM helpers ---
        "orm_insert" => {
            if vals.len() != 2 {
                return Err("orm_insert(model_name, record_json) expects 2 arguments".to_string());
            }

            let model_name = vals[0].to_string();
            let record_json = vals[1].to_string();

            let rowid = orm::orm_insert(&model_name, &record_json)
                .map_err(|e| format!("orm_insert: {}", e))?;

            Ok(ValueRuntime::Str(rowid))
        }

        "orm_find_by_id" => {
            if vals.len() != 2 {
                return Err("orm_find_by_id(model_name, id_json) expects 2 arguments".to_string());
            }

            let model_name = vals[0].to_string();
            let id_json = vals[1].to_string();

            let result = orm::orm_find_by_id(&model_name, &id_json)
                .map_err(|e| format!("orm_find_by_id: {}", e))?;

            // Clippy fix: use unwrap_or_default instead of manual match
            let out: String = result.unwrap_or_default();

            Ok(ValueRuntime::Str(out))
        }

        _ => Err(format!("Undefined function '{}'", name)),
    }
}

// ---------- helpers ----------

fn value_to_json(v: &ValueRuntime) -> Value {
    match v {
        ValueRuntime::Number(n) => json!(n),
        ValueRuntime::Bool(b) => json!(*b),
        ValueRuntime::Json(value) => value.clone(),
        ValueRuntime::Str(s) => {
            // Try to parse as JSON; fall back to string.
            serde_json::from_str::<Value>(s).unwrap_or_else(|_| json!(s))
        }
    }
}

fn json_to_runtime_value(v: &Value) -> ValueRuntime {
    if let Some(b) = v.as_bool() {
        ValueRuntime::Bool(b)
    } else if let Some(n) = v.as_f64() {
        ValueRuntime::Number(n)
    } else if let Some(s) = v.as_str() {
        ValueRuntime::Str(s.to_string())
    } else {
        ValueRuntime::Json(v.clone())
    }
}

fn as_number(v: &ValueRuntime) -> EvalResult<f64> {
    match v {
        ValueRuntime::Number(n) => Ok(*n),
        ValueRuntime::Str(s) => s
            .parse::<f64>()
            .map_err(|_| format!("Value '{}' is not a number", s)),
        ValueRuntime::Bool(b) => Err(format!("Value '{}' is not a number", b)),
        ValueRuntime::Json(value) => value
            .as_f64()
            .ok_or_else(|| format!("Value '{}' is not a number", value)),
    }
}

fn as_bool(v: &ValueRuntime) -> EvalResult<bool> {
    match v {
        ValueRuntime::Bool(b) => Ok(*b),
        ValueRuntime::Number(n) => Ok(*n != 0.0),
        ValueRuntime::Str(s) => Ok(!s.is_empty()),
        ValueRuntime::Json(value) => Ok(match value {
            Value::Null => false,
            Value::Bool(b) => *b,
            Value::Number(n) => n.as_f64().unwrap_or(0.0) != 0.0,
            Value::String(s) => !s.is_empty(),
            Value::Array(items) => !items.is_empty(),
            Value::Object(map) => !map.is_empty(),
        }),
    }
}

fn eval_binary_expr(
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    program: &Program,
    env: &Env,
) -> EvalResult<ValueRuntime> {
    let lv = eval_expr(left, program, env)?;

    match op {
        BinOp::And => {
            if !as_bool(&lv)? {
                return Ok(ValueRuntime::Bool(false));
            }
            let rv = eval_expr(right, program, env)?;
            Ok(ValueRuntime::Bool(as_bool(&rv)?))
        }
        BinOp::Or => {
            if as_bool(&lv)? {
                return Ok(ValueRuntime::Bool(true));
            }
            let rv = eval_expr(right, program, env)?;
            Ok(ValueRuntime::Bool(as_bool(&rv)?))
        }
        _ => {
            let rv = eval_expr(right, program, env)?;
            eval_binary(&lv, op, &rv)
        }
    }
}

fn eval_binary(left: &ValueRuntime, op: &BinOp, right: &ValueRuntime) -> EvalResult<ValueRuntime> {
    match op {
        BinOp::Add => match (left, right) {
            (ValueRuntime::Number(a), ValueRuntime::Number(b)) => Ok(ValueRuntime::Number(a + b)),
            _ => Ok(ValueRuntime::Str(format!("{}{}", left, right))),
        },

        BinOp::Sub | BinOp::Mul | BinOp::Div => {
            let a = as_number(left)?;
            let b = as_number(right)?;

            let res = match op {
                BinOp::Sub => a - b,
                BinOp::Mul => a * b,
                BinOp::Div => {
                    if b == 0.0 {
                        return Err("Division by zero".to_string());
                    }
                    a / b
                }
                _ => unreachable!(),
            };

            Ok(ValueRuntime::Number(res))
        }

        BinOp::Eq => Ok(ValueRuntime::Bool(
            value_to_json(left) == value_to_json(right),
        )),

        BinOp::Ne => Ok(ValueRuntime::Bool(
            value_to_json(left) != value_to_json(right),
        )),

        BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
            let a = as_number(left)?;
            let b = as_number(right)?;
            let result = match op {
                BinOp::Lt => a < b,
                BinOp::Le => a <= b,
                BinOp::Gt => a > b,
                BinOp::Ge => a >= b,
                _ => unreachable!(),
            };
            Ok(ValueRuntime::Bool(result))
        }

        BinOp::And => {
            let a = as_bool(left)?;
            let b = as_bool(right)?;
            Ok(ValueRuntime::Bool(a && b))
        }

        BinOp::Or => {
            let a = as_bool(left)?;
            let b = as_bool(right)?;
            Ok(ValueRuntime::Bool(a || b))
        }
    }
}

fn runtime_type_name(value: &ValueRuntime) -> &'static str {
    match value {
        ValueRuntime::Number(_) => "number",
        ValueRuntime::Str(_) => "string",
        ValueRuntime::Bool(_) => "bool",
        ValueRuntime::Json(Value::Null) => "null",
        ValueRuntime::Json(Value::Array(_)) => "list",
        ValueRuntime::Json(Value::Object(_)) => "map",
        ValueRuntime::Json(Value::Bool(_)) => "bool",
        ValueRuntime::Json(Value::Number(_)) => "number",
        ValueRuntime::Json(Value::String(_)) => "string",
    }
}

fn json_value_to_text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n
            .as_f64()
            .map(|value| {
                if value.fract() == 0.0 {
                    (value as i64).to_string()
                } else {
                    value.to_string()
                }
            })
            .unwrap_or_else(|| n.to_string()),
        Value::Bool(b) => b.to_string(),
        Value::Null => String::new(),
        Value::Array(_) | Value::Object(_) => value.to_string(),
    }
}

fn runtime_array(label: &str, value: &ValueRuntime) -> EvalResult<Vec<Value>> {
    match value_to_json(value) {
        Value::Array(items) => Ok(items),
        other => Err(format!("{}: expected a list, got {}", label, other)),
    }
}

fn runtime_object(label: &str, value: &ValueRuntime) -> EvalResult<serde_json::Map<String, Value>> {
    match value_to_json(value) {
        Value::Object(map) => Ok(map),
        other => Err(format!("{}: expected a map, got {}", label, other)),
    }
}

fn runtime_contains(container: &ValueRuntime, needle: &ValueRuntime) -> bool {
    match value_to_json(container) {
        Value::String(s) => s.contains(&needle.to_string()),
        Value::Array(items) => {
            let needle_json = value_to_json(needle);
            items.iter().any(|item| item == &needle_json)
        }
        Value::Object(map) => map.contains_key(&needle.to_string()),
        other => other == value_to_json(needle),
    }
}

fn build_range(start: f64, stop: f64, step: f64) -> EvalResult<Vec<f64>> {
    if step == 0.0 {
        return Err("range(...): step cannot be 0".to_string());
    }

    let mut out = Vec::new();
    let mut current = start;
    let forward = step > 0.0;

    while (forward && current < stop) || (!forward && current > stop) {
        out.push(current);
        if out.len() > 10_000 {
            return Err("range(...): too many values (max 10_000)".to_string());
        }
        current += step;
    }

    Ok(out)
}

#[derive(Debug, Clone)]
enum JsonPathSegment {
    Key(String),
    Index(usize),
}

fn parse_json_path(path: &str) -> EvalResult<Vec<JsonPathSegment>> {
    let mut text = path.trim();
    if text == "$" || text.is_empty() {
        return Ok(Vec::new());
    }
    if let Some(rest) = text.strip_prefix("$.") {
        text = rest;
    } else if let Some(rest) = text.strip_prefix('$') {
        text = rest.trim_start_matches('.');
    } else {
        text = text.trim_start_matches('.');
    }

    let chars: Vec<char> = text.chars().collect();
    let mut segments = Vec::new();
    let mut key = String::new();
    let mut i = 0usize;

    while i < chars.len() {
        match chars[i] {
            '.' => {
                push_json_path_key(&mut segments, &mut key);
                i += 1;
            }
            '[' => {
                push_json_path_key(&mut segments, &mut key);
                i += 1;
                let start = i;
                while i < chars.len() && chars[i] != ']' {
                    i += 1;
                }
                if i >= chars.len() {
                    return Err(format!("Invalid JSON path '{}': missing ']'", path));
                }
                let index_text: String = chars[start..i].iter().collect();
                let index = index_text.trim().parse::<usize>().map_err(|_| {
                    format!(
                        "Invalid JSON path '{}': '{}' is not a list index",
                        path, index_text
                    )
                })?;
                segments.push(JsonPathSegment::Index(index));
                i += 1;
            }
            c => {
                key.push(c);
                i += 1;
            }
        }
    }

    push_json_path_key(&mut segments, &mut key);
    Ok(segments)
}

fn push_json_path_key(segments: &mut Vec<JsonPathSegment>, key: &mut String) {
    let trimmed = key.trim();
    if !trimmed.is_empty() {
        if let Ok(index) = trimmed.parse::<usize>() {
            segments.push(JsonPathSegment::Index(index));
        } else {
            segments.push(JsonPathSegment::Key(trimmed.to_string()));
        }
    }
    key.clear();
}

fn json_get_path<'a>(root: &'a Value, path: &str) -> EvalResult<Option<&'a Value>> {
    let mut current = root;
    for segment in parse_json_path(path)? {
        match segment {
            JsonPathSegment::Key(key) => {
                let Some(next) = current.get(&key) else {
                    return Ok(None);
                };
                current = next;
            }
            JsonPathSegment::Index(index) => {
                let Some(next) = current.get(index) else {
                    return Ok(None);
                };
                current = next;
            }
        }
    }
    Ok(Some(current))
}

fn json_set_path(root: &mut Value, path: &str, new_value: Value) -> EvalResult<()> {
    let segments = parse_json_path(path)?;
    if segments.is_empty() {
        *root = new_value;
        return Ok(());
    }

    let mut current = root;
    let last_index = segments.len() - 1;

    for (idx, segment) in segments.iter().enumerate() {
        let is_last = idx == last_index;
        match segment {
            JsonPathSegment::Key(key) => {
                if is_last {
                    ensure_object(current, path)?.insert(key.clone(), new_value.clone());
                    return Ok(());
                }
                let object = ensure_object(current, path)?;
                current = object
                    .entry(key.clone())
                    .or_insert_with(|| Value::Object(serde_json::Map::new()));
            }
            JsonPathSegment::Index(index) => {
                let array = ensure_array(current, path)?;
                if *index >= array.len() {
                    return Err(format!(
                        "json_set: index {} is out of bounds for path '{}'",
                        index, path
                    ));
                }
                if is_last {
                    array[*index] = new_value.clone();
                    return Ok(());
                }
                current = &mut array[*index];
            }
        }
    }

    Ok(())
}

fn ensure_object<'a>(
    value: &'a mut Value,
    path: &str,
) -> EvalResult<&'a mut serde_json::Map<String, Value>> {
    if !value.is_object() {
        *value = Value::Object(serde_json::Map::new());
    }
    value
        .as_object_mut()
        .ok_or_else(|| format!("json_set: path '{}' expected a map", path))
}

fn ensure_array<'a>(value: &'a mut Value, path: &str) -> EvalResult<&'a mut Vec<Value>> {
    value
        .as_array_mut()
        .ok_or_else(|| format!("json_set: path '{}' expected a list", path))
}

fn validate_http_url(url: &str) -> EvalResult<()> {
    if url.starts_with("http://") || url.starts_with("https://") {
        Ok(())
    } else {
        Err(format!(
            "HTTP URL '{}' must start with http:// or https://",
            url
        ))
    }
}

fn parse_json_array_numbers(label: &str, text: &str) -> EvalResult<Vec<f64>> {
    let val: Value = serde_json::from_str(text).unwrap_or_else(|_| json!(text));

    let arr = if let Some(arr) = val.as_array() {
        arr
    } else {
        return Err(format!("{}: JSON value is not an array", label));
    };

    let mut out = Vec::new();
    for v in arr {
        if let Some(n) = v.as_f64() {
            out.push(n);
        } else if let Some(s) = v.as_str() {
            let n = s
                .parse::<f64>()
                .map_err(|_| format!("{}: element '{}' is not a number", label, s))?;
            out.push(n);
        } else {
            return Err(format!("{}: element is not a number", label));
        }
    }

    Ok(out)
}

struct DataFrame {
    columns: Vec<String>,
    rows: Vec<Value>,
}

fn parse_df(text: &str) -> EvalResult<DataFrame> {
    let val: Value =
        serde_json::from_str(text).map_err(|e| format!("df: not valid JSON table: {}", e))?;

    let cols_val = val
        .get("columns")
        .ok_or_else(|| "df: missing 'columns' field".to_string())?;
    let rows_val = val
        .get("rows")
        .ok_or_else(|| "df: missing 'rows' field".to_string())?;

    let cols_arr = cols_val
        .as_array()
        .ok_or_else(|| "df: 'columns' is not an array".to_string())?;
    let rows_arr = rows_val
        .as_array()
        .ok_or_else(|| "df: 'rows' is not an array".to_string())?;

    let columns: Vec<String> = cols_arr
        .iter()
        .map(|c| c.as_str().unwrap_or("").to_string())
        .collect();
    let rows: Vec<Value> = rows_arr.to_vec();

    Ok(DataFrame { columns, rows })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast::Body;
    use crate::parser::parse_program;

    #[test]
    fn repeat_concatenates_string_results() {
        let source = r#"server 3000
func repeat_greet(name, n): repeat number(n) times: "Hello " + name + "! "
endpoint GET "/": repeat_greet("Ana", 2)
"#;
        let program = parse_program(source).expect("program should parse");
        let endpoint = program.endpoints.first().expect("endpoint should exist");

        let Body::TextExpr(expr) = &endpoint.body else {
            panic!("expected text expression body");
        };

        let vars = HashMap::new();
        let result = eval_body_expr(expr, &program, &vars).expect("expression should evaluate");
        assert_eq!(result, "Hello Ana! Hello Ana! ");
    }

    #[test]
    fn json_helpers_preserve_structured_values() {
        let source = r#"server 3000
func profile(): { name: "Ana", scores: [10, 20] }
endpoint GET "/": json_get(json_set(profile(), "scores[0]", 99), "scores.0")
"#;
        let program = parse_program(source).expect("program should parse");
        let endpoint = program.endpoints.first().expect("endpoint should exist");

        let Body::TextExpr(expr) = &endpoint.body else {
            panic!("expected text expression body");
        };

        let vars = HashMap::new();
        let result = eval_body_expr(expr, &program, &vars).expect("expression should evaluate");
        assert_eq!(result, "99");
    }

    #[test]
    fn list_helpers_keep_simple_data_workflows_compact() {
        let source = r#"server 3000
endpoint GET "/": join(range(1, 4), ",")
"#;
        let program = parse_program(source).expect("program should parse");
        let endpoint = program.endpoints.first().expect("endpoint should exist");

        let Body::TextExpr(expr) = &endpoint.body else {
            panic!("expected text expression body");
        };

        let vars = HashMap::new();
        let result = eval_body_expr(expr, &program, &vars).expect("expression should evaluate");
        assert_eq!(result, "1,2,3");
    }

    #[test]
    fn parser_handles_negative_numbers_and_escaped_strings() {
        let source = "server 3000\nendpoint GET \"/\": \"line\\n\" + string(-2 + 5)\n";
        let program = parse_program(source).expect("program should parse");
        let endpoint = program.endpoints.first().expect("endpoint should exist");

        let Body::TextExpr(expr) = &endpoint.body else {
            panic!("expected text expression body");
        };

        let vars = HashMap::new();
        let result = eval_body_expr(expr, &program, &vars).expect("expression should evaluate");
        assert_eq!(result, "line\n3");
    }

    #[test]
    fn boolean_logic_short_circuits_runtime_errors() {
        let source = r#"server 3000
endpoint GET "/": false and missing()
"#;
        let program = parse_program(source).expect("program should parse");
        let endpoint = program.endpoints.first().expect("endpoint should exist");

        let Body::TextExpr(expr) = &endpoint.body else {
            panic!("expected text expression body");
        };

        let vars = HashMap::new();
        let result = eval_body_expr(expr, &program, &vars).expect("expression should evaluate");
        assert_eq!(result, "false");
    }
}
