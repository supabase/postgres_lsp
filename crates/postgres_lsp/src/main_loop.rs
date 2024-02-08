//! The main loop of `rust-analyzer` responsible for dispatching LSP
//! requests/replies and notifications back to the client.
use std::{
    fmt,
    time::{Duration, Instant},
};

use always_assert::always;
use crossbeam_channel::{select, Receiver};
use flycheck::FlycheckHandle;
use ide_db::base_db::{SourceDatabaseExt, VfsPath};
use lsp_server::{Connection, Notification, Request};
use lsp_types::notification::Notification as _;
use stdx::thread::ThreadIntent;
use triomphe::Arc;
use vfs::FileId;

use crate::{
    config::Config,
    diagnostics::fetch_native_diagnostics,
    dispatch::{NotificationDispatcher, RequestDispatcher},
    global_state::{file_id_to_url, url_to_file_id, GlobalState},
    lsp::{
        from_proto,
        utils::{notification_is, Progress},
    },
    lsp_ext,
    reload::{BuildDataProgress, ProcMacroProgress, ProjectWorkspaceProgress},
};

pub fn main_loop(config: Config, connection: Connection) -> anyhow::Result<()> {
    tracing::info!("initial config: {:#?}", config);

    // Windows scheduler implements priority boosts: if thread waits for an
    // event (like a condvar), and event fires, priority of the thread is
    // temporary bumped. This optimization backfires in our case: each time the
    // `main_loop` schedules a task to run on a threadpool, the worker threads
    // gets a higher priority, and (on a machine with fewer cores) displaces the
    // main loop! We work around this by marking the main loop as a
    // higher-priority thread.
    //
    // https://docs.microsoft.com/en-us/windows/win32/procthread/scheduling-priorities
    // https://docs.microsoft.com/en-us/windows/win32/procthread/priority-boosts
    // https://github.com/rust-lang/rust-analyzer/issues/2835
    #[cfg(windows)]
    unsafe {
        use winapi::um::processthreadsapi::*;
        let thread = GetCurrentThread();
        let thread_priority_above_normal = 1;
        SetThreadPriority(thread, thread_priority_above_normal);
    }

    GlobalState::new(connection.sender, config).run(connection.receiver)
}

enum Event {
    Lsp(lsp_server::Message),
    Task(Task),
    Vfs(vfs::loader::Message),
}

#[derive(Debug)]
pub(crate) enum Task {
    Response(lsp_server::Response),
    Retry(lsp_server::Request),
    Diagnostics(Vec<(FileId, Vec<lsp_types::Diagnostic>)>),
    PrimeCaches(PrimeCachesProgress),
    FetchWorkspace(ProjectWorkspaceProgress),
    FetchBuildData(BuildDataProgress),
    LoadProcMacros(ProcMacroProgress),
}

#[derive(Debug)]
pub(crate) enum PrimeCachesProgress {
    Begin,
    Report(ide::ParallelPrimeCachesProgress),
    End { cancelled: bool },
}

impl fmt::Debug for Event {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let debug_non_verbose = |not: &Notification, f: &mut fmt::Formatter<'_>| {
            f.debug_struct("Notification")
                .field("method", &not.method)
                .finish()
        };

        match self {
            Event::Lsp(lsp_server::Message::Notification(not)) => {
                if notification_is::<lsp_types::notification::DidOpenTextDocument>(not)
                    || notification_is::<lsp_types::notification::DidChangeTextDocument>(not)
                {
                    return debug_non_verbose(not, f);
                }
            }
            Event::Task(Task::Response(resp)) => {
                return f
                    .debug_struct("Response")
                    .field("id", &resp.id)
                    .field("error", &resp.error)
                    .finish();
            }
            _ => (),
        }
        match self {
            Event::Lsp(it) => fmt::Debug::fmt(it, f),
            Event::Task(it) => fmt::Debug::fmt(it, f),
            Event::Vfs(it) => fmt::Debug::fmt(it, f),
        }
    }
}

