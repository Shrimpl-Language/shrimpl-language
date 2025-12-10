// src/lib.rs
//
// Library crate for Shrimpl.
// Exposes the core compiler / interpreter modules so the CLI binary,
// LSP, and any other tools can share the same implementation.

pub mod ast;
pub mod cache;
pub mod concurrency;
pub mod config;
pub mod docs;
pub mod format;
pub mod interpreter;
pub mod lint;
pub mod loader;
pub mod lockfile;
pub mod metrics;
pub mod orm; // <-- NEW: make `crate::orm` available to interpreter/eval
pub mod parser;
pub mod tests;
pub mod typecheck;
