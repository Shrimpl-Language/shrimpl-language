// src/lockfile.rs
//
// Lightweight dependency / program lockfile for Shrimpl.
// Creates a `shrimpl.lock` JSON file capturing:
//   - Shrimpl CLI version
//   - Logical environment (SHRIMPL_ENV)
//   - Entry path (e.g. app.shr)
//   - SHA-256 hash of the entry file contents
//   - Timestamp (seconds since UNIX epoch)

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

const LOCKFILE_NAME: &str = "shrimpl.lock";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShrimplLock {
    pub shrimpl_version: String,
    pub environment: String,
    pub entry_path: String,
    pub entry_hash: String,
    pub generated_at: u64,
}

fn compute_hash(contents: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(contents.as_bytes());
    let bytes = hasher.finalize();
    hex::encode(bytes)
}

/// Load an existing lockfile from disk, if present.
///
/// This is primarily intended for future tooling / diagnostics and is not yet
/// wired into the main CLI flow, so we allow it to be "dead code" from Clippy's
/// perspective while still keeping it available as a public API.
#[allow(dead_code)]
pub fn load_lockfile() -> Option<ShrimplLock> {
    if !Path::new(LOCKFILE_NAME).exists() {
        return None;
    }
    let data = fs::read_to_string(LOCKFILE_NAME).ok()?;
    serde_json::from_str(&data).ok()
}

pub fn write_lockfile(
    shrimpl_version: &str,
    environment: &str,
    entry_path: &str,
    entry_contents: &str,
) {
    let hash = compute_hash(entry_contents);
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let lock = ShrimplLock {
        shrimpl_version: shrimpl_version.to_string(),
        environment: environment.to_string(),
        entry_path: entry_path.to_string(),
        entry_hash: hash,
        generated_at: ts,
    };

    let json = match serde_json::to_string_pretty(&lock) {
        Ok(j) => j,
        Err(err) => {
            eprintln!("[shrimpl-lock] failed to serialize lockfile: {}", err);
            return;
        }
    };

    let mut file = match fs::File::create(LOCKFILE_NAME) {
        Ok(f) => f,
        Err(err) => {
            eprintln!("[shrimpl-lock] failed to create {}: {}", LOCKFILE_NAME, err);
            return;
        }
    };

    if let Err(err) = file.write_all(json.as_bytes()) {
        eprintln!("[shrimpl-lock] failed to write {}: {}", LOCKFILE_NAME, err);
    }
}
