use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;
use tokio::sync::Mutex;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use shrimpl::docs;
use shrimpl::parser::ast::Program;
use shrimpl::parser::parse_program;

/// Backend state for the LSP server.
#[derive(Debug)]
struct Backend {
    client: Client,
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
        let (mut diagnostics, program_opt) = analyze_source(text);

        if let Some(program) = program_opt {
            let diags_json: Value = docs::build_diagnostics(&program);
            diagnostics.extend(convert_static_diagnostics(&diags_json));
        }

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
            let mut line: u32 = 0;
            if let Some(idx) = msg.find("Line ") {
                let rest = &msg[idx + 5..];
                if let Some(colon) = rest.find(':') {
                    let num_str = rest[..colon].trim();
                    if let Ok(num) = num_str.parse::<u32>() {
                        line = num.saturating_sub(1);
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
struct ModelOutline {
    name: String,
    line: u32,
    start_char: u32,
    end_char: u32,
}

#[derive(Debug, Clone)]
struct Outline {
    server: Option<ServerOutline>,
    endpoints: Vec<EndpointOutline>,
    functions: Vec<FunctionOutline>,
    classes: Vec<ClassOutline>,
    models: Vec<ModelOutline>,
}

/// Build a lightweight outline by scanning the Shrimpl source text.
fn parse_outline(text: &str) -> Outline {
    let mut outline = Outline {
        server: None,
        endpoints: Vec::new(),
        functions: Vec::new(),
        classes: Vec::new(),
        models: Vec::new(),
    };

    let lines: Vec<&str> = text.lines().collect();
    let mut i: usize = 0;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        let indent = (line.len() - trimmed.len()) as u32;
        let line_no = i as u32;
        let end_char = line.len() as u32;

        if let Some(rest) = trimmed.strip_prefix("server") {
            let parts: Vec<&str> = rest.split_whitespace().collect();
            let port = if !parts.is_empty() {
                parts[0].parse::<u16>().ok()
            } else {
                None
            };
            outline.server = Some(ServerOutline {
                port,
                line: line_no,
                start_char: indent,
                end_char,
            });
            i += 1;
            continue;
        }

        if let Some(stripped) = trimmed.strip_prefix("endpoint") {
            let rest = stripped.trim_start();
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

        if let Some(rest) = trimmed.strip_prefix("func ") {
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

        if let Some(rest) = trimmed.strip_prefix("class ") {
            let colon_pos = rest.find(':').unwrap_or(rest.len());
            let class_name = rest[..colon_pos].trim().to_string();

            let mut class = ClassOutline {
                name: class_name.clone(),
                line: line_no,
                start_char: indent,
                end_char,
                methods: Vec::new(),
            };

            i += 1;
            while i < lines.len() {
                let line2 = lines[i];
                let trimmed2 = line2.trim_start();

                if trimmed2.is_empty() || trimmed2.starts_with('#') {
                    i += 1;
                    continue;
                }

                let indent2 = (line2.len() - trimmed2.len()) as u32;
                if indent2 <= indent {
                    break;
                }

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

        if let Some(rest) = trimmed.strip_prefix("model ") {
            let colon_pos = rest.find(':').unwrap_or(rest.len());
            let model_name = rest[..colon_pos].trim().to_string();

            outline.models.push(ModelOutline {
                name: model_name,
                line: line_no,
                start_char: indent,
                end_char,
            });

            i += 1;
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

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == ':' || c == '/' || c == '"'
}

fn find_word_span(line: &str, idx: usize) -> (usize, usize) {
    if line.is_empty() {
        return (0, 0);
    }
    let mut start = idx.min(line.len());
    while start > 0 && is_word_char(line.as_bytes()[start - 1] as char) {
        start -= 1;
    }
    let mut end = idx.min(line.len());
    while end < line.len() && is_word_char(line.as_bytes()[end] as char) {
        end += 1;
    }
    (start, end)
}

/// Static completion items for Shrimpl keywords and basic patterns.
fn keyword_completions() -> Vec<CompletionItem> {
    vec![
        CompletionItem {
            label: "server".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some("Declare server port: server <port>".to_string()),
            insert_text: Some("server ${1:3000}".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..CompletionItem::default()
        },
        CompletionItem {
            label: "endpoint".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some("Declare endpoint: endpoint METHOD \"/path\": expr".to_string()),
            insert_text: Some("endpoint ${1:GET} \"/${2:path}\": ${3:body}".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..CompletionItem::default()
        },
        CompletionItem {
            label: "func".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some("Define function: func name(args): expr".to_string()),
            insert_text: Some("func ${1:name}(${2:args}): ${3:expr}".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..CompletionItem::default()
        },
        CompletionItem {
            label: "class".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some("Define class: class Name: (methods indented below)".to_string()),
            insert_text: Some("class ${1:Name}:\n  ${2:method}(${3:args}): ${4:expr}".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..CompletionItem::default()
        },
        CompletionItem {
            label: "model".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some("Define ORM model: model Name: fields".to_string()),
            insert_text: Some(
                "model ${1:User}:\n  id: int pk\n  name: string\n  created_at?: string".to_string(),
            ),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..CompletionItem::default()
        },
        CompletionItem {
            label: "GET".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some("HTTP GET method".to_string()),
            insert_text: Some("GET".to_string()),
            insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
            ..CompletionItem::default()
        },
        CompletionItem {
            label: "POST".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some("HTTP POST method".to_string()),
            insert_text: Some("POST".to_string()),
            insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
            ..CompletionItem::default()
        },
        CompletionItem {
            label: "json".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some("JSON body wrapper: json { ... }".to_string()),
            insert_text: Some("json { \"message\": \"Hello\" }".to_string()),
            insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
            ..CompletionItem::default()
        },
    ]
}

/// Helper to construct DocumentSymbol while suppressing the `deprecated` field warning.
fn make_document_symbol(
    name: String,
    detail: Option<String>,
    kind: SymbolKind,
    range: Range,
    children: Option<Vec<DocumentSymbol>>,
) -> DocumentSymbol {
    #[allow(deprecated)]
    DocumentSymbol {
        name,
        detail,
        kind,
        tags: None,
        deprecated: None,
        range,
        selection_range: range,
        children,
    }
}

/// Compute rename edits for the identifier under `position` in `text`.
fn compute_rename_edits(
    uri: &Url,
    text: &str,
    position: Position,
    new_name: &str,
) -> Option<WorkspaceEdit> {
    let lines: Vec<&str> = text.lines().collect();
    let line_idx = position.line as usize;
    if line_idx >= lines.len() {
        return None;
    }
    let line = lines[line_idx];
    let idx = (position.character as usize).min(line.len());
    let (start, end) = find_word_span(line, idx);
    if start >= end || end > line.len() {
        return None;
    }

    let old_name = line[start..end].trim_matches('"').to_string();
    if old_name.is_empty() {
        return None;
    }

    let mut edits: Vec<TextEdit> = Vec::new();

    for (ln, l) in lines.iter().enumerate() {
        let mut search = 0usize;
        while let Some(rel) = l[search..].find(&old_name) {
            let abs = search + rel;
            let after = abs + old_name.len();

            let before_ch = if abs == 0 {
                None
            } else {
                Some(l.as_bytes()[abs - 1] as char)
            };
            let after_ch = if after < l.len() {
                Some(l.as_bytes()[after] as char)
            } else {
                None
            };

            if before_ch.map_or(true, |c| !is_word_char(c))
                && after_ch.map_or(true, |c| !is_word_char(c))
            {
                let range = Range {
                    start: Position {
                        line: ln as u32,
                        character: abs as u32,
                    },
                    end: Position {
                        line: ln as u32,
                        character: after as u32,
                    },
                };
                edits.push(TextEdit {
                    range,
                    new_text: new_name.to_string(),
                });
            }

            search = after;
            if search >= l.len() {
                break;
            }
        }
    }

    if edits.is_empty() {
        None
    } else {
        let mut changes = HashMap::new();
        changes.insert(uri.clone(), edits);
        Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        })
    }
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
            rename_provider: Some(OneOf::Left(true)),
            ..ServerCapabilities::default()
        };

        Ok(InitializeResult {
            capabilities,
            server_info: Some(ServerInfo {
                name: "Shrimpl Language Server".to_string(),
                version: Some("0.5.5".to_string()),
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

        let _ = self.client.publish_diagnostics(uri, Vec::new(), None).await;
    }

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

        let mut func_names = HashMap::<String, FunctionOutline>::new();
        for f in &outline.functions {
            func_names.insert(f.name.clone(), f.clone());
        }

        let mut class_names = HashMap::<String, ClassOutline>::new();
        for c in &outline.classes {
            class_names.insert(c.name.clone(), c.clone());
        }

        let mut model_names = HashMap::<String, ModelOutline>::new();
        for m in &outline.models {
            model_names.insert(m.name.clone(), m.clone());
        }

        let mut method_names = HashMap::<String, Vec<MethodOutline>>::new();
        for c in &outline.classes {
            for m in &c.methods {
                method_names
                    .entry(m.name.clone())
                    .or_default()
                    .push(m.clone());
            }
        }

        let mut endpoint_paths = HashMap::<String, Vec<EndpointOutline>>::new();
        for ep in &outline.endpoints {
            endpoint_paths
                .entry(ep.path.clone())
                .or_default()
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
        } else if word == "model" {
            Some(
                "Shrimpl ORM model definition.\n\nSyntax:\n```shrimpl\nmodel User:\n  id: int pk\n  name: string\n  age?: int\n```"
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
        } else if let Some(m) = model_names.get(&word) {
            Some(format!(
                "Model `{}`.\n\nDeclared on line {}.\n\nUsed by the ORM layer to create tables and map records to rows.",
                m.name,
                m.line + 1
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

    async fn completion(&self, _params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let items = keyword_completions();
        Ok(Some(CompletionResponse::Array(items)))
    }

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

        if let Some(srv) = outline.server {
            let name = "server".to_string();
            let detail = srv
                .port
                .map(|p| format!("port {}", p))
                .or_else(|| Some("server".to_string()));
            let range = make_range(srv.line, srv.start_char, srv.end_char);
            symbols.push(make_document_symbol(
                name,
                detail,
                SymbolKind::NAMESPACE,
                range,
                None,
            ));
        }

        for ep in outline.endpoints {
            let name = format!("{} {}", ep.method, ep.path);
            let detail = Some("endpoint".to_string());
            let range = make_range(ep.line, ep.start_char, ep.end_char);
            symbols.push(make_document_symbol(
                name,
                detail,
                SymbolKind::FUNCTION,
                range,
                None,
            ));
        }

        for f in outline.functions {
            let range = make_range(f.line, f.start_char, f.end_char);
            symbols.push(make_document_symbol(
                f.name,
                Some("func".to_string()),
                SymbolKind::FUNCTION,
                range,
                None,
            ));
        }

        for c in outline.classes {
            let class_range = make_range(c.line, c.start_char, c.end_char);
            let mut method_symbols = Vec::new();
            for m in c.methods {
                let range = make_range(m.line, m.start_char, m.end_char);
                method_symbols.push(make_document_symbol(
                    m.name,
                    Some(format!("method of {}", m.class_name)),
                    SymbolKind::METHOD,
                    range,
                    None,
                ));
            }

            let children = if method_symbols.is_empty() {
                None
            } else {
                Some(method_symbols)
            };

            symbols.push(make_document_symbol(
                c.name,
                Some("class".to_string()),
                SymbolKind::CLASS,
                class_range,
                children,
            ));
        }

        for m in outline.models {
            let range = make_range(m.line, m.start_char, m.end_char);
            symbols.push(make_document_symbol(
                m.name,
                Some("model".to_string()),
                SymbolKind::STRUCT,
                range,
                None,
            ));
        }

        Ok(Some(DocumentSymbolResponse::Nested(symbols)))
    }

    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        let uri = params.text_document.uri;
        let position = params.position;

        let text_opt = {
            let docs = self.documents.lock().await;
            docs.get(&uri).cloned()
        };
        let text = match text_opt {
            Some(t) => t,
            None => return Ok(None),
        };

        let lines: Vec<&str> = text.lines().collect();
        let line_idx = position.line as usize;
        if line_idx >= lines.len() {
            return Ok(None);
        }
        let line = lines[line_idx];
        let idx = (position.character as usize).min(line.len());
        let (start, end) = find_word_span(line, idx);
        if start >= end {
            return Ok(None);
        }

        let range = Range {
            start: Position {
                line: position.line,
                character: start as u32,
            },
            end: Position {
                line: position.line,
                character: end as u32,
            },
        };

        Ok(Some(PrepareRenameResponse::Range(range)))
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let new_name = params.new_name;

        let text_opt = {
            let docs = self.documents.lock().await;
            docs.get(&uri).cloned()
        };
        let text = match text_opt {
            Some(t) => t,
            None => return Ok(None),
        };

        let edit = compute_rename_edits(&uri, &text, position, &new_name);
        Ok(edit)
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
