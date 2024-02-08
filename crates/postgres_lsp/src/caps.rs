//! Advertises the capabilities of the LSP Server.

use lsp_types::{
    SaveOptions, ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind,
    TextDocumentSyncOptions,
};

use crate::config::Config;

pub fn server_capabilities(config: &Config) -> ServerCapabilities {
    ServerCapabilities {
        position_encoding: None,
        text_document_sync: Some(TextDocumentSyncCapability::Options(
            TextDocumentSyncOptions {
                // open and close notifications are sent to the server
                open_close: Some(true),
                // documents are synced by always sending the full content on open.
                // after that only incremental updates to the document are sent.
                change: Some(TextDocumentSyncKind::INCREMENTAL),
                will_save: None,
                will_save_wait_until: None,
                save: Some(SaveOptions::default().into()),
            },
        )),
        hover_provider: None,
        completion_provider: None,
        signature_help_provider: None,
        declaration_provider: None,
        definition_provider: None,
        type_definition_provider: None,
        implementation_provider: None,
        references_provider: None,
        document_highlight_provider: None,
        document_symbol_provider: None,
        workspace_symbol_provider: None,
        code_action_provider: None,
        code_lens_provider: None,
        document_formatting_provider: None,
        document_range_formatting_provider: None,
        document_on_type_formatting_provider: None,
        selection_range_provider: None,
        folding_range_provider: None,
        rename_provider: None,
        linked_editing_range_provider: None,
        document_link_provider: None,
        color_provider: None,
        execute_command_provider: None,
        workspace: None,
        call_hierarchy_provider: None,
        semantic_tokens_provider: None,
        moniker_provider: None,
        inlay_hint_provider: None,
        inline_value_provider: None,
        experimental: None,
        diagnostic_provider: None,
        inline_completion_provider: None,
    }
}
