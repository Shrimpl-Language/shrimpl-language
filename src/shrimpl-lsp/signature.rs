use tower_lsp::lsp_types::*;
use tower_lsp::jsonrpc::Result;

pub async fn signature_help(_params: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
    Ok(Some(SignatureHelp {
        signatures: vec![SignatureInformation {
            label: "func example(x: number, y: number) -> number".to_string(),
            documentation: Some(Documentation::String("Example function".into())),
            parameters: None,
            active_parameter: None,
        }],
        active_signature: None,
        active_parameter: None,
    }))
}
