// src/docs.rs
//
// Shrimpl API Studio: schema, diagnostics, and HTML UI.

use crate::ast::{Body, Expr, Method, Program};
use serde_json::{json, Value};
use std::collections::HashSet;

pub fn build_schema(program: &Program) -> Value {
    let endpoints: Vec<Value> = program
        .endpoints
        .iter()
        .map(|ep| {
            let method_str = match ep.method {
                Method::Get => "GET",
                Method::Post => "POST",
            };
            let body_kind = match ep.body {
                Body::TextExpr(_) => "text",
                Body::JsonRaw(_) => "json",
            };
            json!({
                "method": method_str,
                "path": ep.path,
                "bodyKind": body_kind,
            })
        })
        .collect();

    json!({
        "server": { "port": program.server.port },
        "endpoints": endpoints
    })
}

/// Build simple static diagnostics from AST:
/// - unused path params in endpoints
/// - unused parameters in functions
/// - unused parameters in methods
/// - duplicate (method, path) endpoint definitions
pub fn build_diagnostics(program: &Program) -> Value {
    let mut warnings = Vec::<Value>::new();
    let errors: Vec<Value> = Vec::new();

    // 1) Duplicate endpoints (same method + path)
    let mut seen = std::collections::HashSet::<(String, String)>::new();
    for ep in &program.endpoints {
        let m = match ep.method {
            Method::Get => "GET".to_string(),
            Method::Post => "POST".to_string(),
        };
        let key = (m.clone(), ep.path.clone());
        if !seen.insert(key.clone()) {
            warnings.push(json!({
                "kind": "warning",
                "scope": "endpoint",
                "name": ep.path,
                "message": format!("Duplicate endpoint for {} {}", m, ep.path),
            }));
        }
    }

    // 2) Endpoint: path params that are never used in body
    for ep in &program.endpoints {
        let path_params: Vec<String> = ep
            .path
            .split('/')
            .filter(|p| p.starts_with(':') && p.len() > 1)
            .map(|p| p[1..].to_string())
            .collect();

        if path_params.is_empty() {
            continue;
        }

        let mut used_vars = HashSet::<String>::new();
        if let Body::TextExpr(ref expr) = ep.body {
            collect_vars_expr(expr, &mut used_vars);
        }

        for param in path_params {
            if !used_vars.contains(&param) {
                warnings.push(json!({
                    "kind": "warning",
                    "scope": "endpoint",
                    "name": ep.path,
                    "message": format!("Path parameter :{} is never used in this endpoint body", param),
                }));
            }
        }
    }

    // 3) Functions: unused parameters
    for func in program.functions.values() {
        let mut used = HashSet::<String>::new();
        collect_vars_expr(&func.body, &mut used);

        for param in &func.params {
            if !used.contains(param) {
                warnings.push(json!({
                    "kind": "warning",
                    "scope": "function",
                    "name": func.name,
                    "message": format!("Parameter '{}' is never used in function body", param),
                }));
            }
        }
    }

    // 4) Methods: unused parameters
    for class in program.classes.values() {
        for method in class.methods.values() {
            let mut used = HashSet::<String>::new();
            collect_vars_expr(&method.body, &mut used);

            for param in &method.params {
                if !used.contains(param) {
                    warnings.push(json!({
                        "kind": "warning",
                        "scope": "method",
                        "name": format!("{}.{}", class.name, method.name),
                        "message": format!("Parameter '{}' is never used in method body", param),
                    }));
                }
            }
        }
    }

    json!({
        "errors": errors,
        "warnings": warnings,
    })
}

