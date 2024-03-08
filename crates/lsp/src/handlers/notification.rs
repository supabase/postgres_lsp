use lsp_types::{
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DidSaveTextDocumentParams,
};

use crate::global_state::GlobalState;

// we do not need to track workspace or file changes ourselves to notify is not required
// rust analyzer i think also does only track file changes with vfs to watch for changes directly
// from the file system
// same for latex:
// https://github.com/latex-lsp/texlab/blob/master/crates/texlab/src/server.rs#L1026

pub(crate) fn handle_did_open_text_document(
    state: &mut GlobalState,
    params: DidOpenTextDocumentParams,
) -> anyhow::Result<()> {
    if let Ok(path) = from_proto::vfs_path(&params.text_document.uri) {
        let already_exists = state
            .mem_docs
            .insert(
                path.clone(),
                DocumentData::new(
                    params.text_document.version,
                    params.text_document.text.clone().into_bytes(),
                ),
            )
            .is_err();
        if already_exists {}

        state
            .vfs
            .write()
            .0
            .set_file_contents(path, Some(params.text_document.text.into_bytes()));
        if state.config.notifications().unindexed_project {
            tracing::debug!("queuing task");
            let _ = state.deferred_task_queue.sender.send(
                crate::main_loop::QueuedTask::CheckIfIndexed(params.text_document.uri),
            );
        }
    }
    Ok(())
}

pub(crate) fn handle_did_change_text_document(
    state: &mut GlobalState,
    params: DidChangeTextDocumentParams,
) -> anyhow::Result<()> {
    if let Ok(path) = from_proto::vfs_path(&params.text_document.uri) {
        let Some(DocumentData { version, data }) = state.mem_docs.get_mut(&path) else {
            tracing::error!(?path, "unexpected DidChangeTextDocument");
            return Ok(());
        };
        // The version passed in DidChangeTextDocument is the version after all edits are applied
        // so we should apply it before the vfs is notified.
        *version = params.text_document.version;

        let new_contents = apply_document_changes(
            state.config.position_encoding(),
            std::str::from_utf8(data).unwrap(),
            params.content_changes,
        )
        .into_bytes();
        if *data != new_contents {
            *data = new_contents.clone();
            state
                .vfs
                .write()
                .0
                .set_file_contents(path, Some(new_contents));
        }
    }
    Ok(())
}

pub(crate) fn handle_did_close_text_document(
    state: &mut GlobalState,
    params: DidCloseTextDocumentParams,
) -> anyhow::Result<()> {
    if let Ok(path) = from_proto::vfs_path(&params.text_document.uri) {
        if state.mem_docs.remove(&path).is_err() {
            tracing::error!("orphan DidCloseTextDocument: {}", path);
        }

        if let Some(file_id) = state.vfs.read().0.file_id(&path) {
            state.diagnostics.clear_native_for(file_id);
        }

        state
            .semantic_tokens_cache
            .lock()
            .remove(&params.text_document.uri);

        if let Some(path) = path.as_path() {
            state.loader.handle.invalidate(path.to_path_buf());
        }
    }
    Ok(())
}

pub(crate) fn handle_did_save_text_document(
    state: &mut GlobalState,
    params: DidSaveTextDocumentParams,
) -> anyhow::Result<()> {
    if state.config.script_rebuild_on_save() && state.build_deps_changed {
        state.build_deps_changed = false;
        state
            .fetch_build_data_queue
            .request_op("build_deps_changed - save notification".to_owned(), ());
    }

    if let Ok(vfs_path) = from_proto::vfs_path(&params.text_document.uri) {
        // Re-fetch workspaces if a workspace related file has changed
        if let Some(abs_path) = vfs_path.as_path() {
            if reload::should_refresh_for_change(abs_path, ChangeKind::Modify) {
                state
                    .fetch_workspaces_queue
                    .request_op(format!("workspace vfs file change saved {abs_path}"), false);
            }
        }

        if !state.config.check_on_save() || run_flycheck(state, vfs_path) {
            return Ok(());
        }
    } else if state.config.check_on_save() {
        // No specific flycheck was triggered, so let's trigger all of them.
        for flycheck in state.flycheck.iter() {
            flycheck.restart_workspace(None);
        }
    }
    Ok(())
}
