use tower_lsp::lsp_types::*;
use tower_lsp::jsonrpc::Result;

pub async fn formatting(_params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
    // Stub: not formatting anything yet
    Ok(Some(vec![]))
}
