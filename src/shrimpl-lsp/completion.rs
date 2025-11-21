use tower_lsp::lsp_types::*;
use tower_lsp::jsonrpc::Result;

pub async fn completion(_params: CompletionParams) -> Result<Option<CompletionResponse>> {
    Ok(Some(CompletionResponse::Array(vec![
        CompletionItem {
            label: "server".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some("Shrimpl keyword".into()),
            ..Default::default()
        },
        CompletionItem {
            label: "endpoint".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            ..Default::default()
        },
    ])))
}
