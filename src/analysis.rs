// src/analysis.rs
//
// Shared semantic analysis for CLI diagnostics, API Studio, and LSP.
// This module deliberately stays parser-agnostic: it consumes the parsed AST
// plus source text, then attaches best-effort source ranges for editor use.

use crate::parser::ast::{Body, Expr, Method, Program};
use crate::typecheck;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
}

impl DiagnosticSeverity {
    pub fn as_str(self) -> &'static str {
        match self {
            DiagnosticSeverity::Error => "error",
            DiagnosticSeverity::Warning => "warning",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceSpan {
    pub line: u32,
    pub character: u32,
    pub end_line: u32,
    pub end_character: u32,
}

impl SourceSpan {
    fn new(line: u32, character: u32, end_line: u32, end_character: u32) -> Self {
        Self {
            line,
            character,
            end_line,
            end_character,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AnalysisDiagnostic {
    pub severity: DiagnosticSeverity,
    pub source: String,
    pub code: String,
    pub scope: String,
    pub name: String,
    pub message: String,
    pub span: Option<SourceSpan>,
}

impl AnalysisDiagnostic {
    fn error(
        code: &str,
        scope: &str,
        name: &str,
        message: String,
        span: Option<SourceSpan>,
    ) -> Self {
        Self {
            severity: DiagnosticSeverity::Error,
            source: "shrimpl-analysis".to_string(),
            code: code.to_string(),
            scope: scope.to_string(),
            name: name.to_string(),
            message,
            span,
        }
    }

    fn warning(
        code: &str,
        scope: &str,
        name: &str,
        message: String,
        span: Option<SourceSpan>,
    ) -> Self {
        Self {
            severity: DiagnosticSeverity::Warning,
            source: "shrimpl-analysis".to_string(),
            code: code.to_string(),
            scope: scope.to_string(),
            name: name.to_string(),
            message,
            span,
        }
    }

    pub fn to_json(&self) -> Value {
        let mut obj = json!({
            "kind": self.severity.as_str(),
            "source": self.source,
            "code": self.code,
            "scope": self.scope,
            "name": self.name,
            "message": self.message,
        });

        if let Some(span) = self.span {
            if let Some(map) = obj.as_object_mut() {
                map.insert("line".to_string(), json!(span.line + 1));
                map.insert("column".to_string(), json!(span.character + 1));
                map.insert(
                    "range".to_string(),
                    json!({
                        "start": {
                            "line": span.line,
                            "character": span.character
                        },
                        "end": {
                            "line": span.end_line,
                            "character": span.end_character
                        }
                    }),
                );
            }
        }

        obj
    }
}

#[derive(Debug, Clone)]
struct EndpointOccurrence {
    method: String,
    path: String,
    span: SourceSpan,
}

#[derive(Debug)]
struct SourceIndex {
    lines: Vec<String>,
    server: Option<SourceSpan>,
    functions: HashMap<String, SourceSpan>,
    classes: HashMap<String, SourceSpan>,
    methods: HashMap<(String, String), SourceSpan>,
    models: HashMap<String, SourceSpan>,
    endpoints: Vec<EndpointOccurrence>,
}

impl SourceIndex {
    fn new(source: &str) -> Self {
        let lines: Vec<String> = source.lines().map(|line| line.to_string()).collect();
        let mut index = Self {
            lines,
            server: None,
            functions: HashMap::new(),
            classes: HashMap::new(),
            methods: HashMap::new(),
            models: HashMap::new(),
            endpoints: Vec::new(),
        };
        index.scan();
        index
    }

    fn scan(&mut self) {
        let mut i = 0usize;

        while i < self.lines.len() {
            let line = &self.lines[i];
            let trimmed = line.trim_start();

            if trimmed.is_empty() || trimmed.starts_with('#') {
                i += 1;
                continue;
            }

            let indent = line.len() - trimmed.len();
            let line_no = i as u32;

            if trimmed.starts_with("server") {
                self.server = Some(self.line_span(line_no, indent, line.len()));
                i += 1;
                continue;
            }

            if let Some(stripped) = trimmed.strip_prefix("endpoint") {
                let rest = stripped.trim_start();
                let mut parts = rest.splitn(2, ' ');
                let method = parts.next().unwrap_or("").to_string();
                let path = parts
                    .next()
                    .and_then(extract_quoted_simple)
                    .unwrap_or_else(|| "/".to_string());

                self.endpoints.push(EndpointOccurrence {
                    method,
                    path,
                    span: self.line_span(line_no, indent, line.len()),
                });
                i += 1;
                continue;
            }

            if let Some(rest) = trimmed.strip_prefix("func ") {
                let name_end = rest.find('(').unwrap_or(rest.len());
                let name = rest[..name_end].trim();
                if !name.is_empty() {
                    self.functions.insert(
                        name.to_string(),
                        self.line_span(line_no, indent, line.len()),
                    );
                }
                i += 1;
                continue;
            }

            if let Some(rest) = trimmed.strip_prefix("class ") {
                let colon_pos = rest.find(':').unwrap_or(rest.len());
                let class_name = rest[..colon_pos].trim().to_string();
                if !class_name.is_empty() {
                    self.classes.insert(
                        class_name.clone(),
                        self.line_span(line_no, indent, line.len()),
                    );
                }

                i += 1;
                while i < self.lines.len() {
                    let method_line = &self.lines[i];
                    let method_trimmed = method_line.trim_start();
                    if method_trimmed.is_empty() || method_trimmed.starts_with('#') {
                        i += 1;
                        continue;
                    }

                    let method_indent = method_line.len() - method_trimmed.len();
                    if method_indent <= indent {
                        break;
                    }

                    if let Some(paren) = method_trimmed.find('(') {
                        let method_name = method_trimmed[..paren].trim();
                        if !method_name.is_empty() {
                            self.methods.insert(
                                (class_name.clone(), method_name.to_string()),
                                self.line_span(i as u32, method_indent, method_line.len()),
                            );
                        }
                    }

                    i += 1;
                }
                continue;
            }

            if let Some(rest) = trimmed.strip_prefix("model ") {
                let colon_pos = rest.find(':').unwrap_or(rest.len());
                let name = rest[..colon_pos].trim();
                if !name.is_empty() {
                    self.models.insert(
                        name.to_string(),
                        self.line_span(line_no, indent, line.len()),
                    );
                }
                i += 1;
                continue;
            }

            i += 1;
        }
    }

    fn line(&self, line: u32) -> Option<&str> {
        self.lines.get(line as usize).map(String::as_str)
    }

    fn line_span(&self, line: u32, start_byte: usize, end_byte: usize) -> SourceSpan {
        let text = self.line(line).unwrap_or_default();
        SourceSpan::new(
            line,
            byte_to_utf16_col(text, start_byte),
            line,
            byte_to_utf16_col(text, end_byte),
        )
    }

    fn function_span(&self, name: &str) -> Option<SourceSpan> {
        self.functions.get(name).copied()
    }

    fn method_span(&self, class_name: &str, method_name: &str) -> Option<SourceSpan> {
        self.methods
            .get(&(class_name.to_string(), method_name.to_string()))
            .copied()
    }

    fn endpoint_occurrence(
        &self,
        method: &str,
        path: &str,
        occurrence_index: usize,
    ) -> Option<&EndpointOccurrence> {
        self.endpoints
            .iter()
            .filter(|ep| ep.method == method && ep.path == path)
            .nth(occurrence_index)
    }

    fn endpoint_param_span(
        &self,
        method: &str,
        path: &str,
        occurrence_index: usize,
        param: &str,
    ) -> Option<SourceSpan> {
        let occurrence = self.endpoint_occurrence(method, path, occurrence_index)?;
        let line = self.line(occurrence.span.line)?;
        let needle = format!(":{param}");
        let byte = line.find(&needle)?;
        Some(self.line_span(occurrence.span.line, byte, byte + needle.len()))
    }

    fn param_span_in_decl(&self, decl_span: SourceSpan, param: &str) -> Option<SourceSpan> {
        let line = self.line(decl_span.line)?;
        let open = line.find('(')?;
        let close = line[open..].find(')').map(|idx| open + idx)?;
        let params = &line[open + 1..close];
        let mut offset = open + 1;

        for raw in params.split(',') {
            let trimmed = raw.trim();
            let leading = raw.len() - raw.trim_start().len();
            if trimmed == param {
                let start = offset + leading;
                return Some(self.line_span(decl_span.line, start, start + param.len()));
            }
            offset += raw.len() + 1;
        }

        None
    }

    fn call_span(&self, owner_span: Option<SourceSpan>, symbol: &str) -> Option<SourceSpan> {
        if let Some(owner) = owner_span {
            if let Some(span) = self.symbol_after_body_colon(owner.line, symbol) {
                return Some(span);
            }
        }

        self.find_symbol(symbol)
    }

    fn symbol_after_body_colon(&self, line_no: u32, symbol: &str) -> Option<SourceSpan> {
        let line = self.line(line_no)?;
        let colon = line.find(':')?;
        self.find_symbol_on_line(line_no, symbol, colon + 1)
    }

    fn find_symbol(&self, symbol: &str) -> Option<SourceSpan> {
        for line_no in 0..self.lines.len() as u32 {
            if let Some(span) = self.find_symbol_on_line(line_no, symbol, 0) {
                return Some(span);
            }
        }
        None
    }

    fn find_symbol_on_line(
        &self,
        line_no: u32,
        symbol: &str,
        start_byte: usize,
    ) -> Option<SourceSpan> {
        let line = self.line(line_no)?;
        let mut search = start_byte.min(line.len());
        while let Some(rel) = line[search..].find(symbol) {
            let byte = search + rel;
            let end = byte + symbol.len();
            if is_symbol_boundary(line, byte, end) {
                return Some(self.line_span(line_no, byte, end));
            }
            search = end;
        }
        None
    }
}

pub fn analyze_program(program: &Program, source: &str) -> Vec<AnalysisDiagnostic> {
    let index = SourceIndex::new(source);
    let mut diagnostics = Vec::new();

    analyze_declarations(program, &index, &mut diagnostics);
    analyze_duplicate_endpoints(program, &index, &mut diagnostics);
    analyze_unused_path_params(program, &index, &mut diagnostics);
    analyze_unused_function_params(program, &index, &mut diagnostics);
    analyze_calls(program, &index, &mut diagnostics);
    append_typecheck_diagnostics(program, &index, &mut diagnostics);

    diagnostics.sort_by(|a, b| {
        let a_pos = a
            .span
            .map(|span| (span.line, span.character))
            .unwrap_or((u32::MAX, u32::MAX));
        let b_pos = b
            .span
            .map(|span| (span.line, span.character))
            .unwrap_or((u32::MAX, u32::MAX));
        a_pos
            .cmp(&b_pos)
            .then_with(|| a.severity.as_str().cmp(b.severity.as_str()))
            .then_with(|| a.code.cmp(&b.code))
    });

    diagnostics
}

pub fn build_diagnostics_json(program: &Program, source: Option<&str>) -> Value {
    let diagnostics = analyze_program(program, source.unwrap_or_default());

    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    for diagnostic in diagnostics {
        match diagnostic.severity {
            DiagnosticSeverity::Error => errors.push(diagnostic.to_json()),
            DiagnosticSeverity::Warning => warnings.push(diagnostic.to_json()),
        }
    }

    json!({
        "errors": errors,
        "warnings": warnings,
    })
}

fn analyze_declarations(
    program: &Program,
    index: &SourceIndex,
    diagnostics: &mut Vec<AnalysisDiagnostic>,
) {
    if program.server.port == 0 {
        diagnostics.push(AnalysisDiagnostic::warning(
            "server-ephemeral-port",
            "server",
            "server",
            "Server port 0 asks the OS to choose a random port; use an explicit port for production"
                .to_string(),
            index.server,
        ));
    }

    if program.server.tls && program.server.port != 443 {
        diagnostics.push(AnalysisDiagnostic::warning(
            "server-tls-port",
            "server",
            "server",
            format!(
                "TLS is enabled on port {}; production TLS is usually served on 443 or behind a reverse proxy",
                program.server.port
            ),
            index.server,
        ));
    }

    analyze_rate_limits(program, index, diagnostics);
    analyze_duplicate_secrets(program, index, diagnostics);
    analyze_duplicate_params(program, index, diagnostics);
    analyze_models(program, index, diagnostics);
    analyze_duplicate_map_keys(program, index, diagnostics);
}

fn analyze_rate_limits(
    program: &Program,
    index: &SourceIndex,
    diagnostics: &mut Vec<AnalysisDiagnostic>,
) {
    let mut occurrence_counts = HashMap::<(String, String), usize>::new();

    for ep in &program.endpoints {
        let method = method_to_str(&ep.method).to_string();
        let key = (method.clone(), ep.path.clone());
        let occurrence = occurrence_counts.entry(key).or_insert(0);
        let occurrence_index = *occurrence;
        *occurrence += 1;

        let Some(rate_limit) = &ep.rate_limit else {
            continue;
        };

        let span = index
            .endpoint_occurrence(&method, &ep.path, occurrence_index)
            .map(|occ| occ.span);

        if rate_limit.max_requests == 0 {
            diagnostics.push(AnalysisDiagnostic::error(
                "rate-limit-zero-max",
                "endpoint",
                &ep.path,
                "@rate_limit max_requests must be greater than 0".to_string(),
                span,
            ));
        }

        if rate_limit.window_secs == 0 {
            diagnostics.push(AnalysisDiagnostic::error(
                "rate-limit-zero-window",
                "endpoint",
                &ep.path,
                "@rate_limit window_secs must be greater than 0".to_string(),
                span,
            ));
        }
    }
}

fn analyze_duplicate_secrets(
    program: &Program,
    index: &SourceIndex,
    diagnostics: &mut Vec<AnalysisDiagnostic>,
) {
    let mut seen = HashSet::<String>::new();
    for secret in &program.secrets {
        if !seen.insert(secret.name.clone()) {
            diagnostics.push(AnalysisDiagnostic::error(
                "duplicate-secret",
                "secret",
                &secret.name,
                format!("Secret '{}' is declared more than once", secret.name),
                index.find_symbol(&secret.name),
            ));
        }
    }
}

fn analyze_duplicate_params(
    program: &Program,
    index: &SourceIndex,
    diagnostics: &mut Vec<AnalysisDiagnostic>,
) {
    for func in program.functions.values() {
        let decl_span = index.function_span(&func.name);
        for param in duplicate_names(&func.params) {
            diagnostics.push(AnalysisDiagnostic::error(
                "duplicate-function-param",
                "function",
                &func.name,
                format!(
                    "Function '{}' declares parameter '{}' more than once",
                    func.name, param
                ),
                decl_span.and_then(|span| index.param_span_in_decl(span, &param)),
            ));
        }
    }

    for class in program.classes.values() {
        for method in class.methods.values() {
            let decl_span = index.method_span(&class.name, &method.name);
            for param in duplicate_names(&method.params) {
                diagnostics.push(AnalysisDiagnostic::error(
                    "duplicate-method-param",
                    "method",
                    &format!("{}.{}", class.name, method.name),
                    format!(
                        "Method '{}.{}' declares parameter '{}' more than once",
                        class.name, method.name, param
                    ),
                    decl_span.and_then(|span| index.param_span_in_decl(span, &param)),
                ));
            }
        }
    }
}

fn analyze_models(
    program: &Program,
    index: &SourceIndex,
    diagnostics: &mut Vec<AnalysisDiagnostic>,
) {
    for model in program.models.values() {
        let model_span = index.models.get(&model.name).copied();
        let mut seen_fields = HashSet::<String>::new();
        let mut primary_keys = Vec::new();

        for field in &model.fields {
            if !seen_fields.insert(field.name.clone()) {
                diagnostics.push(AnalysisDiagnostic::error(
                    "duplicate-model-field",
                    "model",
                    &model.name,
                    format!(
                        "Model '{}' declares field '{}' more than once",
                        model.name, field.name
                    ),
                    model_span,
                ));
            }

            if field.is_primary_key {
                primary_keys.push(field.name.clone());
            }

            if !is_supported_model_type(&field.ty) {
                diagnostics.push(AnalysisDiagnostic::warning(
                    "unsupported-model-type",
                    "model",
                    &model.name,
                    format!(
                        "Field '{}.{}' uses type '{}'; supported model types are int, integer, number, float, double, real, string, text, bool, boolean, json, and any",
                        model.name, field.name, field.ty
                    ),
                    model_span,
                ));
            }
        }

        if primary_keys.len() > 1 {
            diagnostics.push(AnalysisDiagnostic::error(
                "multiple-model-primary-keys",
                "model",
                &model.name,
                format!(
                    "Model '{}' declares multiple primary keys: {}",
                    model.name,
                    primary_keys.join(", ")
                ),
                model_span,
            ));
        }
    }
}

fn analyze_duplicate_map_keys(
    program: &Program,
    index: &SourceIndex,
    diagnostics: &mut Vec<AnalysisDiagnostic>,
) {
    for endpoint in &program.endpoints {
        if let Body::TextExpr(expr) = &endpoint.body {
            let method = method_to_str(&endpoint.method);
            let owner_span = index
                .endpoint_occurrence(method, &endpoint.path, 0)
                .map(|occ| occ.span);
            analyze_duplicate_map_keys_expr(expr, owner_span, diagnostics);
        }
    }

    for func in program.functions.values() {
        analyze_duplicate_map_keys_expr(&func.body, index.function_span(&func.name), diagnostics);
    }

    for class in program.classes.values() {
        for method in class.methods.values() {
            analyze_duplicate_map_keys_expr(
                &method.body,
                index.method_span(&class.name, &method.name),
                diagnostics,
            );
        }
    }

    for test in &program.tests {
        for assertion in &test.assertions {
            analyze_duplicate_map_keys_expr(assertion, None, diagnostics);
        }
    }
}

fn analyze_duplicate_endpoints(
    program: &Program,
    index: &SourceIndex,
    diagnostics: &mut Vec<AnalysisDiagnostic>,
) {
    let mut seen = HashSet::<(String, String)>::new();
    let mut counts = HashMap::<(String, String), usize>::new();

    for ep in &program.endpoints {
        let method = method_to_str(&ep.method).to_string();
        let key = (method.clone(), ep.path.clone());
        let occurrence = counts.entry(key.clone()).or_insert(0);
        let occurrence_index = *occurrence;
        *occurrence += 1;

        if !seen.insert(key) {
            let span = index
                .endpoint_occurrence(&method, &ep.path, occurrence_index)
                .map(|occ| occ.span);
            diagnostics.push(AnalysisDiagnostic::warning(
                "duplicate-endpoint",
                "endpoint",
                &ep.path,
                format!("Duplicate endpoint for {} {}", method, ep.path),
                span,
            ));
        }
    }
}

fn analyze_unused_path_params(
    program: &Program,
    index: &SourceIndex,
    diagnostics: &mut Vec<AnalysisDiagnostic>,
) {
    let mut occurrence_counts = HashMap::<(String, String), usize>::new();

    for ep in &program.endpoints {
        let method = method_to_str(&ep.method).to_string();
        let key = (method.clone(), ep.path.clone());
        let occurrence = occurrence_counts.entry(key).or_insert(0);
        let occurrence_index = *occurrence;
        *occurrence += 1;

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
                let span = index.endpoint_param_span(&method, &ep.path, occurrence_index, &param);
                diagnostics.push(AnalysisDiagnostic::warning(
                    "unused-path-param",
                    "endpoint",
                    &ep.path,
                    format!(
                        "Path parameter :{} is never used in this endpoint body",
                        param
                    ),
                    span,
                ));
            }
        }
    }
}

fn analyze_unused_function_params(
    program: &Program,
    index: &SourceIndex,
    diagnostics: &mut Vec<AnalysisDiagnostic>,
) {
    for func in program.functions.values() {
        let mut used = HashSet::<String>::new();
        collect_vars_expr(&func.body, &mut used);
        let func_span = index.function_span(&func.name);

        for param in &func.params {
            if !used.contains(param) {
                diagnostics.push(AnalysisDiagnostic::warning(
                    "unused-function-param",
                    "function",
                    &func.name,
                    format!("Parameter '{}' is never used in function body", param),
                    func_span.and_then(|span| index.param_span_in_decl(span, param)),
                ));
            }
        }
    }

    for class in program.classes.values() {
        for method in class.methods.values() {
            let mut used = HashSet::<String>::new();
            collect_vars_expr(&method.body, &mut used);
            let method_span = index.method_span(&class.name, &method.name);

            for param in &method.params {
                if !used.contains(param) {
                    diagnostics.push(AnalysisDiagnostic::warning(
                        "unused-method-param",
                        "method",
                        &format!("{}.{}", class.name, method.name),
                        format!("Parameter '{}' is never used in method body", param),
                        method_span.and_then(|span| index.param_span_in_decl(span, param)),
                    ));
                }
            }
        }
    }
}

fn analyze_calls(
    program: &Program,
    index: &SourceIndex,
    diagnostics: &mut Vec<AnalysisDiagnostic>,
) {
    for endpoint in &program.endpoints {
        if let Body::TextExpr(expr) = &endpoint.body {
            let method = method_to_str(&endpoint.method);
            let owner_span = index
                .endpoint_occurrence(method, &endpoint.path, 0)
                .map(|occ| occ.span);
            analyze_expr_calls(expr, program, index, owner_span, diagnostics);
        }
    }

    for func in program.functions.values() {
        analyze_expr_calls(
            &func.body,
            program,
            index,
            index.function_span(&func.name),
            diagnostics,
        );
    }

    for class in program.classes.values() {
        for method in class.methods.values() {
            analyze_expr_calls(
                &method.body,
                program,
                index,
                index.method_span(&class.name, &method.name),
                diagnostics,
            );
        }
    }

    for test in &program.tests {
        for assertion in &test.assertions {
            analyze_expr_calls(assertion, program, index, None, diagnostics);
        }
    }
}

fn analyze_expr_calls(
    expr: &Expr,
    program: &Program,
    index: &SourceIndex,
    owner_span: Option<SourceSpan>,
    diagnostics: &mut Vec<AnalysisDiagnostic>,
) {
    match expr {
        Expr::Call { name, args } => {
            if let Some(func) = program.functions.get(name) {
                if args.len() != func.params.len() {
                    diagnostics.push(AnalysisDiagnostic::error(
                        "call-arity",
                        "call",
                        name,
                        format!(
                            "Function '{}' expects {} arguments but got {}",
                            name,
                            func.params.len(),
                            args.len()
                        ),
                        index.call_span(owner_span, name),
                    ));
                }
            } else if let Some(signature) = builtin_signature(name) {
                if !signature.accepts(args.len()) {
                    diagnostics.push(AnalysisDiagnostic::error(
                        "builtin-arity",
                        "call",
                        name,
                        format!(
                            "Built-in '{}' expects {} but got {} argument(s)",
                            name,
                            signature.describe(),
                            args.len()
                        ),
                        index.call_span(owner_span, name),
                    ));
                }
            } else {
                diagnostics.push(AnalysisDiagnostic::error(
                    "undefined-function",
                    "call",
                    name,
                    format!("Undefined function '{}'", name),
                    index.call_span(owner_span, name),
                ));
            }

            for arg in args {
                analyze_expr_calls(arg, program, index, owner_span, diagnostics);
            }
        }

        Expr::MethodCall {
            class_name,
            method_name,
            args,
        } => {
            match program.classes.get(class_name) {
                Some(class) => match class.methods.get(method_name) {
                    Some(method) => {
                        if args.len() != method.params.len() {
                            diagnostics.push(AnalysisDiagnostic::error(
                                "method-arity",
                                "method-call",
                                &format!("{}.{}", class_name, method_name),
                                format!(
                                    "Method '{}.{}' expects {} arguments but got {}",
                                    class_name,
                                    method_name,
                                    method.params.len(),
                                    args.len()
                                ),
                                index.call_span(
                                    owner_span,
                                    &format!("{}.{}", class_name, method_name),
                                ),
                            ));
                        }
                    }
                    None => diagnostics.push(AnalysisDiagnostic::error(
                        "undefined-method",
                        "method-call",
                        &format!("{}.{}", class_name, method_name),
                        format!("Class '{}' has no method '{}'", class_name, method_name),
                        index.call_span(owner_span, &format!("{}.{}", class_name, method_name)),
                    )),
                },
                None => diagnostics.push(AnalysisDiagnostic::error(
                    "undefined-class",
                    "method-call",
                    class_name,
                    format!("Undefined class '{}'", class_name),
                    index.call_span(owner_span, class_name),
                )),
            }

            for arg in args {
                analyze_expr_calls(arg, program, index, owner_span, diagnostics);
            }
        }

        Expr::List(items) => {
            for item in items {
                analyze_expr_calls(item, program, index, owner_span, diagnostics);
            }
        }

        Expr::Map(entries) => {
            for (_key, value) in entries {
                analyze_expr_calls(value, program, index, owner_span, diagnostics);
            }
        }

        Expr::Binary { left, right, .. } => {
            analyze_expr_calls(left, program, index, owner_span, diagnostics);
            analyze_expr_calls(right, program, index, owner_span, diagnostics);
        }

        Expr::If {
            branches,
            else_branch,
        } => {
            for (cond, body) in branches {
                analyze_expr_calls(cond, program, index, owner_span, diagnostics);
                analyze_expr_calls(body, program, index, owner_span, diagnostics);
            }
            if let Some(else_expr) = else_branch {
                analyze_expr_calls(else_expr, program, index, owner_span, diagnostics);
            }
        }

        Expr::Repeat { count, body } => {
            analyze_expr_calls(count, program, index, owner_span, diagnostics);
            analyze_expr_calls(body, program, index, owner_span, diagnostics);
        }

        Expr::Try {
            try_body,
            catch_body,
            finally_body,
            ..
        } => {
            analyze_expr_calls(try_body, program, index, owner_span, diagnostics);
            if let Some(catch_expr) = catch_body {
                analyze_expr_calls(catch_expr, program, index, owner_span, diagnostics);
            }
            if let Some(finally_expr) = finally_body {
                analyze_expr_calls(finally_expr, program, index, owner_span, diagnostics);
            }
        }

        Expr::Number(_) | Expr::Str(_) | Expr::Bool(_) | Expr::Var(_) => {}
    }
}

fn append_typecheck_diagnostics(
    program: &Program,
    index: &SourceIndex,
    diagnostics: &mut Vec<AnalysisDiagnostic>,
) {
    for item in typecheck::build_type_diagnostics(program) {
        let kind = item
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or("warning");
        let scope = item.get("scope").and_then(Value::as_str).unwrap_or("type");
        let name = item.get("name").and_then(Value::as_str).unwrap_or("");
        let message = item
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("Type diagnostic")
            .to_string();

        let span = match scope {
            "function" => index.function_span(name),
            "call" if !name.is_empty() => index.call_span(None, name),
            _ => None,
        };

        let diagnostic = if kind.eq_ignore_ascii_case("error") {
            AnalysisDiagnostic::error("typecheck", scope, name, message, span)
        } else {
            AnalysisDiagnostic::warning("typecheck", scope, name, message, span)
        };
        diagnostics.push(diagnostic);
    }
}

fn collect_vars_expr(expr: &Expr, out: &mut HashSet<String>) {
    match expr {
        Expr::Var(name) => {
            out.insert(name.clone());
        }
        Expr::Number(_) | Expr::Str(_) | Expr::Bool(_) => {}
        Expr::List(items) => {
            for item in items {
                collect_vars_expr(item, out);
            }
        }
        Expr::Map(entries) => {
            for (_key, value) in entries {
                collect_vars_expr(value, out);
            }
        }
        Expr::Binary { left, right, .. } => {
            collect_vars_expr(left, out);
            collect_vars_expr(right, out);
        }
        Expr::Call { args, .. } | Expr::MethodCall { args, .. } => {
            for arg in args {
                collect_vars_expr(arg, out);
            }
        }
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
        Expr::Repeat { count, body } => {
            collect_vars_expr(count, out);
            collect_vars_expr(body, out);
        }
        Expr::Try {
            try_body,
            catch_body,
            finally_body,
            ..
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

fn analyze_duplicate_map_keys_expr(
    expr: &Expr,
    owner_span: Option<SourceSpan>,
    diagnostics: &mut Vec<AnalysisDiagnostic>,
) {
    match expr {
        Expr::Map(entries) => {
            let mut seen = HashSet::<String>::new();
            for (key, value) in entries {
                if !seen.insert(key.clone()) {
                    diagnostics.push(AnalysisDiagnostic::warning(
                        "duplicate-map-key",
                        "expression",
                        key,
                        format!("Map key '{}' is declared more than once; later values overwrite earlier values", key),
                        owner_span,
                    ));
                }
                analyze_duplicate_map_keys_expr(value, owner_span, diagnostics);
            }
        }
        Expr::List(items) => {
            for item in items {
                analyze_duplicate_map_keys_expr(item, owner_span, diagnostics);
            }
        }
        Expr::Binary { left, right, .. } => {
            analyze_duplicate_map_keys_expr(left, owner_span, diagnostics);
            analyze_duplicate_map_keys_expr(right, owner_span, diagnostics);
        }
        Expr::Call { args, .. } | Expr::MethodCall { args, .. } => {
            for arg in args {
                analyze_duplicate_map_keys_expr(arg, owner_span, diagnostics);
            }
        }
        Expr::If {
            branches,
            else_branch,
        } => {
            for (cond, body) in branches {
                analyze_duplicate_map_keys_expr(cond, owner_span, diagnostics);
                analyze_duplicate_map_keys_expr(body, owner_span, diagnostics);
            }
            if let Some(else_expr) = else_branch {
                analyze_duplicate_map_keys_expr(else_expr, owner_span, diagnostics);
            }
        }
        Expr::Repeat { count, body } => {
            analyze_duplicate_map_keys_expr(count, owner_span, diagnostics);
            analyze_duplicate_map_keys_expr(body, owner_span, diagnostics);
        }
        Expr::Try {
            try_body,
            catch_body,
            finally_body,
            ..
        } => {
            analyze_duplicate_map_keys_expr(try_body, owner_span, diagnostics);
            if let Some(catch_expr) = catch_body {
                analyze_duplicate_map_keys_expr(catch_expr, owner_span, diagnostics);
            }
            if let Some(finally_expr) = finally_body {
                analyze_duplicate_map_keys_expr(finally_expr, owner_span, diagnostics);
            }
        }
        Expr::Number(_) | Expr::Str(_) | Expr::Bool(_) | Expr::Var(_) => {}
    }
}

fn duplicate_names(names: &[String]) -> Vec<String> {
    let mut seen = HashSet::<String>::new();
    let mut duplicates = Vec::<String>::new();
    let mut emitted = HashSet::<String>::new();

    for name in names {
        if !seen.insert(name.clone()) && emitted.insert(name.clone()) {
            duplicates.push(name.clone());
        }
    }

    duplicates
}

fn is_supported_model_type(ty: &str) -> bool {
    matches!(
        ty.to_ascii_lowercase().as_str(),
        "int"
            | "integer"
            | "number"
            | "float"
            | "double"
            | "real"
            | "string"
            | "text"
            | "bool"
            | "boolean"
            | "json"
            | "any"
    )
}

#[derive(Debug, Clone, Copy)]
struct BuiltinSignature {
    min: usize,
    max: Option<usize>,
}

impl BuiltinSignature {
    fn exact(n: usize) -> Self {
        Self {
            min: n,
            max: Some(n),
        }
    }

    fn range(min: usize, max: Option<usize>) -> Self {
        Self { min, max }
    }

    fn accepts(self, count: usize) -> bool {
        count >= self.min && self.max.is_none_or(|max| count <= max)
    }

    fn describe(self) -> String {
        match (self.min, self.max) {
            (min, Some(max)) if min == max => format!("{min} argument(s)"),
            (min, Some(max)) => format!("{min}-{max} arguments"),
            (min, None) => format!("at least {min} argument(s)"),
        }
    }
}

fn builtin_signature(name: &str) -> Option<BuiltinSignature> {
    let signature = match name {
        "len"
        | "upper"
        | "lower"
        | "number"
        | "string"
        | "type"
        | "json_parse"
        | "json_stringify"
        | "json_pretty"
        | "keys"
        | "values"
        | "config_has"
        | "env"
        | "http_get"
        | "http_get_json"
        | "df_from_csv"
        | "openai_set_api_key"
        | "openai_set_system_prompt"
        | "openai_chat"
        | "openai_chat_json" => BuiltinSignature::exact(1),

        "contains" | "join" | "split" | "tensor_add" | "tensor_dot" | "df_head" | "df_select"
        | "linreg_fit" | "linreg_predict" | "orm_insert" | "orm_find_by_id" | "config_set"
        | "http_post_json" => BuiltinSignature::exact(2),

        "openai_mcp_call" => BuiltinSignature::exact(3),

        "sum" | "avg" | "min" | "max" | "vec" => BuiltinSignature::range(1, None),

        "config_get" | "secret" => BuiltinSignature::range(1, Some(2)),

        "range" => BuiltinSignature::range(1, Some(3)),

        "json_get" | "list_get" => BuiltinSignature::range(2, Some(3)),

        "json_set" => BuiltinSignature::exact(3),

        _ => return None,
    };

    Some(signature)
}

fn method_to_str(method: &Method) -> &'static str {
    match method {
        Method::Get => "GET",
        Method::Post => "POST",
    }
}

fn extract_quoted_simple(s: &str) -> Option<String> {
    let start = s.find('"')? + 1;
    let end = s[start..].find('"').map(|idx| start + idx)?;
    Some(s[start..end].to_string())
}

fn utf16_len(text: &str) -> u32 {
    text.encode_utf16().count() as u32
}

fn byte_to_utf16_col(text: &str, byte: usize) -> u32 {
    let byte = byte.min(text.len());
    text.get(..byte)
        .map(utf16_len)
        .unwrap_or_else(|| text.encode_utf16().count() as u32)
}

fn is_symbol_boundary(line: &str, start: usize, end: usize) -> bool {
    let before = if start == 0 {
        None
    } else {
        line[..start].chars().next_back()
    };
    let after = line[end..].chars().next();

    before.is_none_or(|c| !is_identifier_char(c)) && after.is_none_or(|c| !is_identifier_char(c))
}

fn is_identifier_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_program;

    #[test]
    fn flags_undefined_function_with_source_range() {
        let source = r#"server 3000
endpoint GET "/": missing()
"#;
        let program = parse_program(source).expect("program should parse");
        let diagnostics = analyze_program(&program, source);

        let diagnostic = diagnostics
            .iter()
            .find(|item| item.code == "undefined-function")
            .expect("expected undefined function diagnostic");

        assert_eq!(diagnostic.severity, DiagnosticSeverity::Error);
        assert_eq!(diagnostic.name, "missing");
        assert_eq!(diagnostic.span.map(|span| span.line), Some(1));
    }

    #[test]
    fn flags_duplicate_endpoint_at_second_declaration() {
        let source = r#"server 3000
endpoint GET "/ping": "one"
endpoint GET "/ping": "two"
"#;
        let program = parse_program(source).expect("program should parse");
        let diagnostics = analyze_program(&program, source);

        let diagnostic = diagnostics
            .iter()
            .find(|item| item.code == "duplicate-endpoint")
            .expect("expected duplicate endpoint diagnostic");

        assert_eq!(diagnostic.severity, DiagnosticSeverity::Warning);
        assert_eq!(diagnostic.span.map(|span| span.line), Some(2));
    }

    #[test]
    fn validates_user_function_call_arity() {
        let source = r#"server 3000
func greet(name): "Hello " + name
endpoint GET "/": greet("A", "B")
"#;
        let program = parse_program(source).expect("program should parse");
        let diagnostics = analyze_program(&program, source);

        assert!(diagnostics
            .iter()
            .any(|item| item.code == "call-arity" && item.name == "greet"));
    }

    #[test]
    fn accepts_shrimpl_1_data_builtins() {
        let source = r#"server 3000
endpoint GET "/": json_get(json_set({ name: "Ana" }, "name", "Ben"), "name")
"#;
        let program = parse_program(source).expect("program should parse");
        let diagnostics = analyze_program(&program, source);

        assert!(
            diagnostics
                .iter()
                .all(|item| item.code != "undefined-function"),
            "new data built-ins should be recognized: {diagnostics:?}"
        );
    }

    #[test]
    fn flags_production_declaration_mistakes() {
        let source = r#"server 0
@rate_limit(0, 0)
endpoint GET "/": "bad"
secret API = "ONE"
secret API = "TWO"
func bad(x, x): x
model User:
  id: int pk
  id: string
"#;
        let program = parse_program(source).expect("program should parse");
        let diagnostics = analyze_program(&program, source);

        for expected in [
            "server-ephemeral-port",
            "rate-limit-zero-max",
            "rate-limit-zero-window",
            "duplicate-secret",
            "duplicate-function-param",
            "duplicate-model-field",
        ] {
            assert!(
                diagnostics.iter().any(|item| item.code == expected),
                "missing diagnostic {expected}: {diagnostics:?}"
            );
        }
    }
}
