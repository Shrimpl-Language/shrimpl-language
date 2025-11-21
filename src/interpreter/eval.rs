// src/interpreter/eval.rs
//
// Expression evaluator for Shrimpl.
// Built-ins:
//
//   String / basic helpers
//   ----------------------
//   len(x)        -> number  (length of string)
//   upper(x)      -> string  (uppercase)
//   lower(x)      -> string  (lowercase)
//   number(x)     -> number  (string/number -> number)
//   string(x)     -> string  (anything -> string)
//
//   Numeric helpers (analysis)
//   --------------------------
//   sum(a, b, ...) -> number (sum of numbers)
//   avg(a, b, ...) -> number (average)
//   min(a, b, ...) -> number (minimum)
//   max(a, b, ...) -> number (maximum)
//
//   HTTP helpers (call other APIs)
//   ------------------------------
//   http_get(url)        -> string (raw response body)
//   http_get_json(url)   -> string (pretty JSON or error)
//
//   Vector / tensor helpers (PyTorch-ish)
//   -------------------------------------
//   vec(a, b, c, ...)          -> string JSON array, e.g. "[1,2,3]"
//   tensor_add(a, b)           -> string JSON array, elementwise sum
//   tensor_dot(a, b)           -> number dot product
//
//   DataFrame helpers (pandas-ish)
//   ------------------------------
//   df_from_csv(url)           -> string JSON table
//       { "columns": [...], "rows": [[...], [...], ...] }
//   df_head(df_json, n)        -> string JSON table, first n rows
//   df_select(df_json, cols)   -> string JSON table with selected columns
//       cols is "col1,col2"
//
//   ML helpers (scikit-learn-ish, linear regression)
//   -----------------------------------------------
//   linreg_fit(xs_json, ys_json) -> string JSON model { "kind":"linreg","a":..,"b":.. }
//       xs_json, ys_json are JSON arrays, e.g. "[1,2,3]"
//   linreg_predict(model_json, x) -> number prediction
//
// All complex objects are passed as JSON strings in Shrimpl.
// Kids only see numbers, strings, and function calls.

use crate::parser::ast::{BinOp, Expr, FunctionDef, Program};
use std::collections::HashMap;

use serde_json::{json, Value};
use std::io::Cursor;
use ureq;

// Runtime values for expressions
#[derive(Debug, Clone)]
enum ValueRuntime {
    Number(f64),
    Str(String),
}

impl ToString for ValueRuntime {
    fn to_string(&self) -> String {
        match self {
            ValueRuntime::Number(n) => {
                if n.fract() == 0.0 {
                    format!("{}", *n as i64)
                } else {
                    format!("{}", n)
                }
            }
            ValueRuntime::Str(s) => s.clone(),
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

// Public entry point for endpoint bodies
pub fn eval_body_expr(
    expr: &Expr,
    program: &Program,
    vars: &HashMap<String, String>,
) -> Result<String, String> {
    let mut env = Env::new();
    for (k, v) in vars {
        env.set(k.clone(), ValueRuntime::Str(v.clone()));
    }

    let value = eval_expr(expr, program, &env)?;
    Ok(value.to_string())
}

// Evaluate expression tree
fn eval_expr(expr: &Expr, program: &Program, env: &Env) -> Result<ValueRuntime, String> {
    match expr {
        Expr::Number(n) => Ok(ValueRuntime::Number(*n)),
        Expr::Str(s) => Ok(ValueRuntime::Str(s.clone())),
        Expr::Var(name) => env
            .get(name)
            .ok_or_else(|| format!("Unknown variable '{}'", name)),
        Expr::Binary { left, op, right } => {
            let lv = eval_expr(left, program, env)?;
            let rv = eval_expr(right, program, env)?;
            eval_binary(&lv, op, &rv)
        }
        Expr::Call { name, args } => {
            // First, user-defined functions
            if let Some(func) = program.functions.get(name) {
                let arg_vals = eval_args(args, program, env)?;
                eval_function(func, arg_vals, program, env)
            } else {
                // Then, built-in functions
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
    }
}

fn eval_args(args: &[Expr], program: &Program, env: &Env) -> Result<Vec<ValueRuntime>, String> {
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
) -> Result<ValueRuntime, String> {
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

// Built-in functions for strings, numbers, HTTP, vectors/tensors, dataframes, ML
fn eval_builtin(
    name: &str,
    args: &[Expr],
    program: &Program,
    env: &Env,
) -> Result<ValueRuntime, String> {
    let vals = eval_args(args, program, env)?;

    match name {
        // --- string helpers ---
        "len" => {
            if vals.len() != 1 {
                return Err("len(x) expects exactly 1 argument".to_string());
            }
            Ok(ValueRuntime::Number(
                vals[0].to_string().chars().count() as f64
            ))
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

        // --- HTTP client helpers ---
        "http_get" => {
            if vals.len() != 1 {
                return Err("http_get(url) expects exactly 1 argument".to_string());
            }
            let url = vals[0].to_string();
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
            let txt = serde_json::to_string_pretty(&table).unwrap_or_else(|_| df_txt);
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
            let txt = serde_json::to_string_pretty(&table).unwrap_or_else(|_| df_txt);
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
                "b": b
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

        _ => Err(format!("Undefined function '{}'", name)),
    }
}

// Try to coerce a value to a number when needed
fn as_number(v: &ValueRuntime) -> Result<f64, String> {
    match v {
        ValueRuntime::Number(n) => Ok(*n),
        ValueRuntime::Str(s) => s
            .parse::<f64>()
            .map_err(|_| format!("Value '{}' is not a number", s)),
    }
}

fn eval_binary(
    left: &ValueRuntime,
    op: &BinOp,
    right: &ValueRuntime,
) -> Result<ValueRuntime, String> {
    match op {
        BinOp::Add => match (left, right) {
            (ValueRuntime::Number(a), ValueRuntime::Number(b)) => Ok(ValueRuntime::Number(a + b)),
            _ => Ok(ValueRuntime::Str(format!(
                "{}{}",
                left.to_string(),
                right.to_string()
            ))),
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
    }
}

// Helper: parse JSON array of numbers from a string
fn parse_json_array_numbers(label: &str, text: &str) -> Result<Vec<f64>, String> {
    let val: Value = serde_json::from_str(text)
        .map_err(|e| format!("{}: not valid JSON array: {}", label, e))?;
    let arr = val
        .as_array()
        .ok_or_else(|| format!("{}: JSON value is not an array", label))?;
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

// Lightweight DataFrame representation for internal use
struct DataFrame {
    columns: Vec<String>,
    rows: Vec<Value>, // each row is Value::Array([...])
}

// Helper: parse DF JSON of the form { "columns": [...], "rows": [[...], ...] }
fn parse_df(text: &str) -> Result<DataFrame, String> {
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
