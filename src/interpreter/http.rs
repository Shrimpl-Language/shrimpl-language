// src/interpreter/http.rs
//
// Actix-Web HTTP server for Shrimpl.
// - Serves Shrimpl endpoints.
// - Exposes /__shrimpl/schema and /__shrimpl/ui (API Studio).
// - Exposes /__shrimpl/diagnostics (static analysis).
// - Exposes /__shrimpl/source (raw app.shr).
// - Exposes /health (built-in health check).
// - Supports optional TLS via `server <port> tls` and env certs.
// - Built-in JWT auth based on config.auth.*
// - Input validation + sanitization via config.validation.schemas (JSON Schema).
// - Structured JSON logging per HTTP request.

use crate::config;
use crate::docs;
use crate::parser::ast::{Body, EndpointDecl, Method, Program};
use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer};
use chrono::Utc;
use jsonschema::{Draft, JSONSchema};
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::time::Instant;

use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::ServerConfig as TlsServerConfig;
use rustls_pemfile::{certs, pkcs8_private_keys};
use std::io::BufReader;

use super::eval;

// --- JWT claims ---

#[derive(Debug, Deserialize)]
struct JwtClaims {
    pub sub: Option<String>,
    pub scope: Option<String>,
    pub role: Option<String>,
    #[allow(dead_code)]
    pub exp: Option<u64>,
}

// --- helpers: auth, validation, logging ---

fn path_requires_auth(path: &str) -> bool {
    let auth = match config::auth_section() {
        Some(a) => a,
        None => return false,
    };

    let allow_on = auth.allow_missing_on.unwrap_or_default();
    if allow_on.iter().any(|p| path.starts_with(p)) {
        return false;
    }

    let protected = auth.protected_paths.unwrap_or_default();
    protected.iter().any(|p| path.starts_with(p))
}

fn extract_bearer_token(req: &HttpRequest) -> Option<String> {
    let auth_header = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())?;
    let prefix = "Bearer ";
    if auth_header.starts_with(prefix) && auth_header.len() > prefix.len() {
        Some(auth_header[prefix.len()..].trim().to_string())
    } else {
        None
    }
}

fn verify_jwt(token: &str) -> Result<JwtClaims, String> {
    let secret = config::jwt_secret_from_env()
        .ok_or_else(|| "JWT secret not configured (auth.jwt_secret_env)".to_string())?;
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;

    decode::<JwtClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map(|data| data.claims)
    .map_err(|e| format!("invalid token: {}", e))
}

