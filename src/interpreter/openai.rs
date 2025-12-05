// src/interpreter/openai.rs
//
// Small helper layer for calling OpenAI from Shrimpl.
// Provides:
//   - set_api_key(key)
//   - set_system_prompt(prompt)
//   - chat(user_message) -> String
//   - chat_json(user_message) -> serde_json::Value
//   - mcp_call(server_id, tool_name, args_json) -> serde_json::Value
//
// The API key is resolved from:
//   1. Environment variable SHRIMPL_OPENAI_API_KEY
//   2. Environment variable OPENAI_API_KEY
//   3. A key set at runtime via set_api_key()
//
// NOTE: The main evaluator currently uses its own OpenAI config and HTTP
// client. This module is kept as a standalone helper layer for potential
// reuse and is therefore not wired in yet.
//
// Because it is not yet referenced from the rest of the crate, we suppress
// dead-code warnings for this module so that `-D warnings` does not fail
// the build.

#![allow(dead_code)]

use serde_json::{json, Value};
use std::env;
use std::sync::{Mutex, OnceLock};

use reqwest::blocking::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use thiserror::Error;

#[derive(Debug)]
struct OpenAIConfig {
    api_key: Option<String>,
    system_prompt: String,
}

// Global config, initialized on first use.
static CONFIG: OnceLock<Mutex<OpenAIConfig>> = OnceLock::new();

fn config() -> &'static Mutex<OpenAIConfig> {
    CONFIG.get_or_init(|| {
        let env_key = env::var("SHRIMPL_OPENAI_API_KEY")
            .or_else(|_| env::var("OPENAI_API_KEY"))
            .ok();

        Mutex::new(OpenAIConfig {
            api_key: env_key,
            system_prompt: String::new(),
        })
    })
}

#[derive(Debug, Error)]
pub enum OpenAIError {
    #[error(
        "Missing OpenAI API key. Set SHRIMPL_OPENAI_API_KEY / OPENAI_API_KEY or call openai_set_api_key(key)."
    )]
    MissingApiKey,

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("OpenAI response malformed: {0}")]
    Malformed(String),
}

fn get_api_key() -> Result<String, OpenAIError> {
    let cfg = config().lock().unwrap();
    if let Some(k) = &cfg.api_key {
        Ok(k.clone())
    } else {
        Err(OpenAIError::MissingApiKey)
    }
}

pub fn set_api_key(key: &str) {
    let mut cfg = config().lock().unwrap();
    cfg.api_key = Some(key.to_string());
}

pub fn set_system_prompt(prompt: &str) {
    let mut cfg = config().lock().unwrap();
    cfg.system_prompt = prompt.to_string();
}

fn build_messages(user_message: &str) -> Vec<Value> {
    let cfg = config().lock().unwrap();
    let mut msgs = Vec::new();

    if !cfg.system_prompt.is_empty() {
        msgs.push(json!({
            "role": "system",
            "content": cfg.system_prompt
        }));
    }

    msgs.push(json!({
        "role": "user",
        "content": user_message
    }));

    msgs
}

const CHAT_URL: &str = "https://api.openai.com/v1/chat/completions";
const RESPONSES_URL: &str = "https://api.openai.com/v1/responses";

/// Call the Chat Completions API and return just the assistant content.
pub fn chat(user_message: &str) -> Result<String, OpenAIError> {
    let api_key = get_api_key()?;
    let client = Client::new();

    let body = json!({
        "model": "gpt-5.1",
        "messages": build_messages(user_message),
    });

    let resp = client
        .post(CHAT_URL)
        .header(CONTENT_TYPE, "application/json")
        .header(AUTHORIZATION, format!("Bearer {}", api_key))
        .json(&body)
        .send()?;

    let status = resp.status();
    let text = resp.text()?;

    if !status.is_success() {
        return Err(OpenAIError::Malformed(format!(
            "status {}: {}",
            status, text
        )));
    }

    let v: Value = serde_json::from_str(&text)?;
    let content = v["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| OpenAIError::Malformed("choices[0].message.content missing".to_string()))?;

    Ok(content.to_string())
}

/// Call Chat Completions and return the full JSON response.
pub fn chat_json(user_message: &str) -> Result<Value, OpenAIError> {
    let api_key = get_api_key()?;
    let client = Client::new();

    let body = json!({
        "model": "gpt-4o-mini",
        "messages": build_messages(user_message),
    });

    let resp = client
        .post(CHAT_URL)
        .header(CONTENT_TYPE, "application/json")
        .header(AUTHORIZATION, format!("Bearer {}", api_key))
        .json(&body)
        .send()?;

    let status = resp.status();
    let text = resp.text()?;

    if !status.is_success() {
        return Err(OpenAIError::Malformed(format!(
            "status {}: {}",
            status, text
        )));
    }

    let v: Value = serde_json::from_str(&text)?;
    Ok(v)
}

/// Very lightweight MCP/tools-style call using the Responses API.
/// This is generic on purpose: you pass `server_id`, `tool_name`, and `args_json`,
/// and we just forward them as structured text. You can refine this later.
pub fn mcp_call(server_id: &str, tool_name: &str, args_json: &str) -> Result<Value, OpenAIError> {
    let api_key = get_api_key()?;
    let client = Client::new();

    // Try to parse the args JSON; if it fails, just treat it as a string.
    let parsed_args: Value = serde_json::from_str(args_json).unwrap_or_else(|_| json!(args_json));

    let body = json!({
        "model": "gpt-4.1-mini",
        "input": format!(
            "MCP server: {} tool: {} args: {}",
            server_id,
            tool_name,
            parsed_args
        ),
    });

    let resp = client
        .post(RESPONSES_URL)
        .header(CONTENT_TYPE, "application/json")
        .header(AUTHORIZATION, format!("Bearer {}", api_key))
        .json(&body)
        .send()?;

    let status = resp.status();
    let text = resp.text()?;

    if !status.is_success() {
        return Err(OpenAIError::Malformed(format!(
            "status {}: {}",
            status, text
        )));
    }

    let v: Value = serde_json::from_str(&text)?;
    Ok(v)
}
