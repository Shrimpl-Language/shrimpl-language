// src/tests.rs
//
// Runtime support for Shrimpl "test" blocks embedded in a Program.
//
// The AST exposes:
//   - Program.tests: Vec<TestCase>
//   - TestCase { name: String, assertions: Vec<Expr> }
//
// This module provides helpers to execute those tests using the current
// interpreter API (eval::eval_body_expr), which returns a String
// representation of the evaluated value.
//
// A test assertion is considered "passing" if it evaluates to the string
// "true" (case-sensitive) after trimming whitespace. Any other value or
// runtime error is treated as a failure.

use std::collections::HashMap;

use crate::interpreter::eval;
use crate::parser::ast::{Expr, Program, TestCase};

/// Result for a single Shrimpl test case.
#[derive(Debug, Clone)]
pub struct TestResult {
    /// Name of the test case (from the Shrimpl source).
    pub name: String,
    /// Whether all assertions in the test case passed.
    pub passed: bool,
    /// Human-readable descriptions of individual assertion failures.
    pub failures: Vec<String>,
}

/// Run all Shrimpl tests embedded in a Program and return a list of
/// structured results.
///
/// This function does not print anything by itself; callers can decide
/// how to surface the results (e.g. pretty print, JSON, etc.).
pub fn run_program_tests(program: &Program) -> Vec<TestResult> {
    let mut results = Vec::new();

    for test_case in &program.tests {
        results.push(run_single_test(program, test_case));
    }

    results
}

/// Internal helper: run a single TestCase.
fn run_single_test(program: &Program, test: &TestCase) -> TestResult {
    let mut failures = Vec::new();

    // Currently Shrimpl tests run with an empty "vars" environment.
    // If you later add support for injecting variables into tests,
    // this is the map to populate.
    let vars: HashMap<String, String> = HashMap::new();

    for (idx, expr) in test.assertions.iter().enumerate() {
        match eval_assertion(expr, program, &vars) {
            Ok(true) => {
                // assertion passed
            }
            Ok(false) => {
                failures.push(format!(
                    "assertion {} in test '{}' evaluated to a non-true value",
                    idx + 1,
                    test.name
                ));
            }
            Err(err_msg) => {
                failures.push(format!(
                    "assertion {} in test '{}' failed with error: {}",
                    idx + 1,
                    test.name,
                    err_msg
                ));
            }
        }
    }

    TestResult {
        name: test.name.clone(),
        passed: failures.is_empty(),
        failures,
    }
}

/// Evaluate a single assertion expression.
///
/// Returns:
///   Ok(true)  – assertion evaluated to "true"
///   Ok(false) – assertion evaluated successfully but to a non-true value
///   Err(..)   – runtime error while evaluating the expression
fn eval_assertion(
    expr: &Expr,
    program: &Program,
    vars: &HashMap<String, String>,
) -> Result<bool, String> {
    // Use the new interpreter helper that returns a String.
    match eval::eval_body_expr(expr, program, vars) {
        Ok(value_str) => {
            let trimmed = value_str.trim();
            Ok(trimmed == "true")
        }
        Err(err) => Err(err.to_string()),
    }
}

/// Convenience helper: run all tests and return Err(..) if any failed.
///
/// This is useful for CLI commands like `shrimpl test` that want a
/// single success/failure status.
pub fn assert_program_tests_pass(program: &Program) -> Result<(), String> {
    let results = run_program_tests(program);

    let mut all_ok = true;
    let mut msg = String::new();

    for result in &results {
        if result.passed {
            continue;
        }

        all_ok = false;
        msg.push_str(&format!("Test '{}'\n", result.name));
        for failure in &result.failures {
            msg.push_str("  - ");
            msg.push_str(failure);
            msg.push('\n');
        }
    }

    if all_ok {
        Ok(())
    } else {
        Err(msg)
    }
}