impl GlobalState {
    fn run(mut self, inbox: Receiver<lsp_server::Message>) -> anyhow::Result<()> {
        self.update_status_or_notify();

        if self.config.did_save_text_document_dynamic_registration() {
            self.register_did_save_capability();
        }

        self.fetch_workspaces_queue
            .request_op("startup".to_string(), false);
        if let Some((cause, force_crate_graph_reload)) =
            self.fetch_workspaces_queue.should_start_op()
        {
            self.fetch_workspaces(cause, force_crate_graph_reload);
        }

        while let Some(event) = self.next_event(&inbox) {
            if matches!(
                &event,
                Event::Lsp(lsp_server::Message::Notification(Notification { method, .. }))
                if method == lsp_types::notification::Exit::METHOD
            ) {
                return Ok(());
            }
            self.handle_event(event)?;
        }

        anyhow::bail!("client exited without proper shutdown sequence")
    }

    fn register_did_save_capability(&mut self) {
        let save_registration_options = lsp_types::TextDocumentSaveRegistrationOptions {
            include_text: Some(false),
            text_document_registration_options: lsp_types::TextDocumentRegistrationOptions {
                document_selector: Some(vec![
                    lsp_types::DocumentFilter {
                        language: None,
                        scheme: None,
                        pattern: Some("**/*.rs".into()),
                    },
                    lsp_types::DocumentFilter {
                        language: None,
                        scheme: None,
                        pattern: Some("**/Cargo.toml".into()),
                    },
                    lsp_types::DocumentFilter {
                        language: None,
                        scheme: None,
                        pattern: Some("**/Cargo.lock".into()),
                    },
                ]),
            },
        };

        let registration = lsp_types::Registration {
            id: "textDocument/didSave".to_string(),
            method: "textDocument/didSave".to_string(),
            register_options: Some(serde_json::to_value(save_registration_options).unwrap()),
        };
        self.send_request::<lsp_types::request::RegisterCapability>(
            lsp_types::RegistrationParams {
                registrations: vec![registration],
            },
            |_, _| (),
        );
    }

    fn next_event(&self, inbox: &Receiver<lsp_server::Message>) -> Option<Event> {
        select! {
            recv(inbox) -> msg =>
                msg.ok().map(Event::Lsp),

            recv(self.task_pool.receiver) -> task =>
                Some(Event::Task(task.unwrap())),

            recv(self.fmt_pool.receiver) -> task =>
                Some(Event::Task(task.unwrap())),

            recv(self.loader.receiver) -> task =>
                Some(Event::Vfs(task.unwrap())),
        }
    }

