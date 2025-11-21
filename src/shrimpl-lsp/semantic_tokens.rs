use tower_lsp::lsp_types::*;
use tower_lsp::jsonrpc::Result;

pub async fn semantic_tokens(
    _params: SemanticTokensParams
) -> Result<Option<SemanticTokensResult>> {
    Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
        data: vec![],  // No tokens yet
        result_id: None,
    })))
}
