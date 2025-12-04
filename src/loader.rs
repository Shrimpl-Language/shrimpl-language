// src/loader.rs
//
// Simple module loader for Shrimpl.
// Supports textual imports of the form:
//
//   import "relative/path/to/file.shr"
//
// The loader recursively inlines imported files, de-duplicating by canonical
// path to avoid cycles.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Load a Shrimpl entry file and inline all `import "..."` directives.
pub fn load_with_imports(entry: &str) -> io::Result<String> {
    let mut visited = HashSet::<PathBuf>::new();
    let mut out = String::new();
    let entry_path = Path::new(entry);
    load_recursive(entry_path, &mut visited, &mut out)?;
    Ok(out)
}

fn load_recursive(path: &Path, visited: &mut HashSet<PathBuf>, out: &mut String) -> io::Result<()> {
    let canonical = path.canonicalize()?;
    if !visited.insert(canonical.clone()) {
        // Already loaded; avoid cycles.
        return Ok(());
    }

    let text = fs::read_to_string(&canonical)?;
    let dir = canonical
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));

    for line in text.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("import ") {
            // Expect import "file.shr"
            if let Some((path_str, trailing)) = extract_import_path(rest) {
                if trailing.trim().is_empty() {
                    let child = dir.join(path_str);
                    load_recursive(&child, visited, out)?;
                    // Do not emit the import line itself.
                    continue;
                }
            }
        }

        out.push_str(line);
        out.push('\n');
    }

    Ok(())
}

/// Extract the string between double quotes in something like `"file.shr"`
/// or `"file.shr" # comment`.
fn extract_import_path(rest: &str) -> Option<(String, &str)> {
    let mut chars = rest.chars().peekable();
    while let Some(c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else {
            break;
        }
    }
    if chars.next()? != '"' {
        return None;
    }
    let mut s = String::new();
    while let Some(c) = chars.next() {
        if c == '"' {
            let remaining: String = chars.collect();
            return Some((s, remaining.as_str()));
        }
        s.push(c);
    }
    None
}
