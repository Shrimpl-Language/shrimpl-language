use tower_lsp::lsp_types::*;
use tower_lsp::jsonrpc::Result;

pub async fn hover(_params: HoverParams) -> Result<Option<Hover>> {
    Ok(Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: "Shrimpl symbol information will appear here.".into(),
        }),
        range: None,
    }))
}