    fn handle_event(&mut self, event: Event) -> anyhow::Result<()> {
        let loop_start = Instant::now();
        // NOTE: don't count blocking select! call as a loop-turn time
        let _p = profile::span("GlobalState::handle_event");

        let event_dbg_msg = format!("{event:?}");
        tracing::debug!("{:?} handle_event({})", loop_start, event_dbg_msg);
        if tracing::enabled!(tracing::Level::INFO) {
            let task_queue_len = self.task_pool.handle.len();
            if task_queue_len > 0 {
                tracing::info!("task queue len: {}", task_queue_len);
            }
        }

        let was_quiescent = self.is_quiescent();
        match event {
            Event::Lsp(msg) => match msg {
                lsp_server::Message::Request(req) => self.on_new_request(loop_start, req),
                lsp_server::Message::Notification(not) => self.on_notification(not)?,
                lsp_server::Message::Response(resp) => self.complete_request(resp),
            },
            Event::Task(task) => {
                let _p = profile::span("GlobalState::handle_event/task");
                let mut prime_caches_progress = Vec::new();

                self.handle_task(&mut prime_caches_progress, task);
                // Coalesce multiple task events into one loop turn
                while let Ok(task) = self.task_pool.receiver.try_recv() {
                    self.handle_task(&mut prime_caches_progress, task);
                }

                for progress in prime_caches_progress {
                    let (state, message, fraction);
                    match progress {
                        PrimeCachesProgress::Begin => {
                            state = Progress::Begin;
                            message = None;
                            fraction = 0.0;
                        }
                        PrimeCachesProgress::Report(report) => {
                            state = Progress::Report;

                            message = match &report.crates_currently_indexing[..] {
                                [crate_name] => Some(format!(
                                    "{}/{} ({crate_name})",
                                    report.crates_done, report.crates_total
                                )),
                                [crate_name, rest @ ..] => Some(format!(
                                    "{}/{} ({} + {} more)",
                                    report.crates_done,
                                    report.crates_total,
                                    crate_name,
                                    rest.len()
                                )),
                                _ => None,
                            };

                            fraction = Progress::fraction(report.crates_done, report.crates_total);
                        }
                        PrimeCachesProgress::End { cancelled } => {
                            state = Progress::End;
                            message = None;
                            fraction = 1.0;

                            self.prime_caches_queue.op_completed(());
                            if cancelled {
                                self.prime_caches_queue
                                    .request_op("restart after cancellation".to_string(), ());
                            }
                        }
                    };

                    self.report_progress("Indexing", state, message, Some(fraction), None);
                }
            }
            Event::Vfs(message) => {
                let _p = profile::span("GlobalState::handle_event/vfs");
                self.handle_vfs_msg(message);
                // Coalesce many VFS event into a single loop turn
                while let Ok(message) = self.loader.receiver.try_recv() {
                    self.handle_vfs_msg(message);
                }
            }
        }
        let event_handling_duration = loop_start.elapsed();

        let state_changed = self.process_changes();
        let memdocs_added_or_removed = self.mem_docs.take_changes();

        if self.is_quiescent() {
            let became_quiescent = !(was_quiescent
                || self.fetch_workspaces_queue.op_requested()
                || self.fetch_build_data_queue.op_requested()
                || self.fetch_proc_macros_queue.op_requested());

            if became_quiescent {
                if self.config.check_on_save() {
                    // Project has loaded properly, kick off initial flycheck
                    self.flycheck.iter().for_each(FlycheckHandle::restart);
                }
                if self.config.prefill_caches() {
                    self.prime_caches_queue
                        .request_op("became quiescent".to_string(), ());
                }
            }

            let client_refresh = !was_quiescent || state_changed;
            if client_refresh {
                // Refresh semantic tokens if the client supports it.
                if self.config.semantic_tokens_refresh() {
                    self.semantic_tokens_cache.lock().clear();
                    self.send_request::<lsp_types::request::SemanticTokensRefresh>((), |_, _| ());
                }

                // Refresh code lens if the client supports it.
                if self.config.code_lens_refresh() {
                    self.send_request::<lsp_types::request::CodeLensRefresh>((), |_, _| ());
                }

                // Refresh inlay hints if the client supports it.
                if (self.send_hint_refresh_query || self.proc_macro_changed)
                    && self.config.inlay_hints_refresh()
                {
                    self.send_request::<lsp_types::request::InlayHintRefreshRequest>((), |_, _| ());
                    self.send_hint_refresh_query = false;
                }
            }

            let update_diagnostics = (!was_quiescent || state_changed || memdocs_added_or_removed)
                && self.config.publish_diagnostics();
            if update_diagnostics {
                self.update_diagnostics()
            }
        }

        if let Some(diagnostic_changes) = self.diagnostics.take_changes() {
            for file_id in diagnostic_changes {
                let uri = file_id_to_url(&self.vfs.read().0, file_id);
                let mut diagnostics = self
                    .diagnostics
                    .diagnostics_for(file_id)
                    .cloned()
                    .collect::<Vec<_>>();

                // VSCode assumes diagnostic messages to be non-empty strings, so we need to patch
                // empty diagnostics. Neither the docs of VSCode nor the LSP spec say whether
                // diagnostic messages are actually allowed to be empty or not and patching this
                // in the VSCode client does not work as the assertion happens in the protocol
                // conversion. So this hack is here to stay, and will be considered a hack
                // until the LSP decides to state that empty messages are allowed.

                // See https://github.com/rust-lang/rust-analyzer/issues/11404
                // See https://github.com/rust-lang/rust-analyzer/issues/13130
                let patch_empty = |message: &mut String| {
                    if message.is_empty() {
                        *message = " ".to_string();
                    }
                };

                for d in &mut diagnostics {
                    patch_empty(&mut d.message);
                    if let Some(dri) = &mut d.related_information {
                        for dri in dri {
                            patch_empty(&mut dri.message);
                        }
                    }
                }

                let version = from_proto::vfs_path(&uri)
                    .map(|path| self.mem_docs.get(&path).map(|it| it.version))
                    .unwrap_or_default();

                self.send_notification::<lsp_types::notification::PublishDiagnostics>(
                    lsp_types::PublishDiagnosticsParams {
                        uri,
                        diagnostics,
                        version,
                    },
                );
            }
        }

        if self.config.cargo_autoreload() {
            if let Some((cause, force_crate_graph_reload)) =
                self.fetch_workspaces_queue.should_start_op()
            {
                self.fetch_workspaces(cause, force_crate_graph_reload);
            }
        }

        if !self.fetch_workspaces_queue.op_in_progress() {
            if let Some((cause, ())) = self.fetch_build_data_queue.should_start_op() {
                self.fetch_build_data(cause);
            } else if let Some((cause, paths)) = self.fetch_proc_macros_queue.should_start_op() {
                self.fetch_proc_macros(cause, paths);
            }
        }

        if let Some((cause, ())) = self.prime_caches_queue.should_start_op() {
            self.prime_caches(cause);
        }

        self.update_status_or_notify();

        let loop_duration = loop_start.elapsed();
        if loop_duration > Duration::from_millis(100) && was_quiescent {
            tracing::warn!("overly long loop turn took {loop_duration:?} (event handling took {event_handling_duration:?}): {event_dbg_msg}");
            self.poke_rust_analyzer_developer(format!(
                "overly long loop turn took {loop_duration:?} (event handling took {event_handling_duration:?}): {event_dbg_msg}"
            ));
        }
        Ok(())
    }

