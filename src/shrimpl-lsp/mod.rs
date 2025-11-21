pub mod completion;
pub mod hover;
pub mod definition;
pub mod signature;
pub mod diagnostics;
pub mod semantic_tokens;
pub mod code_actions;
pub mod formatting;

use tower_lsp::lsp_types::*;

pub fn server_capabilities() -> ServerCapabilities {
    ServerCapabilities {
        completion_provider: Some(CompletionOptions::default()),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        definition_provider: Some(OneOf::Left(true)),
        signature_help_provider: Some(SignatureHelpOptions::default()),
        code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
        document_formatting_provider: Some(OneOf::Left(true)),
        semantic_tokens_provider: Some(
            SemanticTokensServerCapabilities::SemanticTokensOptions(
                SemanticTokensOptions {
                    full: Some(SemanticTokensFullOptions::Bool(true)),
                    ..Default::default()
                }
            )
        ),
        ..Default::default()
    }
}
