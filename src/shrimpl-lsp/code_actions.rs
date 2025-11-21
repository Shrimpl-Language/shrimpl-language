use tower_lsp::lsp_types::*;
use tower_lsp::jsonrpc::Result;

pub async fn code_actions(_params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
    Ok(Some(vec![
        CodeActionOrCommand::CodeAction(CodeAction {
            title: "Shrimpl: no available fix yet".into(),
            kind: Some(CodeActionKind::EMPTY),
            edit: None,
            diagnostics: None,
            command: None,
            disabled: None,
            is_preferred: None,
            data: None,
        })
    ]))
}
