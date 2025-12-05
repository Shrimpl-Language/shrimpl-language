// src/typecheck.rs
//
// Optional static type checker for Shrimpl.
//
// Type annotations live in config/config.<env>.json under:
//
// "types": {
//   "functions": {
//     "add": { "params": ["number","number"], "result":"number" },
//     "greet": { "params": ["string"], "result":"string" }
//   }
// }
//
// The checker:
// - checks that annotated functions have the right number of params
// - infers a simple type for the function body (number/string/bool/any)
// - verifies body type is compatible with declared result type
// - produces diagnostics in the same JSON shape used by docs::build_diagnostics

use crate::config;
use crate::parser::ast::{BinOp, Expr, Program};
use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ty {
    Number,
    String,
    Bool,
    Any,
}

fn parse_type_name(name: &str) -> Ty {
    match name.to_lowercase().as_str() {
        "number" | "float" | "int" | "integer" => Ty::Number,
        "string" | "str" => Ty::String,
        "bool" | "boolean" => Ty::Bool,
        _ => Ty::Any,
    }
}

fn is_assignable(actual: Ty, expected: Ty) -> bool {
    match expected {
        Ty::Any => true,
        _ => actual == expected || actual == Ty::Any,
    }
}

/// Build type-check diagnostics as JSON objects.
///
/// Each diagnostic has:
///   { "kind": "error"|"warning",
///     "scope": "function",
///     "name": "<function name>",
///     "message": "..." }
pub fn build_type_diagnostics(program: &Program) -> Vec<Value> {
    let types_cfg = match config::types_section() {
        Some(t) => t,
        None => return Vec::new(),
    };

    if types_cfg.functions.is_empty() {
        return Vec::new();
    }

    let mut diags = Vec::<Value>::new();

    for (name, func) in &program.functions {
        let annot = match types_cfg.functions.get(name) {
            Some(a) => a,
            None => continue,
        };

        // Param count
        if annot.params.len() != func.params.len() {
            diags.push(json!({
                "kind": "error",
                "scope": "function",
                "name": name,
                "message": format!(
                    "Type annotation has {} params but function '{}' has {} params",
                    annot.params.len(),
                    name,
                    func.params.len()
                )
            }));
            continue;
        }

        // Build param type environment.
        let mut env = HashMap::<String, Ty>::new();
        for (param_name, ty_name) in func.params.iter().zip(annot.params.iter()) {
            env.insert(param_name.clone(), parse_type_name(ty_name));
        }

        let mut local_diags = Vec::<Value>::new();
        let body_ty = infer_expr_type(&func.body, &env, program, &types_cfg, &mut local_diags);
        diags.extend(local_diags);

        if let Some(result_name) = &annot.result {
            let expected = parse_type_name(result_name);
            if !is_assignable(body_ty, expected) {
                diags.push(json!({
                    "kind": "error",
                    "scope": "function",
                    "name": name,
                    "message": format!(
                        "Return type mismatch: expected {}, got {}",
                        display_ty(expected),
                        display_ty(body_ty)
                    )
                }));
            }
        }
    }

    diags
}

fn display_ty(t: Ty) -> &'static str {
    match t {
        Ty::Number => "number",
        Ty::String => "string",
        Ty::Bool => "bool",
        Ty::Any => "any",
    }
}

#[allow(clippy::only_used_in_recursion)]
fn infer_expr_type(
    expr: &Expr,
    env: &HashMap<String, Ty>,
    program: &Program,
    types_cfg: &config::TypesConfigFile,
    diags: &mut Vec<Value>,
) -> Ty {
    match expr {
        Expr::Number(_) => Ty::Number,
        Expr::Str(_) => Ty::String,
        Expr::Bool(_) => Ty::Bool,

        Expr::Var(name) => env.get(name).copied().unwrap_or(Ty::Any),

        Expr::List(_items) => Ty::Any,
        Expr::Map(_pairs) => Ty::Any,

        Expr::Binary { left, op, right } => match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => {
                let lt = infer_expr_type(left, env, program, types_cfg, diags);
                let rt = infer_expr_type(right, env, program, types_cfg, diags);
                if !is_assignable(lt, Ty::Number) || !is_assignable(rt, Ty::Number) {
                    diags.push(json!({
                        "kind": "warning",
                        "scope": "expression",
                        "name": "",
                        "message": "Numeric operator used with non-number operand(s)"
                    }));
                }
                Ty::Number
            }
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => Ty::Bool,
            BinOp::And | BinOp::Or => Ty::Bool,
        },

        Expr::Call { name, args } => {
            // Use annotated function type if known; otherwise Any.
            if let Some(f_annot) = types_cfg.functions.get(name) {
                // Argument count check
                if f_annot.params.len() != args.len() {
                    diags.push(json!({
                        "kind": "error",
                        "scope": "call",
                        "name": name,
                        "message": format!(
                            "Call to '{}' expected {} arguments but got {}",
                            name,
                            f_annot.params.len(),
                            args.len()
                        )
                    }));
                } else {
                    for (idx, (arg_expr, param_ty_name)) in
                        args.iter().zip(f_annot.params.iter()).enumerate()
                    {
                        let expected = parse_type_name(param_ty_name);
                        let actual = infer_expr_type(arg_expr, env, program, types_cfg, diags);
                        if !is_assignable(actual, expected) {
                            diags.push(json!({
                                "kind": "error",
                                "scope": "call",
                                "name": name,
                                "message": format!(
                                    "Argument {} to '{}' has type {} but annotation expects {}",
                                    idx + 1,
                                    name,
                                    display_ty(actual),
                                    display_ty(expected)
                                )
                            }));
                        }
                    }
                }
                f_annot
                    .result
                    .as_ref()
                    .map(|r| parse_type_name(r))
                    .unwrap_or(Ty::Any)
            } else {
                // Unknown function: treat as dynamic.
                Ty::Any
            }
        }

        Expr::MethodCall { .. } => {
            // For now, treat methods as dynamic.
            Ty::Any
        }

        Expr::If {
            branches,
            else_branch,
        } => {
            let mut branch_tys = Vec::new();
            for (cond, body) in branches {
                let _cond_ty = infer_expr_type(cond, env, program, types_cfg, diags);
                branch_tys.push(infer_expr_type(body, env, program, types_cfg, diags));
            }
            if let Some(else_expr) = else_branch {
                branch_tys.push(infer_expr_type(else_expr, env, program, types_cfg, diags));
            }
            // Simple join: if all same non-any type, keep it; else Any.
            if let Some(first) = branch_tys.first() {
                if branch_tys.iter().all(|t| t == first) {
                    *first
                } else {
                    Ty::Any
                }
            } else {
                Ty::Any
            }
        }

        Expr::Repeat { body, .. } => infer_expr_type(body, env, program, types_cfg, diags),

        Expr::Try {
            try_body,
            catch_body,
            finally_body,
            ..
        } => {
            let try_ty = infer_expr_type(try_body, env, program, types_cfg, diags);
            let catch_ty = catch_body
                .as_ref()
                .map(|b| infer_expr_type(b, env, program, types_cfg, diags))
                .unwrap_or(Ty::Any);
            let _finally_ty = finally_body
                .as_ref()
                .map(|b| infer_expr_type(b, env, program, types_cfg, diags))
                .unwrap_or(Ty::Any);

            if try_ty == catch_ty {
                try_ty
            } else {
                Ty::Any
            }
        }
    }
}