    fn prime_caches(&mut self, cause: String) {
        tracing::debug!(%cause, "will prime caches");
        let num_worker_threads = self.config.prime_caches_num_threads();

        self.task_pool
            .handle
            .spawn_with_sender(ThreadIntent::Worker, {
                let analysis = self.snapshot().analysis;
                move |sender| {
                    sender
                        .send(Task::PrimeCaches(PrimeCachesProgress::Begin))
                        .unwrap();
                    let res = analysis.parallel_prime_caches(num_worker_threads, |progress| {
                        let report = PrimeCachesProgress::Report(progress);
                        sender.send(Task::PrimeCaches(report)).unwrap();
                    });
                    sender
                        .send(Task::PrimeCaches(PrimeCachesProgress::End {
                            cancelled: res.is_err(),
                        }))
                        .unwrap();
                }
            });
    }

    fn update_diagnostics(&mut self) {
        let db = self.analysis_host.raw_database();
        let subscriptions = self
            .mem_docs
            .iter()
            .map(|path| self.vfs.read().0.file_id(path).unwrap())
            .filter(|&file_id| {
                let source_root = db.file_source_root(file_id);
                // Only publish diagnostics for files in the workspace, not from crates.io deps
                // or the sysroot.
                // While theoretically these should never have errors, we have quite a few false
                // positives particularly in the stdlib, and those diagnostics would stay around
                // forever if we emitted them here.
                !db.source_root(source_root).is_library
            })
            .collect::<Vec<_>>();
        tracing::trace!("updating notifications for {:?}", subscriptions);

        // Diagnostics are triggered by the user typing
        // so we run them on a latency sensitive thread.
        self.task_pool
            .handle
            .spawn(ThreadIntent::LatencySensitive, {
                let snapshot = self.snapshot();
                move || Task::Diagnostics(fetch_native_diagnostics(snapshot, subscriptions))
            });
    }