// Walk expression tree and collect variable names.
fn collect_vars_expr(expr: &Expr, out: &mut HashSet<String>) {
    match expr {
        // Variable reference: record the name
        Expr::Var(name) => {
            out.insert(name.clone());
        }

        // Literals: they don't contain variable references
        Expr::Number(_) | Expr::Str(_) | Expr::Bool(_) => {}

        // List literal – walk each element
        Expr::List(items) => {
            for e in items {
                collect_vars_expr(e, out);
            }
        }

        // Map literal – walk each value expression (keys are plain strings)
        Expr::Map(entries) => {
            for (_k, v) in entries {
                collect_vars_expr(v, out);
            }
        }

        // Binary operator: recurse into both sides
        Expr::Binary { left, right, .. } => {
            collect_vars_expr(left, out);
            collect_vars_expr(right, out);
        }

        // Function call: recurse into all arguments
        Expr::Call { args, .. } => {
            for a in args {
                collect_vars_expr(a, out);
            }
        }

        // Class method call: recurse into all arguments
        Expr::MethodCall { args, .. } => {
            for a in args {
                collect_vars_expr(a, out);
            }
        }

        // Control-flow expressions: walk all branches and the else body
        Expr::If {
            branches,
            else_branch,
        } => {
            for (cond, body) in branches {
                collect_vars_expr(cond, out);
                collect_vars_expr(body, out);
            }
            if let Some(else_expr) = else_branch {
                collect_vars_expr(else_expr, out);
            }
        }

        // Repeat: walk repeat count and body expression
        Expr::Repeat { count, body } => {
            collect_vars_expr(count, out);
            collect_vars_expr(body, out);
        }

        // Try expression: walk try, catch, and finally bodies so diagnostics
        // remain complete when new control-flow is introduced.
        Expr::Try {
            try_body,
            catch_var: _,
            catch_body,
            finally_body,
        } => {
            collect_vars_expr(try_body, out);
            if let Some(catch_expr) = catch_body {
                collect_vars_expr(catch_expr, out);
            }
            if let Some(finally_expr) = finally_body {
                collect_vars_expr(finally_expr, out);
            }
        }
    }
}

pub fn docs_html() -> &'static str {
    DOCS_HTML
}

