use std::{fmt, time::Instant};

use crossbeam_channel::{select, Receiver};
use lsp_server::{Connection, Notification, Request};
use lsp_types::notification::Notification as _;

use crate::{
    config::Config,
    dispatch::{NotificationDispatcher, RequestDispatcher},
    global_state::GlobalState,
};

pub fn main_loop(config: Config, connection: Connection) -> anyhow::Result<()> {
    GlobalState::new(connection.sender, config).run(connection.receiver)
}

// feature params in latex: document, project and workspace!
// based on that they fetch all
// they parse with a rw lock on open. they open the entire doc on change!
//
// --> we can do the same. but do not name it workspace since we do not care about the workspace,
// just the sql files. and then apply changes and parse with ts and with libg_query. if no syntax
// error, compute cst.
// --> base db stores syntax tree
// also compute cst on save.
// --> computing cst could later be directly in sync.
//
// --> performance analysis for libg_query for large statements

enum Event {
    Lsp(lsp_server::Message),
    Task(Task),
}

impl fmt::Display for Event {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Event::Lsp(_) => write!(f, "Event::Lsp"),
            Event::Task(_) => write!(f, "Event::Task"),
        }
    }
}

#[derive(Debug)]
pub(crate) enum Task {
    Response(lsp_server::Response),
    Retry(lsp_server::Request),
    Diagnostics(Vec<(FileId, Vec<lsp_types::Diagnostic>)>),
}

impl GlobalState {
    fn run(mut self, inbox: Receiver<lsp_server::Message>) -> anyhow::Result<()> {
        // todo: reload schema cache

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

    fn next_event(&self, inbox: &Receiver<lsp_server::Message>) -> Option<Event> {
        select! {
            recv(inbox) -> msg =>
                msg.ok().map(Event::Lsp),

            recv(self.task_pool.receiver) -> task =>
                Some(Event::Task(task.unwrap())),
        }
    }

    fn handle_event(&mut self, event: Event) -> anyhow::Result<()> {
        let loop_start = Instant::now();

        match event {
            Event::Lsp(msg) => match msg {
                lsp_server::Message::Request(req) => self.on_new_request(loop_start, req),
                lsp_server::Message::Notification(not) => self.on_notification(not)?,
                lsp_server::Message::Response(resp) => self.complete_request(resp),
            },
            Event::Task(task) => {
                self.handle_task(task);
                // Coalesce multiple task events into one loop turn
                while let Ok(task) = self.task_pool.receiver.try_recv() {
                    self.handle_task(task);
                }
            }
        }

        Ok(())
    }

    fn handle_task(&mut self, task: Task) {
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
        .on_sync_mut::<notifs::DidOpenTextDocument>(handlers::handle_did_open_text_document)?
        .on_sync_mut::<notifs::DidChangeTextDocument>(handlers::handle_did_change_text_document)?
        .on_sync_mut::<notifs::DidCloseTextDocument>(handlers::handle_did_close_text_document)?
        .on_sync_mut::<notifs::DidSaveTextDocument>(handlers::handle_did_save_text_document)?
        .finish();
        Ok(())
    }
}
