// src/interpreter/http.rs
//
// Actix-Web HTTP server for Shrimpl.
// - Serves Shrimpl endpoints.
// - Exposes /__shrimpl/schema and /__shrimpl/ui (API Studio).
// - Exposes /__shrimpl/diagnostics (static analysis).
// - Exposes /__shrimpl/source (raw app.shr).

use crate::docs;
use crate::parser::ast::{Body, EndpointDecl, Method, Program};
use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;

use super::eval;

pub async fn run(program: Program) -> std::io::Result<()> {
    let program_for_server = program.clone();

    HttpServer::new(move || {
        let mut app = App::new();
        let program_cloned = program_for_server.clone();

        // User-defined endpoints from Shrimpl program
        for ep in program_cloned.endpoints.clone() {
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
                                let vars = collect_all_vars(&req);
                                respond(ep_here, program_here, vars)
                            }
                        }),
                    );
                }
                Method::Post => {
                    app = app.route(
                        &actix_path,
                        web::post().to(move |req: HttpRequest| {
                            let ep_here = endpoint_clone.clone();
                            let program_here = program_for_route.clone();
                            async move {
                                let vars = collect_all_vars(&req);
                                respond(ep_here, program_here, vars)
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
    })
    .bind(("0.0.0.0", program.server.port))?
    .run()
    .await
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