/// Return Ok(claims) if valid token or auth not required, Err(HttpResponse) on failure.
/// If auth is not required for this path, Ok(None).
fn verify_jwt_if_required(
    path: &str,
    req: &HttpRequest,
) -> Result<Option<JwtClaims>, HttpResponse> {
    if !path_requires_auth(path) {
        return Ok(None);
    }

    let token = match extract_bearer_token(req) {
        Some(t) => t,
        None => {
            let body = r#"{"error":"missing bearer token"}"#;
            return Err(HttpResponse::Unauthorized()
                .content_type("application/json; charset=utf-8")
                .body(body));
        }
    };

    match verify_jwt(&token) {
        Ok(claims) => Ok(Some(claims)),
        Err(msg) => Err(HttpResponse::Unauthorized()
            .content_type("application/json; charset=utf-8")
            .body(format!(r#"{{"error":"unauthorized","detail":"{}"}}"#, msg))),
    }
}

/// Validate and sanitize JSON request body for a Shrimpl path.
/// - If no schema is configured, returns Ok(body_string) unchanged.
/// - If schema exists, validates using jsonschema.
/// - On success, returns sanitized JSON serialized back to a string.
/// - On failure, returns Err(HttpResponse) with 400 status.
async fn validate_and_sanitize_body(
    path: &str,
    raw_body: web::Bytes,
) -> Result<String, HttpResponse> {
    let body_text = String::from_utf8_lossy(&raw_body).to_string();

    let schema_val = match config::validation_schema_for_path(path) {
        Some(s) => s,
        None => return Ok(body_text),
    };

    let mut json_val: Value = match serde_json::from_str(&body_text) {
        Ok(v) => v,
        Err(e) => {
            return Err(HttpResponse::BadRequest()
                .content_type("application/json; charset=utf-8")
                .body(format!(r#"{{"error":"invalid_json","detail":"{}"}}"#, e)));
        }
    };

    let compiled = match JSONSchema::options()
        .with_draft(Draft::Draft7)
        .compile(&schema_val)
    {
        Ok(c) => c,
        Err(e) => {
            // Misconfigured schema -> 500, not user's fault.
            return Err(HttpResponse::InternalServerError()
                .content_type("application/json; charset=utf-8")
                .body(format!(
                    r#"{{"error":"schema_compile_error","detail":"{}"}}"#,
                    e
                )));
        }
    };

    if let Err(errors) = compiled.validate(&json_val) {
        let first = errors.into_iter().next();
        let msg = match first {
            Some(e) => format!("{} at {}", e, e.instance_path),
            None => "validation failed".to_string(),
        };
        return Err(HttpResponse::BadRequest()
            .content_type("application/json; charset=utf-8")
            .body(format!(
                r#"{{"error":"validation_failed","detail":"{}"}}"#,
                msg
            )));
    }

    sanitize_json(&mut json_val);
    let sanitized = serde_json::to_string(&json_val).unwrap_or(body_text);
    Ok(sanitized)
}

/// Simple sanitization: trim strings, recurse into arrays/objects.
fn sanitize_json(val: &mut Value) {
    match val {
        Value::String(s) => {
            *s = s.trim().to_string();
        }
        Value::Array(arr) => {
            for v in arr {
                sanitize_json(v);
            }
        }
        Value::Object(map) => {
            for (_k, v) in map.iter_mut() {
                sanitize_json(v);
            }
        }
        _ => {}
    }
}

fn log_request(
    path: &str,
    method: &str,
    status: u16,
    client: &str,
    elapsed_ms: u128,
    auth_ok: bool,
) {
    let payload = serde_json::json!({
        "ts": Utc::now().to_rfc3339(),
        "level": "info",
        "kind": "http-request",
        "method": method,
        "path": path,
        "status": status,
        "client": client,
        "elapsed_ms": elapsed_ms,
        "auth_ok": auth_ok
    });
    println!("{}", payload);
}

pub async fn run(program: Program) -> std::io::Result<()> {
    // Take server configuration from the original Program
    let addr = ("0.0.0.0", program.server.port);
    let tls_enabled = program.server.tls;

    // Clone once for moving into the Actix factory closure
    let program_for_server = program.clone();

    let factory = move || {
        let mut app = App::new();
        let program_cloned = program_for_server.clone();

        // Built-in health check endpoint
        app = app.route(
            "/health",
            web::get().to(|| async {
                HttpResponse::Ok()
                    .content_type("application/json; charset=utf-8")
                    .body(r#"{"status":"ok"}"#)
            }),
        );

        // User-defined endpoints from Shrimpl program
        let endpoints_snapshot = program_cloned.endpoints.clone();
        for ep in endpoints_snapshot {
            let actix_path = convert_path_for_actix(&ep.path);
            let endpoint_clone = ep.clone();
            let program_for_route = program_cloned.clone();

            match ep.method {
                Method::Get => {
                    app = app.route(
                        &actix_path,
                        web::get().to(move |req: HttpRequest| {
                            let ep_here = endpoint_clone.clone();
                            let program_here = program_for_route.clone();
                            async move {
                                let started = Instant::now();
                                let path = ep_here.path.clone();
                                let method = "GET";
                                let client = req
                                    .connection_info()
                                    .realip_remote_addr()
                                    .unwrap_or("unknown")
                                    .to_string();

                                let jwt_result = verify_jwt_if_required(&path, &req);
                                let claims_opt = match jwt_result {
                                    Ok(c) => c,
                                    Err(resp) => {
                                        log_request(
                                            &path,
                                            method,
                                            resp.status().as_u16(),
                                            &client,
                                            started.elapsed().as_millis(),
                                            false,
                                        );
                                        return resp;
                                    }
                                };

                                // Collect vars from path + query
                                let mut vars = collect_all_vars(&req);

                                // Inject claims when present
                                if let Some(claims) = claims_opt.as_ref() {
                                    if let Some(sub) = &claims.sub {
                                        vars.insert("jwt_sub".to_string(), sub.clone());
                                    }
                                    if let Some(scope) = &claims.scope {
                                        vars.insert("jwt_scope".to_string(), scope.clone());
                                    }
                                    if let Some(role) = &claims.role {
                                        vars.insert("jwt_role".to_string(), role.clone());
                                    }
                                }

                                let resp = respond(ep_here, program_here, vars);
                                let status = resp.status().as_u16();
                                log_request(
                                    &path,
                                    method,
                                    status,
                                    &client,
                                    started.elapsed().as_millis(),
                                    claims_opt.is_some(),
                                );
                                resp
                            }
                        }),
                    );
                }
                Method::Post => {
                    app = app.route(
                        &actix_path,
                        web::post().to(move |req: HttpRequest, body: web::Bytes| {
                            let ep_here = endpoint_clone.clone();
                            let program_here = program_for_route.clone();
                            async move {
                                let started = Instant::now();
                                let path = ep_here.path.clone();
                                let method = "POST";
                                let client = req
                                    .connection_info()
                                    .realip_remote_addr()
                                    .unwrap_or("unknown")
                                    .to_string();

                                let jwt_result = verify_jwt_if_required(&path, &req);
                                let claims_opt = match jwt_result {
                                    Ok(c) => c,
                                    Err(resp) => {
                                        log_request(
                                            &path,
                                            method,
                                            resp.status().as_u16(),
                                            &client,
                                            started.elapsed().as_millis(),
                                            false,
                                        );
                                        return resp;
                                    }
                                };

                                // Validate + sanitize JSON body (if schema exists)
                                let body_text_res = validate_and_sanitize_body(&path, body).await;
                                let body_text = match body_text_res {
                                    Ok(t) => t,
                                    Err(resp) => {
                                        log_request(
                                            &path,
                                            method,
                                            resp.status().as_u16(),
                                            &client,
                                            started.elapsed().as_millis(),
                                            claims_opt.is_some(),
                                        );
                                        return resp;
                                    }
                                };

                                // Collect vars from path + query
                                let mut vars = collect_all_vars(&req);

                                // Insert request body under "body" for Shrimpl code
                                vars.insert("body".to_string(), body_text);

                                // Inject claims when present
                                if let Some(claims) = claims_opt.as_ref() {
                                    if let Some(sub) = &claims.sub {
                                        vars.insert("jwt_sub".to_string(), sub.clone());
                                    }
                                    if let Some(scope) = &claims.scope {
                                        vars.insert("jwt_scope".to_string(), scope.clone());
                                    }
                                    if let Some(role) = &claims.role {
                                        vars.insert("jwt_role".to_string(), role.clone());
                                    }
                                }

                                let resp = respond(ep_here, program_here, vars);
                                let status = resp.status().as_u16();
                                log_request(
                                    &path,
                                    method,
                                    status,
                                    &client,
                                    started.elapsed().as_millis(),
                                    claims_opt.is_some(),
                                );
                                resp
                            }
                        }),
                    );
                }
            }
        }

        // Docs + schema + diagnostics + source
        let program_schema = program_cloned.clone();
        let program_ui = program_cloned.clone();
        let program_diag = program_cloned.clone();

        app = app
            .route(
                "/__shrimpl/schema",
                web::get().to(move || {
                    let program_here = program_schema.clone();
                    async move {
                        let json: Value = docs::build_schema(&program_here);
                        HttpResponse::Ok().json(json)
                    }
                }),
            )
            .route(
                "/__shrimpl/diagnostics",
                web::get().to(move || {
                    let program_here = program_diag.clone();
                    async move {
                        let json: Value = docs::build_diagnostics(&program_here);
                        HttpResponse::Ok().json(json)
                    }
                }),
            )
            .route(
                "/__shrimpl/ui",
                web::get().to(move || {
                    let _program_here = program_ui.clone(); // reserved for future customization
                    async move {
                        HttpResponse::Ok()
                            .content_type("text/html; charset=utf-8")
                            .body(docs::docs_html())
                    }
                }),
            )
            .route(
                "/__shrimpl/source",
                web::get().to(|| async {
                    match fs::read_to_string("app.shr") {
                        Ok(text) => HttpResponse::Ok()
                            .content_type("text/plain; charset=utf-8")
                            .body(text),
                        Err(_) => {
                            HttpResponse::InternalServerError().body("Could not read app.shr")
                        }
                    }
                }),
            );

        app
    };

    if tls_enabled {
        let tls_cfg = load_tls_config()?;
        HttpServer::new(factory)
            .bind_rustls_0_23(addr, tls_cfg)?
            .run()
            .await
    } else {
        HttpServer::new(factory).bind(addr)?.run().await
    }
}

fn load_tls_config() -> std::io::Result<TlsServerConfig> {
    let cert_path = env::var("SHRIMPL_TLS_CERT").unwrap_or_else(|_| "cert.pem".to_string());
    let key_path = env::var("SHRIMPL_TLS_KEY").unwrap_or_else(|_| "key.pem".to_string());

    let cert_file = fs::File::open(&cert_path)?;
    let key_file = fs::File::open(&key_path)?;

    let mut cert_reader = BufReader::new(cert_file);
    let mut key_reader = BufReader::new(key_file);

    // rustls-pemfile 2.x APIs:
    // - certs() -> impl Iterator<Item = Result<CertificateDer<'static>, _>>
    // - pkcs8_private_keys() -> impl Iterator<Item = Result<PrivatePkcs8KeyDer<'static>, _>>
    let certs: Vec<CertificateDer<'static>> = certs(&mut cert_reader)
        .collect::<Result<_, _>>()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    let mut pkcs8_keys: Vec<PrivatePkcs8KeyDer<'static>> = pkcs8_private_keys(&mut key_reader)
        .collect::<Result<_, _>>()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    let pkcs8_key = pkcs8_keys
        .drain(..)
        .next()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "no private keys"))?;

    // Convert PrivatePkcs8KeyDer -> PrivateKeyDer for rustls 0.23
    let key: PrivateKeyDer<'static> = pkcs8_key.into();

    let cfg = TlsServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, format!("{e}")))?;

    Ok(cfg)
}

// Convert "/hello/:name" to "/hello/{name}" for actix
fn convert_path_for_actix(shrimpl_path: &str) -> String {
    let mut parts = Vec::new();
    for part in shrimpl_path.split('/') {
        if part.starts_with(':') && part.len() > 1 {
            parts.push(format!("{{{}}}", &part[1..]));
        } else {
            parts.push(part.to_string());
        }
    }
    parts.join("/")
}

// Collect path params
fn collect_path_params(req: &HttpRequest) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for (k, v) in req.match_info().iter() {
        map.insert(k.to_string(), v.to_string());
    }
    map
}

// Collect query params (?k=v&x=y)
fn collect_query_params(req: &HttpRequest) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let qs = req.query_string();
    if qs.is_empty() {
        return map;
    }
    for part in qs.split('&') {
        if part.is_empty() {
            continue;
        }
        let mut split = part.splitn(2, '=');
        let key = split.next().unwrap_or("").to_string();
        let value = split.next().unwrap_or("").to_string();
        if !key.is_empty() {
            map.insert(key, value);
        }
    }
    map
}

// Merge both (path overrides query on conflict)
fn collect_all_vars(req: &HttpRequest) -> HashMap<String, String> {
    let mut vars = collect_query_params(req);
    for (k, v) in collect_path_params(req) {
        vars.insert(k, v);
    }
    vars
}

fn respond(
    endpoint: EndpointDecl,
    program: Program,
    vars: HashMap<String, String>,
) -> HttpResponse {
    match endpoint.body {
        Body::JsonRaw(json_str) => match serde_json::from_str::<Value>(&json_str) {
            Ok(json) => HttpResponse::Ok().json(json),
            Err(err) => HttpResponse::InternalServerError().body(format!(
                "Invalid JSON in Shrimpl endpoint '{}': {}",
                endpoint.path, err
            )),
        },
        Body::TextExpr(expr) => match eval::eval_body_expr(&expr, &program, &vars) {
            Ok(text) => HttpResponse::Ok().body(text),
            Err(err) => HttpResponse::InternalServerError().body(err),
        },
    }
}
