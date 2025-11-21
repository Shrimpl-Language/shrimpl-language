use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;
use tokio::sync::Mutex;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

// Use the library modules defined in src/lib.rs
// We only need the parser's AST Program type, the top-level docs module,
// and the parse_program entry point.
use shrimpl::docs;
use shrimpl::parser::ast::Program;
use shrimpl::parser::parse_program;

/// Backend state for the LSP server.
#[derive(Debug)]
struct Backend {
    client: Client,
    // Current text of open documents
    documents: Arc<Mutex<HashMap<Url, String>>>,
}

impl Backend {
    fn new(client: Client) -> Self {
        Self {
            client,
            documents: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn update_document(&self, uri: Url, text: String) {
        {
            let mut docs = self.documents.lock().await;
            docs.insert(uri.clone(), text.clone());
        }
        self.reanalyze(uri, text).await;
    }

    async fn reanalyze(&self, uri: Url, text: String) {
        // 1) Parse
        let (mut diagnostics, program_opt) = analyze_source(text);

        // 2) If parse succeeded, run static diagnostics from docs::build_diagnostics
        if let Some(program) = program_opt {
            let diags_json: Value = docs::build_diagnostics(&program);
            diagnostics.extend(convert_static_diagnostics(&diags_json));
        }

        // 3) Publish diagnostics to the client
        // tower-lsp 0.20 API: publish_diagnostics(uri, diagnostics, version)
        let _ = self
            .client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }
}

/// Analyze Shrimpl source into (LSP diagnostics, optional Program).
fn analyze_source(source: String) -> (Vec<Diagnostic>, Option<Program>) {
    match parse_program(&source) {
        Ok(program) => (Vec::new(), Some(program)),
        Err(msg) => {
            // Try to extract "Line N:" from the error message.
            let mut line: u32 = 0;
            if let Some(idx) = msg.find("Line ") {
                let rest = &msg[idx + 5..];
                if let Some(colon) = rest.find(':') {
                    let num_str = rest[..colon].trim();
                    if let Ok(num) = num_str.parse::<u32>() {
                        line = num.saturating_sub(1); // LSP is 0-based
                    }
                }
            }

            let diagnostic = Diagnostic {
                range: Range {
                    start: Position { line, character: 0 },
                    end: Position {
                        line,
                        character: 200,
                    },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: None,
                code_description: None,
                source: Some("shrimpl-parser".to_string()),
                message: msg,
                related_information: None,
                tags: None,
                data: None,
            };

            (vec![diagnostic], None)
        }
    }
}

/// Convert JSON diagnostics from docs::build_diagnostics into LSP diagnostics.
fn convert_static_diagnostics(json: &Value) -> Vec<Diagnostic> {
    let mut out = Vec::new();

    let errors = json
        .get("errors")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let warnings = json
        .get("warnings")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    for item in errors.into_iter().chain(warnings.into_iter()) {
        let message = item
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("Shrimpl diagnostic")
            .to_string();

        let severity = if item
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or("warning")
            .eq_ignore_ascii_case("error")
        {
            DiagnosticSeverity::ERROR
        } else {
            DiagnosticSeverity::WARNING
        };

        // For now we don’t have exact positions, so put them at the top.
        let diagnostic = Diagnostic {
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 1,
                },
            },
            severity: Some(severity),
            code: None,
            code_description: None,
            source: Some("shrimpl-diag".to_string()),
            message,
            related_information: None,
            tags: None,
            data: None,
        };

        out.push(diagnostic);
    }

    out
}

/// Simple outline structures used for hover and documentSymbol.

#[derive(Debug, Clone)]
struct ServerOutline {
    port: Option<u16>,
    line: u32,
    start_char: u32,
    end_char: u32,
}

#[derive(Debug, Clone)]
struct EndpointOutline {
    method: String,
    path: String,
    line: u32,
    start_char: u32,
    end_char: u32,
}

#[derive(Debug, Clone)]
struct FunctionOutline {
    name: String,
    line: u32,
    start_char: u32,
    end_char: u32,
}

#[derive(Debug, Clone)]
struct MethodOutline {
    class_name: String,
    name: String,
    line: u32,
    start_char: u32,
    end_char: u32,
}

#[derive(Debug, Clone)]
struct ClassOutline {
    name: String,
    line: u32,
    start_char: u32,
    end_char: u32,
    methods: Vec<MethodOutline>,
}

#[derive(Debug, Clone)]
struct Outline {
    server: Option<ServerOutline>,
    endpoints: Vec<EndpointOutline>,
    functions: Vec<FunctionOutline>,
    classes: Vec<ClassOutline>,
}

/// Build a lightweight outline by scanning the Shrimpl source text.
///
/// This does not use the full parser; it just looks for:
/// - `server <port>`
/// - `endpoint METHOD "/path"`
/// - `func name(...)`
/// - `class Name:` + indented method lines.
fn parse_outline(text: &str) -> Outline {
    let mut outline = Outline {
        server: None,
        endpoints: Vec::new(),
        functions: Vec::new(),
        classes: Vec::new(),
    };

    let lines: Vec<&str> = text.lines().collect();
    let mut i: usize = 0;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        let indent = (line.len() - trimmed.len()) as u32;
        let line_no = i as u32;
        let end_char = line.len() as u32;

        if trimmed.starts_with("server") {
            // server <port>
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            let port = if parts.len() >= 2 {
                parts[1].parse::<u16>().ok()
            } else {
                None
            };
            outline.server = Some(ServerOutline {
                port,
                line: line_no,
                start_char: indent as u32,
                end_char,
            });
            i += 1;
            continue;
        }

        if trimmed.starts_with("endpoint") {
            // endpoint METHOD "/path"
            let rest = &trimmed["endpoint".len()..].trim_start();
            let mut parts = rest.splitn(2, ' ');
            let method = parts.next().unwrap_or("").to_string();
            let rest_after_method = parts.next().unwrap_or("");
            let path = extract_quoted_simple(rest_after_method).unwrap_or_else(|| "/".to_string());

            outline.endpoints.push(EndpointOutline {
                method,
                path,
                line: line_no,
                start_char: indent,
                end_char,
            });

            i += 1;
            continue;
        }

        if trimmed.starts_with("func ") {
            // func name(a, b):
            let rest = &trimmed["func ".len()..];
            let name_end = rest.find('(').unwrap_or(rest.len());
            let name = rest[..name_end].trim().to_string();

            outline.functions.push(FunctionOutline {
                name,
                line: line_no,
                start_char: indent,
                end_char,
            });

            i += 1;
            continue;
        }

        if trimmed.starts_with("class ") {
            // class Name:
            let rest = &trimmed["class ".len()..];
            let colon_pos = rest.find(':').unwrap_or(rest.len());
            let class_name = rest[..colon_pos].trim().to_string();

            let mut class = ClassOutline {
                name: class_name.clone(),
                line: line_no,
                start_char: indent,
                end_char,
                methods: Vec::new(),
            };

            // Scan indented methods following the class
            i += 1;
            while i < lines.len() {
                let line2 = lines[i];
                let trimmed2 = line2.trim_start();

                // Skip blank/comment lines inside class, but keep them within body
                if trimmed2.is_empty() || trimmed2.starts_with('#') {
                    i += 1;
                    continue;
                }

                let indent2 = (line2.len() - trimmed2.len()) as u32;
                if indent2 <= indent {
                    // Out of class body
                    break;
                }

                // methodName(a, b): expr
                if let Some(paren_idx) = trimmed2.find('(') {
                    let method_name = trimmed2[..paren_idx].trim().to_string();
                    let line2_no = i as u32;
                    let end2_char = line2.len() as u32;
                    class.methods.push(MethodOutline {
                        class_name: class_name.clone(),
                        name: method_name,
                        line: line2_no,
                        start_char: indent2,
                        end_char: end2_char,
                    });
                }

                i += 1;
            }

            outline.classes.push(class);
            continue;
        }

        i += 1;
    }

