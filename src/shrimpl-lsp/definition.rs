use tower_lsp::lsp_types::*;
use tower_lsp::jsonrpc::Result;

pub async fn definition(
    _params: GotoDefinitionParams,
) -> Result<Option<GotoDefinitionResponse>> {
    // No definition support yet – return None so the client does not error.
    Ok(None)
}
