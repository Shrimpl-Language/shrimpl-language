// src/interpreter/mod.rs
//
// Top-level module for the Shrimpl interpreter.
// Re-exports `run` so main.rs can call `interpreter::run(...)`.

pub mod eval;
pub mod http;

pub use http::run;