    outline
}

fn extract_quoted_simple(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut start_idx: Option<usize> = None;
    for (i, b) in bytes.iter().enumerate() {
        if *b == b'"' {
            start_idx = Some(i + 1);
            break;
        }
    }
    let start = start_idx?;
    for j in start..bytes.len() {
        if bytes[j] == b'"' {
            return Some(s[start..j].to_string());
        }
    }
    None
}

/// Helper for building a Range from line + character positions.
fn make_range(line: u32, start_char: u32, end_char: u32) -> Range {
    Range {
        start: Position {
            line,
            character: start_char,
        },
        end: Position {
            line,
            character: end_char,
        },
    }
}

/// Identify a "word" around a character index in a line, for hover.
///
/// We treat ASCII letters, digits, '_', ':', '/', and '"' as part of a word
/// so that endpoints paths and methods are easily picked up.
fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == ':' || c == '/' || c == '"'
}

fn find_word_span(line: &str, idx: usize) -> (usize, usize) {
    if line.is_empty() {
        return (0, 0);
    }
    let mut start = idx.min(line.len());
    if start > 0 {
        start -= 1;
    }
    while start > 0 {
        let c = line.as_bytes()[start] as char;
        if !is_word_char(c) {
            start += 1;
            break;
        }
        if start == 0 {
            break;
        }
        start -= 1;
    }
    let mut end = idx.min(line.len());
    while end < line.len() {
        let c = line.as_bytes()[end] as char;
        if !is_word_char(c) {
            break;
        }
        end += 1;
    }
    (start, end)
}

