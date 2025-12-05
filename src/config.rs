// src/config.rs
//
// Environment-based configuration loader for Shrimpl CLI/runtime.
//
// Features:
// - Loads JSON from config/config.<env>.json where env = SHRIMPL_ENV or "dev".
// - Exposes server overrides (port, tls).
// - Exposes secret mappings (logical -> env var).
// - Exposes auth (JWT) configuration.
// - Exposes per-path validation schemas.
// - Exposes type annotations for functions (used by type checker).
// - Exposes generic key/value config for config_get/config_set.
//
// Example config/dev file (config/config.dev.json):
//
// {
//   "server": { "port": 3000, "tls": false },
//   "auth": {
//     "jwt_secret_env": "SHRIMPL_JWT_SECRET",
//     "protected_paths": ["/secure", "/admin"],
//     "allow_missing_on": ["/health"]
//   },
//   "validation": {
//     "schemas": {
//       "/login": {
//         "type": "object",
//         "required": ["email","password"],
//         "properties": {
//           "email": { "type":"string", "format":"email" },
//           "password": { "type":"string", "minLength": 8 }
//         }
//       }
//     }
//   },
//   "types": {
//     "functions": {
//       "add": {
//         "params": ["number", "number"],
//         "result": "number"
//       },
//       "greet": {
//         "params": ["string"],
//         "result": "string"
//       }
//     }
//   },
//   "secrets": {
//     "env": {
//       "OPENAI": "SHRIMPL_OPENAI_API_KEY"
//     }
//   },
//   "values": {
//     "greeting": "Hello Shrimpl",
//     "threshold": 0.75,
//     "debug": true
//   }
// }

