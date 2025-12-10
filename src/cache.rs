// src/cache.rs
//
// Simple in-memory TTL cache for Shrimpl.
//
// This is intentionally straightforward and safe to use from async
// contexts via a Mutex + Instant. It can be wrapped by interpreter
// builtins like:
//
//   cache_set(key, value, ttl_seconds)
//   cache_get(key)
//   cache_delete(key)

use std::collections::HashMap;
use std::time::{Duration, Instant};

use once_cell::sync::Lazy;
use serde_json::Value as JsonValue;
use tokio::sync::Mutex;

#[derive(Debug, Clone)]
struct CacheEntry {
    value: JsonValue,
    expires_at: Option<Instant>,
}

static GLOBAL_CACHE: Lazy<Mutex<HashMap<String, CacheEntry>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Set a cache key to a JSON value with optional TTL in seconds.
/// ttl_secs == None => no expiration.
pub async fn cache_set(key: String, value: JsonValue, ttl_secs: Option<u64>) {
    let expires_at = ttl_secs.map(|s| Instant::now() + Duration::from_secs(s));
    let entry = CacheEntry { value, expires_at };

    let mut cache = GLOBAL_CACHE.lock().await;
    cache.insert(key, entry);
}

/// Get a value from the cache if present and not expired.
pub async fn cache_get(key: &str) -> Option<JsonValue> {
    let mut cache = GLOBAL_CACHE.lock().await;

    if let Some(entry) = cache.get(key) {
        if let Some(deadline) = entry.expires_at {
            if Instant::now() > deadline {
                cache.remove(key);
                return None;
            }
        }
        return Some(entry.value.clone());
    }

    None
}

/// Delete a key from the cache, ignoring missing keys.
pub async fn cache_delete(key: &str) {
    let mut cache = GLOBAL_CACHE.lock().await;
    cache.remove(key);
}

/// Convenience helper for Shrimpl builtins using strings.
///
/// - `value_json` is a JSON string representation of the value.
/// - `ttl_secs` is optional TTL in seconds.
///
/// Returns `Ok(())` or an error string.
pub async fn cache_set_json(
    key: &str,
    value_json: &str,
    ttl_secs: Option<u64>,
) -> Result<(), String> {
    let value: JsonValue =
        serde_json::from_str(value_json).map_err(|e| format!("invalid JSON: {e}"))?;
    cache_set(key.to_string(), value, ttl_secs).await;
    Ok(())
}

/// Get a JSON string from cache, if present.
pub async fn cache_get_json(key: &str) -> Option<String> {
    cache_get(key).await.map(|v| v.to_string())
}
