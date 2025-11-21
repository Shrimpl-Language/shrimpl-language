use tower_lsp::lsp_types::*;

pub fn diagnostics(uri: Url) -> PublishDiagnosticsParams {
    PublishDiagnosticsParams {
        uri,
        diagnostics: vec![
            Diagnostic {
                range: Range {
                    start: Position::new(0, 0),
                    end: Position::new(0, 6),
                },
                severity: Some(DiagnosticSeverity::INFORMATION),
                message: "Shrimpl diagnostics not implemented yet".into(),
                ..Default::default()
            }
        ],
        version: None,
    }
}