use once_cell::sync::OnceCell;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::{env, fs, path::Path, sync::Mutex};

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ServerConfigFile {
    pub port: Option<u16>,
    pub tls: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct SecretsConfigFile {
    /// Logical secret name -> environment variable name.
    pub env: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct AuthConfigFile {
    /// Env var name that holds the HMAC/secret used to sign JWTs.
    pub jwt_secret_env: Option<String>,
    /// Paths that require a valid JWT (prefix match).
    pub protected_paths: Option<Vec<String>>,
    /// Paths that are always allowed even if protected_paths is used.
    pub allow_missing_on: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ValidationConfigFile {
    /// Path -> JSON schema (Draft 7-ish) used to validate JSON bodies.
    pub schemas: HashMap<String, Value>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct FunctionTypeFile {
    /// Parameter types by position: "number", "string", "bool", "any".
    #[allow(dead_code)]
    pub params: Vec<String>,
    /// Optional result type.
    #[allow(dead_code)]
    pub result: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct TypesConfigFile {
    /// Function name -> type info.
    #[allow(dead_code)]
    pub functions: HashMap<String, FunctionTypeFile>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct FileConfig {
    pub server: Option<ServerConfigFile>,
    pub secrets: Option<SecretsConfigFile>,
    pub auth: Option<AuthConfigFile>,
    pub validation: Option<ValidationConfigFile>,
    /// Optional static typing configuration for Shrimpl programs.
    #[allow(dead_code)]
    pub types: Option<TypesConfigFile>,
    /// Arbitrary key/value config for config_get/config_set.
    pub values: Option<HashMap<String, Value>>,
}

#[derive(Debug, Default)]
pub struct RuntimeConfig {
    pub file: FileConfig,
    pub env_name: String,
    pub values: HashMap<String, Value>,
}

static RUNTIME_CONFIG: OnceCell<Mutex<RuntimeConfig>> = OnceCell::new();

fn runtime() -> &'static Mutex<RuntimeConfig> {
    RUNTIME_CONFIG.get_or_init(|| Mutex::new(RuntimeConfig::default()))
}

/// Initialize configuration from disk (idempotent).
pub fn init() {
    let env_name = env::var("SHRIMPL_ENV").unwrap_or_else(|_| "dev".to_string());
    let file_name = format!("config.{}.json", env_name);
    let path = Path::new("config").join(file_name);

    let file_cfg: FileConfig = match fs::read_to_string(&path) {
        Ok(text) => match serde_json::from_str::<FileConfig>(&text) {
            Ok(cfg) => cfg,
            Err(err) => {
                eprintln!(
                    "[shrimpl-config] Failed to parse {}: {} (using defaults)",
                    path.display(),
                    err
                );
                FileConfig::default()
            }
        },
        Err(err) => {
            if path.exists() {
                eprintln!(
                    "[shrimpl-config] Failed to read {}: {} (using defaults)",
                    path.display(),
                    err
                );
            }
            FileConfig::default()
        }
    };

    let mut guard = runtime()
        .lock()
        .expect("shrimpl runtime config mutex poisoned");
    guard.env_name = env_name;
    guard.values = file_cfg.values.clone().unwrap_or_default();
    guard.file = file_cfg;
}

/// Current logical environment name (e.g. "dev", "prod").
pub fn env_name() -> String {
    runtime()
        .lock()
        .expect("shrimpl runtime config mutex poisoned")
        .env_name
        .clone()
}

/// Server overrides loaded from config file, if present.
pub fn server_section() -> Option<ServerConfigFile> {
    runtime()
        .lock()
        .expect("shrimpl runtime config mutex poisoned")
        .file
        .server
        .clone()
}

/// Auth config section, if present.
pub fn auth_section() -> Option<AuthConfigFile> {
    runtime()
        .lock()
        .expect("shrimpl runtime config mutex poisoned")
        .file
        .auth
        .clone()
}

/// Types config section, if present.
#[allow(dead_code)]
pub fn types_section() -> Option<TypesConfigFile> {
    runtime()
        .lock()
        .expect("shrimpl runtime config mutex poisoned")
        .file
        .types
        .clone()
}

/// Validation config section, if present.
pub fn validation_section() -> Option<ValidationConfigFile> {
    runtime()
        .lock()
        .expect("shrimpl runtime config mutex poisoned")
        .file
        .validation
        .clone()
}

/// Resolve a logical secret name to an environment-variable key using the
/// config file mapping, if present.
pub fn secret_env_from_file(logical: &str) -> Option<String> {
    runtime()
        .lock()
        .expect("shrimpl runtime config mutex poisoned")
        .file
        .secrets
        .as_ref()
        .and_then(|s| s.env.get(logical))
        .cloned()
}

/// Get a configuration value, if defined.
pub fn get_value(key: &str) -> Option<Value> {
    runtime()
        .lock()
        .expect("shrimpl runtime config mutex poisoned")
        .values
        .get(key)
        .cloned()
}

/// Set or override a configuration value at runtime.
pub fn set_value(key: &str, value: Value) {
    runtime()
        .lock()
        .expect("shrimpl runtime config mutex poisoned")
        .values
        .insert(key.to_string(), value);
}

/// Determine whether a key exists in configuration.
pub fn has_value(key: &str) -> bool {
    runtime()
        .lock()
        .expect("shrimpl runtime config mutex poisoned")
        .values
        .contains_key(key)
}

/// Apply server overrides from configuration onto a parsed Program.
pub fn apply_server_to_program(program: &mut crate::parser::ast::Program) {
    if let Some(section) = server_section() {
        if let Some(port) = section.port {
            program.server.port = port;
        }
        if let Some(tls) = section.tls {
            program.server.tls = tls;
        }
    }
}

/// Helper: load the JWT secret from the configured env var, if any.
pub fn jwt_secret_from_env() -> Option<String> {
    let guard = runtime()
        .lock()
        .expect("shrimpl runtime config mutex poisoned");
    if let Some(auth) = &guard.file.auth {
        if let Some(env_key) = &auth.jwt_secret_env {
            return std::env::var(env_key).ok();
        }
    }
    None
}

/// Helper: get validation schema for a given Shrimpl endpoint path, if defined.
///
/// The key is expected to be the Shrimpl path string, e.g. "/login" or "/users/:id".
pub fn validation_schema_for_path(path: &str) -> Option<Value> {
    validation_section().and_then(|v| v.schemas.get(path).cloned())
}