const DOCS_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <title>Shrimpl API UI</title>
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <style>
    :root {
      --bg: #050816;
      --bg-panel: #0f172a;
      --bg-panel-soft: #111827;
      --accent: #38bdf8;
      --accent-soft: rgba(56,189,248,0.15);
      --text: #e5e7eb;
      --text-soft: #9ca3af;
      --border: #1f2937;
      --radius-lg: 12px;
      --error: #f97373;
      --warning: #facc15;
    }

    * { box-sizing: border-box; }

    body {
      margin: 0;
      font-family: system-ui, -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
      background: radial-gradient(circle at top, #0b1120 0, #020617 55%, #000 100%);
      color: var(--text);
    }

    #app {
      display: flex;
      flex-direction: column;
      height: 100vh;
    }

    header {
      padding: 0.75rem 1.5rem;
      border-bottom: 1px solid var(--border);
      background: linear-gradient(90deg, rgba(56,189,248,0.1), transparent);
      display: flex;
      align-items: center;
      justify-content: space-between;
    }

    header h1 {
      font-size: 1rem;
      letter-spacing: 0.12em;
      text-transform: uppercase;
      color: var(--text-soft);
      margin: 0;
      display: flex;
      align-items: center;
      gap: 0.5rem;
    }

    header h1 span.logo {
      width: 22px;
      height: 22px;
      border-radius: 999px;
      background: radial-gradient(circle at 30% 15%, #e5e7eb 0, #38bdf8 40%, #0ea5e9 70%, #0369a1 100%);
      box-shadow: 0 0 25px rgba(56,189,248,0.6);
    }

    header small {
      font-size: 0.75rem;
      color: var(--text-soft);
    }

    #layout {
      flex: 1;
      display: grid;
      grid-template-columns: minmax(260px, 340px) 1fr;
      min-height: 0;
    }

    aside {
      border-right: 1px solid var(--border);
      background: linear-gradient(180deg, rgba(15,23,42,0.9), rgba(15,23,42,0.98));
      padding: 0.75rem;
      overflow-y: auto;
    }

    main {
      padding: 0.75rem 1rem;
      overflow-y: auto;
    }

    .section-title {
      font-size: 0.75rem;
      text-transform: uppercase;
      letter-spacing: 0.16em;
      color: var(--text-soft);
      margin: 0 0 0.5rem;
    }

    .endpoint-list {
      display: flex;
      flex-direction: column;
      gap: 0.35rem;
    }

    .endpoint-item {
      display: flex;
      gap: 0.5rem;
      align-items: center;
      padding: 0.4rem 0.6rem;
      border-radius: 999px;
      border: 1px solid transparent;
      background: transparent;
      color: var(--text-soft);
      cursor: pointer;
      font-size: 0.8rem;
      transition: all 0.16s ease;
    }

    .endpoint-item:hover {
      border-color: var(--border);
      background: rgba(15,23,42,0.9);
      color: var(--text);
    }

    .endpoint-item.active {
      border-color: rgba(56,189,248,0.7);
      background: var(--accent-soft);
      color: var(--text);
      box-shadow: 0 0 0 1px rgba(56,189,248,0.4);
    }

    .method-pill {
      padding: 0.1rem 0.5rem;
      border-radius: 999px;
      font-size: 0.7rem;
      font-weight: 600;
      letter-spacing: 0.08em;
      text-transform: uppercase;
      background: rgba(34,197,94,0.08);
      color: #4ade80;
      border: 1px solid rgba(34,197,94,0.5);
      flex-shrink: 0;
    }

    .method-pill.POST {
      background: rgba(249,115,22,0.12);
      color: #fb923c;
      border-color: rgba(249,115,22,0.55);
    }

    .path {
      font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace;
      font-size: 0.78rem;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .badge {
      font-size: 0.65rem;
      padding: 0.1rem 0.3rem;
      border-radius: 999px;
      border: 1px solid var(--border);
      color: var(--text-soft);
      background: rgba(15,23,42,0.9);
    }

    .panel {
      background: rgba(15,23,42,0.98);
      border-radius: var(--radius-lg);
      border: 1px solid var(--border);
      padding: 0.75rem 0.9rem;
      box-shadow: 0 18px 40px rgba(15,23,42,0.7);
      margin-bottom: 0.75rem;
    }

    .panel-header {
      display: flex;
      align-items: baseline;
      justify-content: space-between;
      margin-bottom: 0.35rem;
      gap: 0.5rem;
    }

    .panel-header h2 {
      font-size: 0.9rem;
      margin: 0;
      display: flex;
      align-items: center;
      gap: 0.4rem;
    }

    .chip {
      font-size: 0.7rem;
      padding: 0.15rem 0.45rem;
      border-radius: 999px;
      border: 1px solid var(--border);
      color: var(--text-soft);
    }

    .field-row {
      display: flex;
      gap: 0.5rem;
      margin-top: 0.4rem;
      flex-wrap: wrap;
    }

    label {
      font-size: 0.75rem;
      color: var(--text-soft);
      display: block;
      margin-bottom: 0.1rem;
    }

    input[type="text"] {
      width: 100%;
      padding: 0.38rem 0.55rem;
      border-radius: 8px;
      border: 1px solid var(--border);
      background: var(--bg-panel-soft);
      color: var(--text);
      font-size: 0.8rem;
      font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace;
    }

    input[type="text"]:focus {
      outline: none;
      border-color: var(--accent);
      box-shadow: 0 0 0 1px rgba(56,189,248,0.4);
    }

    button.send-btn {
      padding: 0.4rem 0.9rem;
      border-radius: 999px;
      border: none;
      background: linear-gradient(115deg, #22d3ee, #0ea5e9, #6366f1);
      color: white;
      font-size: 0.78rem;
      font-weight: 600;
      letter-spacing: 0.08em;
      text-transform: uppercase;
      cursor: pointer;
      box-shadow: 0 10px 25px rgba(37,99,235,0.6);
      display: inline-flex;
      align-items: center;
      gap: 0.4rem;
    }

    button.send-btn:hover { filter: brightness(1.06); }
    button.send-btn:active {
      transform: translateY(1px);
      box-shadow: 0 6px 16px rgba(37,99,235,0.7);
    }

    .response-meta {
      font-size: 0.72rem;
      color: var(--text-soft);
      display: flex;
      justify-content: space-between;
      gap: 0.5rem;
      margin-bottom: 0.4rem;
    }

    pre {
      margin: 0;
      padding: 0.6rem 0.7rem;
      border-radius: 8px;
      background: #020617;
      border: 1px solid var(--border);
      font-size: 0.78rem;
      line-height: 1.4;
      overflow-x: auto;
      font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace;
      color: #e5e7eb;
    }

    .pill {
      padding: 0.1rem 0.4rem;
      border-radius: 999px;
      font-size: 0.68rem;
      border: 1px solid var(--border);
      color: var(--text-soft);
    }
    .pill-ok {
      color: #4ade80;
      border-color: rgba(74,222,128,0.7);
    }
    .pill-error {
      color: #f97373;
      border-color: rgba(248,113,113,0.8);
    }

    /* Code highlighting */
    .code-block {
      font-size: 0.78rem;
      background: #020617;
      border-radius: 8px;
      border: 1px solid var(--border);
      padding: 0.5rem 0.6rem;
      overflow-x: auto;
      font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace;
      white-space: pre;
    }
    .tok-kw      { color: #38bdf8; }
    .tok-method  { color: #22c55e; }
    .tok-string  { color: #f97316; }
    .tok-number  { color: #eab308; }
    .tok-comment { color: #6b7280; font-style: italic; }

    /* Diagnostics */
    .diag-list {
      margin: 0.2rem 0 0;
      padding: 0;
      list-style: none;
      font-size: 0.78rem;
    }
    .diag-item {
      display: flex;
      align-items: baseline;
      gap: 0.4rem;
      padding: 0.2rem 0;
      border-bottom: 1px solid rgba(15,23,42,0.7);
    }
    .diag-item:last-child {
      border-bottom: none;
    }
    .diag-kind {
      font-size: 0.7rem;
      text-transform: uppercase;
      letter-spacing: 0.08em;
      padding: 0.08rem 0.4rem;
      border-radius: 999px;
      border: 1px solid var(--border);
      flex-shrink: 0;
    }
    .diag-kind-warning {
      border-color: rgba(250,204,21,0.7);
      color: #facc15;
      background: rgba(250,204,21,0.08);
    }
    .diag-kind-error {
      border-color: rgba(248,113,113,0.7);
      color: #f97373;
      background: rgba(248,113,113,0.08);
    }
    .diag-scope {
      font-size: 0.7rem;
      color: var(--text-soft);
      flex-shrink: 0;
    }
    .diag-message {
      font-size: 0.78rem;
      color: var(--text);
    }

    @media (max-width: 900px) {
      #layout {
        grid-template-columns: 1fr;
      }
      aside {
        border-right: none;
        border-bottom: 1px solid var(--border);
      }
    }
  </style>
</head>
<body>
  <div id="app">
    <header>
      <h1><span class="logo"></span> SHRIMPL API STUDIO</h1>
      <small id="header-meta"></small>
    </header>
    <div id="layout">
      <aside>
        <p class="section-title">Endpoints</p>
        <div id="endpoint-list" class="endpoint-list"></div>
      </aside>
      <main>
        <div id="request-panel" class="panel" style="display:none;"></div>
        <div id="response-panel" class="panel" style="display:none;"></div>
        <div id="code-panel" class="panel"></div>
        <div id="diag-panel" class="panel"></div>
      </main>
    </div>
  </div>

  <script>
    let schema = null;
    let current = null;
    let sourceText = "";
    let diagnostics = { errors: [], warnings: [] };

    async function loadStudio() {
      const [schemaRes, diagRes, srcRes] = await Promise.all([
        fetch('/__shrimpl/schema'),
        fetch('/__shrimpl/diagnostics'),
        fetch('/__shrimpl/source')
      ]);

      schema = await schemaRes.json();
      try {
        diagnostics = await diagRes.json();
      } catch (_) {
        diagnostics = { errors: [], warnings: [] };
      }
      try {
        sourceText = await srcRes.text();
      } catch (_) {
        sourceText = "";
      }

      const port = schema.server && schema.server.port ? schema.server.port : '?';
      const count = schema.endpoints ? schema.endpoints.length : 0;
      document.getElementById('header-meta').textContent =
        'Port ' + port + ' • ' + count + ' endpoint(s)';

      renderEndpointList();
      renderCodePanel();
      renderDiagnosticsPanel();
    }

    function renderEndpointList() {
      const container = document.getElementById('endpoint-list');
      container.innerHTML = '';
      if (!schema || !schema.endpoints) return;

      schema.endpoints.forEach((ep, idx) => {
        const btn = document.createElement('button');
        btn.type = 'button';
        btn.className = 'endpoint-item';
        btn.dataset.index = idx;

        const methodSpan = document.createElement('span');
        methodSpan.className = 'method-pill ' + ep.method;
        methodSpan.textContent = ep.method;

        const pathSpan = document.createElement('span');
        pathSpan.className = 'path';
        pathSpan.textContent = ep.path;

        const bodyBadge = document.createElement('span');
        bodyBadge.className = 'badge';
        bodyBadge.textContent = ep.bodyKind === 'json' ? 'JSON' : 'Text';

        btn.appendChild(methodSpan);
        btn.appendChild(pathSpan);
        btn.appendChild(bodyBadge);

        btn.addEventListener('click', () => selectEndpoint(idx));
        container.appendChild(btn);
      });

      if (schema.endpoints.length > 0) {
        selectEndpoint(0);
      }
    }

    function selectEndpoint(index) {
      if (!schema || !schema.endpoints) return;
      current = schema.endpoints[index];

      document.querySelectorAll('.endpoint-item').forEach(btn => {
        btn.classList.toggle('active', Number(btn.dataset.index) === index);
      });

      renderRequestPanel();
    }

    function renderRequestPanel() {
      const panel = document.getElementById('request-panel');
      if (!current) {
        panel.style.display = 'none';
        return;
      }
      panel.style.display = 'block';

      const pathParams = extractPathParams(current.path);

      let html = '';
      html += '<div class="panel-header">';
      html += '  <h2><span class="method-pill ' + current.method + '">' + current.method + '</span><span class="path">' + current.path + '</span></h2>';
      html += '  <span class="chip">' + (current.bodyKind === 'json' ? 'JSON endpoint' : 'Text endpoint') + '</span>';
      html += '</div>';

      if (pathParams.length > 0) {
        html += '<div class="field-row">';
        pathParams.forEach(p => {
          html += '<div style="min-width:140px;flex:1;">';
          html += '  <label>Path param <strong>:' + p + '</strong></label>';
          html += '  <input type="text" id="param-' + p + '" placeholder="' + p + ' value" />';
          html += '</div>';
        });
        html += '</div>';
      }

      html += '<div class="field-row" style="margin-top:0.4rem;">';
      html += '  <div style="flex:2;min-width:180px;">';
      html += '    <label>Extra query (?key=value&foo=bar)</label>';
      html += '    <input type="text" id="query-string" placeholder="name=Aisen&debug=true" />';
      html += '  </div>';
      html += '</div>';

      html += '<div style="margin-top:0.6rem;">';
      html += '  <button class="send-btn" onclick="sendRequest()">';
      html += '    <span>Send</span><span>⮕</span>';
      html += '</div>';

      panel.innerHTML = html;
    }

    function extractPathParams(path) {
      return path
        .split('/')
        .filter(p => p.startsWith(':'))
        .map(p => p.substring(1));
    }

    async function sendRequest() {
      if (!current) return;

      const pathParams = extractPathParams(current.path);
      let urlPath = current.path;

      pathParams.forEach(p => {
        const input = document.getElementById('param-' + p);
        const value = (input && input.value) ? input.value : '';
        urlPath = urlPath.replace(':' + p, encodeURIComponent(value));
      });

      const qs = document.getElementById('query-string').value.trim();
      if (qs) {
        urlPath += (urlPath.includes('?') ? '&' : '?') + qs;
      }

      const fullUrl = urlPath;

      let res, bodyText;
      let ok = false;
      let status = '';
      try {
        res = await fetch(fullUrl, { method: current.method });
        status = res.status + ' ' + res.statusText;
        bodyText = await res.text();
        ok = res.ok;
      } catch (err) {
        status = 'Network error';
        bodyText = String(err);
        ok = false;
      }

      const panel = document.getElementById('response-panel');
      panel.style.display = 'block';

      let html = '';
      html += '<div class="panel-header">';
      html += '  <h2>Response</h2>';
      html += '  <span class="pill ' + (ok ? 'pill-ok' : 'pill-error') + '">' + status + '</span>';
      html += '</div>';
      html += '<div class="response-meta">';
      html += '  <span><strong>Request URL:</strong> ' + fullUrl + '</span>';
      html += '  <span>' + (new Date()).toLocaleTimeString() + '</span>';
      html += '</div>';
      html += '<pre>' + escapeHtml(prettyMaybeJson(bodyText)) + '</pre>';

      panel.innerHTML = html;
    }

    function prettyMaybeJson(text) {
      try {
        const obj = JSON.parse(text);
        return JSON.stringify(obj, null, 2);
      } catch {
        return text;
      }
    }

    function escapeHtml(text) {
      return text
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;');
    }

    // --- Code view with simple syntax highlighting ---

    function renderCodePanel() {
      const panel = document.getElementById('code-panel');
      let html = '<div class="panel-header">';
      html += '<h2>app.shr</h2>';
      html += '<span class="chip">Source</span>';
      html += '</div>';

      if (!sourceText) {
        html += '<p style="font-size:0.8rem;color:var(--text-soft);margin:0;">Could not load app.shr.</p>';
        panel.innerHTML = html;
        return;
      }

      html += '<div class="code-block">' + highlightShrimpl(sourceText) + '</div>';
      panel.innerHTML = html;
    }

    function highlightShrimpl(source) {
      const lines = source.split(/\r?\n/);
      const out = [];

      for (let line of lines) {
        let escaped = line
          .replace(/&/g, '&amp;')
          .replace(/</g, '&lt;')
          .replace(/>/g, '&gt;');

        // Split comments
        let commentHtml = '';
        const hashIdx = escaped.indexOf('#');
        if (hashIdx >= 0) {
          commentHtml = escaped.substring(hashIdx);
          escaped = escaped.substring(0, hashIdx);
          commentHtml = '<span class="tok-comment">' + commentHtml + '</span>';
        }

        // Strings
        escaped = escaped.replace(/"([^"\\]*(\\.[^"\\]*)*)"/g, function(m) {
          return '<span class="tok-string">' + m + '</span>';
        });

        // Keywords
        escaped = escaped.replace(/\b(server|func|class|endpoint)\b/g, '<span class="tok-kw">$1</span>');
        escaped = escaped.replace(/\b(GET|POST)\b/g, '<span class="tok-method">$1</span>');
        escaped = escaped.replace(/\b(json)\b/g, '<span class="tok-kw">$1</span>');

        // Numbers
        escaped = escaped.replace(/\b\d+(\.\d+)?\b/g, '<span class="tok-number">$&</span>');

        out.push(escaped + commentHtml);
      }

      return out.join('\n');
    }

    // --- Diagnostics view ---

    function renderDiagnosticsPanel() {
      const panel = document.getElementById('diag-panel');
      let html = '<div class="panel-header">';
      html += '<h2>Diagnostics</h2>';

      const warningCount = diagnostics.warnings ? diagnostics.warnings.length : 0;
      const errorCount = diagnostics.errors ? diagnostics.errors.length : 0;
      const summary = warningCount + ' warning(s), ' + errorCount + ' error(s)';
      html += '<span class="chip">' + summary + '</span>';
      html += '</div>';

      if (warningCount === 0 && errorCount === 0) {
        html += '<p style="font-size:0.8rem;color:var(--text-soft);margin:0;">No static diagnostics.</p>';
        panel.innerHTML = html;
        return;
      }

      html += '<ul class="diag-list">';

      if (diagnostics.errors) {
        diagnostics.errors.forEach(d => {
          html += renderDiagItem(d, 'error');
        });
      }
      if (diagnostics.warnings) {
        diagnostics.warnings.forEach(d => {
          html += renderDiagItem(d, 'warning');
        });
      }

      html += '</ul>';
      panel.innerHTML = html;
    }

    function renderDiagItem(d, fallbackKind) {
      const kind = (d.kind || fallbackKind || 'warning').toLowerCase();
      const kindClass = kind === 'error' ? 'diag-kind-error' : 'diag-kind-warning';
      const scope = d.scope || '';
      const name = d.name || '';
      const msg = d.message || JSON.stringify(d);

      let html = '<li class="diag-item">';
      html += '<span class="diag-kind ' + kindClass + '">' + kind.toUpperCase() + '</span>';
      if (scope || name) {
        html += '<span class="diag-scope">' + escapeHtml(scope) + (name ? ' · ' + escapeHtml(name) : '') + '</span>';
      }
      html += '<span class="diag-message">' + escapeHtml(msg) + '</span>';
      html += '</li>';
      return html;
    }

    loadStudio().catch(err => {
      console.error(err);
      document.getElementById('header-meta').textContent = 'Failed to load Shrimpl Studio';
    });
  </script>
</body>
</html>
"#;