/// Static completion items for Shrimpl keywords and basic patterns.
fn keyword_completions() -> Vec<CompletionItem> {
    let mut items = Vec::new();

    // Keywords
    items.push(CompletionItem {
        label: "server".to_string(),
        kind: Some(CompletionItemKind::KEYWORD),
        detail: Some("Declare server port: server <port>".to_string()),
        insert_text: Some("server ${1:3000}".to_string()),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        ..CompletionItem::default()
    });

    items.push(CompletionItem {
        label: "endpoint".to_string(),
        kind: Some(CompletionItemKind::KEYWORD),
        detail: Some("Declare endpoint: endpoint METHOD \"/path\": expr".to_string()),
        insert_text: Some("endpoint ${1:GET} \"/${2:path}\": ${3:body}".to_string()),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        ..CompletionItem::default()
    });

    items.push(CompletionItem {
        label: "func".to_string(),
        kind: Some(CompletionItemKind::KEYWORD),
        detail: Some("Define function: func name(args): expr".to_string()),
        insert_text: Some("func ${1:name}(${2:args}): ${3:expr}".to_string()),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        ..CompletionItem::default()
    });

    items.push(CompletionItem {
        label: "class".to_string(),
        kind: Some(CompletionItemKind::KEYWORD),
        detail: Some("Define class: class Name: (methods indented below)".to_string()),
        insert_text: Some("class ${1:Name}:\n  ${2:method}(${3:args}): ${4:expr}".to_string()),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        ..CompletionItem::default()
    });

    // HTTP methods
    items.push(CompletionItem {
        label: "GET".to_string(),
        kind: Some(CompletionItemKind::KEYWORD),
        detail: Some("HTTP GET method".to_string()),
        insert_text: Some("GET".to_string()),
        insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
        ..CompletionItem::default()
    });

    items.push(CompletionItem {
        label: "POST".to_string(),
        kind: Some(CompletionItemKind::KEYWORD),
        detail: Some("HTTP POST method".to_string()),
        insert_text: Some("POST".to_string()),
        insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
        ..CompletionItem::default()
    });

    // JSON helper
    items.push(CompletionItem {
        label: "json".to_string(),
        kind: Some(CompletionItemKind::KEYWORD),
        detail: Some("JSON body wrapper: json { ... }".to_string()),
        insert_text: Some("json { \"message\": \"Hello\" }".to_string()),
        insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
        ..CompletionItem::default()
    });

    items
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        let capabilities = ServerCapabilities {
            text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
            hover_provider: Some(HoverProviderCapability::Simple(true)),
            completion_provider: Some(CompletionOptions {
                resolve_provider: Some(false),
                trigger_characters: Some(vec![
                    " ".to_string(),
                    "/".to_string(),
                    "\"".to_string(),
                    ":".to_string(),
                ]),
                ..CompletionOptions::default()
            }),
            document_symbol_provider: Some(OneOf::Left(true)),
            ..ServerCapabilities::default()
        };

        Ok(InitializeResult {
            capabilities,
            server_info: Some(ServerInfo {
                name: "Shrimpl Language Server".to_string(),
                version: Some("0.5.0".to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        let _ = self
            .client
            .log_message(
                MessageType::INFO,
                "Shrimpl LSP initialized. Watching .shr files.",
            )
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;
        self.update_document(uri, text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;

        // We use FULL sync, so last change contains full text.
        if let Some(change) = params.content_changes.into_iter().last() {
            self.update_document(uri, change.text).await;
        }
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        if let Some(text) = params.text {
            self.update_document(params.text_document.uri, text).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri.clone();

        {
            let mut docs = self.documents.lock().await;
            docs.remove(&uri);
        }

        // Clear diagnostics for this file
        let _ = self.client.publish_diagnostics(uri, Vec::new(), None).await;
    }

    // --- Hover: basic info for keywords and declarations ---

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let HoverParams {
            text_document_position_params,
            ..
        } = params;
        let TextDocumentPositionParams {
            text_document,
            position,
        } = text_document_position_params;
        let uri = text_document.uri;

        let text_opt = {
            let docs = self.documents.lock().await;
            docs.get(&uri).cloned()
        };
        let text = match text_opt {
            Some(t) => t,
            None => return Ok(None),
        };

        let lines: Vec<&str> = text.lines().collect();
        let line_index = position.line as usize;
        if line_index >= lines.len() {
            return Ok(None);
        }
        let line_str = lines[line_index];
        let char_idx = position.character as usize;
        let idx = char_idx.min(line_str.len());
        let (start, end) = find_word_span(line_str, idx);
        if start >= end || end > line_str.len() {
            return Ok(None);
        }
        let mut word = line_str[start..end].to_string();
        word = word.trim_matches('"').to_string();

        if word.is_empty() {
            return Ok(None);
        }

        let outline = parse_outline(&text);

        // Maps for quick lookup
        let mut func_names = HashMap::<String, FunctionOutline>::new();
        for f in &outline.functions {
            func_names.insert(f.name.clone(), f.clone());
        }

        let mut class_names = HashMap::<String, ClassOutline>::new();
        for c in &outline.classes {
            class_names.insert(c.name.clone(), c.clone());
        }

        let mut method_names = HashMap::<String, Vec<MethodOutline>>::new();
        for c in &outline.classes {
            for m in &c.methods {
                method_names
                    .entry(m.name.clone())
                    .or_insert_with(Vec::new)
                    .push(m.clone());
            }
        }

        let mut endpoint_paths = HashMap::<String, Vec<EndpointOutline>>::new();
        for ep in &outline.endpoints {
            endpoint_paths
                .entry(ep.path.clone())
                .or_insert_with(Vec::new)
                .push(ep.clone());
        }

        let contents: Option<String> = if word == "server" {
            if let Some(srv) = outline.server {
                let port_str = srv
                    .port
                    .map(|p| p.to_string())
                    .unwrap_or_else(|| "?".to_string());
                Some(format!(
                    "Shrimpl server declaration.\n\nPort: `{}`",
                    port_str
                ))
            } else {
                Some("Shrimpl server declaration.\n\nSyntax: `server <port>`".to_string())
            }
        } else if word == "endpoint" {
            Some(
                "Shrimpl endpoint declaration.\n\nSyntax: `endpoint METHOD \"/path\": expr`\n\nMethods currently supported: `GET`, `POST`."
                    .to_string(),
            )
        } else if word == "func" {
            Some(
                "Shrimpl function definition.\n\nSyntax: `func name(arg1, arg2): expr`.\nThe body is a single expression returning a string or number."
                    .to_string(),
            )
        } else if word == "class" {
            Some(
                "Shrimpl class definition.\n\nSyntax: `class Name:` followed by indented methods:\n`  methodName(args): expr`."
                    .to_string(),
            )
        } else if word == "GET" || word == "POST" {
            Some(format!(
                "HTTP `{}` endpoint method.\n\nUsed in `endpoint` declarations, for example:\n```shrimpl\nendpoint {} \"/hello\": \"Hello!\"\n```",
                word, word
            ))
        } else if let Some(f) = func_names.get(&word) {
            Some(format!(
                "Function `{}`.\n\nDeclared on line {}.\n\nSyntax: `func {}(...): expr`.",
                f.name,
                f.line + 1,
                f.name
            ))
        } else if let Some(c) = class_names.get(&word) {
            let method_list = if c.methods.is_empty() {
                "_no methods defined_".to_string()
            } else {
                let mut names: Vec<String> = c.methods.iter().map(|m| m.name.clone()).collect();
                names.sort();
                format!("Methods:\n- `{}`", names.join("`\n- `"))
            };
            Some(format!(
                "Class `{}`.\n\nDeclared on line {}.\n\n{}",
                c.name,
                c.line + 1,
                method_list
            ))
        } else if let Some(methods) = method_names.get(&word) {
            let mut entries = Vec::new();
            for m in methods {
                entries.push(format!(
                    "`{}.{}` (line {})",
                    m.class_name,
                    m.name,
                    m.line + 1
                ));
            }
            Some(format!(
                "Method `{}`.\n\nDefined in:\n{}",
                word,
                entries.join("\n")
            ))
        } else if let Some(eps) = endpoint_paths.get(&word) {
            let mut entries = Vec::new();
            for ep in eps {
                entries.push(format!(
                    "- `{}` `{}` (line {})",
                    ep.method,
                    ep.path,
                    ep.line + 1
                ));
            }
            Some(format!(
                "Endpoint path `{}`.\n\nDeclared as:\n{}",
                word,
                entries.join("\n")
            ))
        } else {
            None
        };

        if let Some(value) = contents {
            let hover = Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value,
                }),
                range: None,
            };
            Ok(Some(hover))
        } else {
            Ok(None)
        }
    }

    // --- Completion: keyword stubs ---

    async fn completion(&self, _params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let items = keyword_completions();
        Ok(Some(CompletionResponse::Array(items)))
    }

    // --- Document symbols: outline of server, endpoints, functions, classes, methods ---

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri;

        let text_opt = {
            let docs = self.documents.lock().await;
            docs.get(&uri).cloned()
        };
        let text = match text_opt {
            Some(t) => t,
            None => return Ok(None),
        };

        let outline = parse_outline(&text);
        let mut symbols: Vec<DocumentSymbol> = Vec::new();

        // Server (as a namespace-like symbol)
        if let Some(srv) = outline.server {
            let name = "server".to_string();
            let detail = srv
                .port
                .map(|p| format!("port {}", p))
                .or_else(|| Some("server".to_string()));
            let range = make_range(srv.line, srv.start_char, srv.end_char);
            let symbol = DocumentSymbol {
                name,
                detail,
                kind: SymbolKind::NAMESPACE,
                tags: None,
                deprecated: None,
                range,
                selection_range: range,
                children: None,
            };
            symbols.push(symbol);
        }

        // Endpoints (as functions/methods)
        for ep in outline.endpoints {
            let name = format!("{} {}", ep.method, ep.path);
            let detail = Some("endpoint".to_string());
            let range = make_range(ep.line, ep.start_char, ep.end_char);
            let symbol = DocumentSymbol {
                name,
                detail,
                kind: SymbolKind::FUNCTION,
                tags: None,
                deprecated: None,
                range,
                selection_range: range,
                children: None,
            };
            symbols.push(symbol);
        }

        // Functions
        for f in outline.functions {
            let range = make_range(f.line, f.start_char, f.end_char);
            let symbol = DocumentSymbol {
                name: f.name,
                detail: Some("func".to_string()),
                kind: SymbolKind::FUNCTION,
                tags: None,
                deprecated: None,
                range,
                selection_range: range,
                children: None,
            };
            symbols.push(symbol);
        }

        // Classes + methods (nested)
        for c in outline.classes {
            let class_range = make_range(c.line, c.start_char, c.end_char);
            let mut method_symbols = Vec::new();
            for m in c.methods {
                let range = make_range(m.line, m.start_char, m.end_char);
                let sym = DocumentSymbol {
                    name: m.name,
                    detail: Some(format!("method of {}", m.class_name)),
                    kind: SymbolKind::METHOD,
                    tags: None,
                    deprecated: None,
                    range,
                    selection_range: range,
                    children: None,
                };
                method_symbols.push(sym);
            }

            let class_symbol = DocumentSymbol {
                name: c.name,
                detail: Some("class".to_string()),
                kind: SymbolKind::CLASS,
                tags: None,
                deprecated: None,
                range: class_range,
                selection_range: class_range,
                children: if method_symbols.is_empty() {
                    None
                } else {
                    Some(method_symbols)
                },
            };
            symbols.push(class_symbol);
        }

        Ok(Some(DocumentSymbolResponse::Nested(symbols)))
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend::new(client));
    Server::new(stdin, stdout, socket).serve(service).await;
}