    fn update_status_or_notify(&mut self) {
        let status = self.current_status();
        if self.last_reported_status.as_ref() != Some(&status) {
            self.last_reported_status = Some(status.clone());

            if self.config.server_status_notification() {
                self.send_notification::<lsp_ext::ServerStatusNotification>(status);
            } else if let (
                health @ (lsp_ext::Health::Warning | lsp_ext::Health::Error),
                Some(message),
            ) = (status.health, &status.message)
            {
                let open_log_button = tracing::enabled!(tracing::Level::ERROR)
                    && (self.fetch_build_data_error().is_err()
                        || self.fetch_workspace_error().is_err());
                self.show_message(
                    match health {
                        lsp_ext::Health::Ok => lsp_types::MessageType::INFO,
                        lsp_ext::Health::Warning => lsp_types::MessageType::WARNING,
                        lsp_ext::Health::Error => lsp_types::MessageType::ERROR,
                    },
                    message.clone(),
                    open_log_button,
                );
            }
        }
    }

    fn handle_task(&mut self, prime_caches_progress: &mut Vec<PrimeCachesProgress>, task: Task) {
        match task {
            Task::Response(response) => self.respond(response),
            // Only retry requests that haven't been cancelled. Otherwise we do unnecessary work.
            Task::Retry(req) if !self.is_completed(&req) => self.on_request(req),
            Task::Retry(_) => (),
            Task::Diagnostics(diagnostics_per_file) => {
                for (file_id, diagnostics) in diagnostics_per_file {
                    self.diagnostics
                        .set_native_diagnostics(file_id, diagnostics)
                }
            }
            Task::PrimeCaches(progress) => match progress {
                PrimeCachesProgress::Begin => prime_caches_progress.push(progress),
                PrimeCachesProgress::Report(_) => {
                    match prime_caches_progress.last_mut() {
                        Some(last @ PrimeCachesProgress::Report(_)) => {
                            // Coalesce subsequent update events.
                            *last = progress;
                        }
                        _ => prime_caches_progress.push(progress),
                    }
                }
                PrimeCachesProgress::End { .. } => prime_caches_progress.push(progress),
            },
            Task::FetchWorkspace(progress) => {
                let (state, msg) = match progress {
                    ProjectWorkspaceProgress::Begin => (Progress::Begin, None),
                    ProjectWorkspaceProgress::Report(msg) => (Progress::Report, Some(msg)),
                    ProjectWorkspaceProgress::End(workspaces, force_reload_crate_graph) => {
                        self.fetch_workspaces_queue
                            .op_completed(Some((workspaces, force_reload_crate_graph)));
                        if let Err(e) = self.fetch_workspace_error() {
                            tracing::error!("FetchWorkspaceError:\n{e}");
                        }

                        let old = Arc::clone(&self.workspaces);
                        self.switch_workspaces("fetched workspace".to_string());
                        let workspaces_updated = !Arc::ptr_eq(&old, &self.workspaces);

                        if self.config.run_build_scripts() && workspaces_updated {
                            self.fetch_build_data_queue
                                .request_op(format!("workspace updated"), ());
                        }

                        (Progress::End, None)
                    }
                };

                self.report_progress("Fetching", state, msg, None, None);
            }
            Task::FetchBuildData(progress) => {
                let (state, msg) = match progress {
                    BuildDataProgress::Begin => (Some(Progress::Begin), None),
                    BuildDataProgress::Report(msg) => (Some(Progress::Report), Some(msg)),
                    BuildDataProgress::End(build_data_result) => {
                        self.fetch_build_data_queue.op_completed(build_data_result);
                        if let Err(e) = self.fetch_build_data_error() {
                            tracing::error!("FetchBuildDataError:\n{e}");
                        }

                        self.switch_workspaces("fetched build data".to_string());
                        self.send_hint_refresh_query = true;

                        (Some(Progress::End), None)
                    }
                };

                if let Some(state) = state {
                    self.report_progress("Building", state, msg, None, None);
                }
            }
            Task::LoadProcMacros(progress) => {
                let (state, msg) = match progress {
                    ProcMacroProgress::Begin => (Some(Progress::Begin), None),
                    ProcMacroProgress::Report(msg) => (Some(Progress::Report), Some(msg)),
                    ProcMacroProgress::End(proc_macro_load_result) => {
                        self.fetch_proc_macros_queue.op_completed(true);
                        self.set_proc_macros(proc_macro_load_result);
                        self.send_hint_refresh_query = true;
                        (Some(Progress::End), None)
                    }
                };

                if let Some(state) = state {
                    self.report_progress("Loading", state, msg, None, None);
                }
            }
        }
    }

