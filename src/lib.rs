// src/lib.rs
//
// Library root for Shrimpl.
// Exposes shared modules so all binaries (CLI, LSP, tests) can use them.

pub mod ast;
pub mod docs;
pub mod interpreter;
pub mod parser;