    fn handle_vfs_msg(&mut self, message: vfs::loader::Message) {
        let is_changed = matches!(message, vfs::loader::Message::Changed { .. });
        match message {
            vfs::loader::Message::Changed { files } | vfs::loader::Message::Loaded { files } => {
                let vfs = &mut self.vfs.write().0;
                for (path, contents) in files {
                    let path = VfsPath::from(path);
                    // if the file is in mem docs, it's managed by the client via notifications
                    // so only set it if its not in there
                    if !self.mem_docs.contains(&path) {
                        if is_changed || vfs.file_id(&path).is_none() {
                            vfs.set_file_contents(path, contents);
                        }
                    }
                }
            }
            vfs::loader::Message::Progress {
                n_total,
                n_done,
                config_version,
            } => {
                always!(config_version <= self.vfs_config_version);

                self.vfs_progress_config_version = config_version;
                self.vfs_progress_n_total = n_total;
                self.vfs_progress_n_done = n_done;

                let state = if n_done == 0 {
                    Progress::Begin
                } else if n_done < n_total {
                    Progress::Report
                } else {
                    assert_eq!(n_done, n_total);
                    Progress::End
                };
                self.report_progress(
                    "Roots Scanned",
                    state,
                    Some(format!("{n_done}/{n_total}")),
                    Some(Progress::fraction(n_done, n_total)),
                    None,
                );
            }
        }
    }

    /// Registers and handles a request. This should only be called once per incoming request.
    fn on_new_request(&mut self, request_received: Instant, req: Request) {
        self.register_request(&req, request_received);
        self.on_request(req);
    }

    /// Handles a request.
    fn on_request(&mut self, req: Request) {
        let mut dispatcher = RequestDispatcher {
            req: Some(req),
            global_state: self,
        };
        dispatcher.on_sync_mut::<lsp_types::request::Shutdown>(|s, ()| {
            s.shutdown_requested = true;
            Ok(())
        });

        match &mut dispatcher {
            RequestDispatcher {
                req: Some(req),
                global_state: this,
            } if this.shutdown_requested => {
                this.respond(lsp_server::Response::new_err(
                    req.id.clone(),
                    lsp_server::ErrorCode::InvalidRequest as i32,
                    "Shutdown already requested.".to_owned(),
                ));
                return;
            }
            _ => (),
        }

        use crate::handlers::request as handlers;
        use lsp_types::request as lsp_request;

        // TODO put request handlers here
        dispatcher.finish();
    }

    /// Handles an incoming notification.
    fn on_notification(&mut self, not: Notification) -> anyhow::Result<()> {
        use crate::handlers::notification as handlers;
        use lsp_types::notification as notifs;

        NotificationDispatcher {
            not: Some(not),
            global_state: self,
        }
        .on_sync_mut::<notifs::Cancel>(handlers::handle_cancel)?
        .on_sync_mut::<notifs::WorkDoneProgressCancel>(handlers::handle_work_done_progress_cancel)?
        .on_sync_mut::<notifs::DidOpenTextDocument>(handlers::handle_did_open_text_document)?
        .on_sync_mut::<notifs::DidChangeTextDocument>(handlers::handle_did_change_text_document)?
        .on_sync_mut::<notifs::DidCloseTextDocument>(handlers::handle_did_close_text_document)?
        .on_sync_mut::<notifs::DidSaveTextDocument>(handlers::handle_did_save_text_document)?
        .on_sync_mut::<notifs::DidChangeConfiguration>(handlers::handle_did_change_configuration)?
        .on_sync_mut::<notifs::DidChangeWatchedFiles>(handlers::handle_did_change_watched_files)?
        .finish();
        Ok(())
    }
}
