use schema_cache::SchemaCache;
use std::{sync::Arc, time::Instant};

use crossbeam_channel::{unbounded, Receiver, Sender};

use crate::{config::Config, main_loop::Task, task_pool::TaskPool};

// Enforces drop order
pub(crate) struct Handle<H, C> {
    pub(crate) handle: H,
    pub(crate) receiver: C,
}

pub(crate) type ReqHandler = fn(&mut GlobalState, lsp_server::Response);
type ReqQueue = lsp_server::ReqQueue<(String, Instant), ReqHandler>;

pub(crate) struct GlobalState {
    sender: Sender<lsp_server::Message>,
    config: Arc<Config>,
    // add listener for schema cache
    schema_cache: SchemaCache,

    req_queue: ReqQueue,
    pub(crate) task_pool: Handle<TaskPool<Task>, Receiver<Task>>,

    // status
    pub(crate) shutdown_requested: bool,
}

impl GlobalState {
    pub(crate) fn new(sender: Sender<lsp_server::Message>, config: Config) -> GlobalState {
        let task_pool = {
            let (sender, receiver) = unbounded();
            let handle = TaskPool::new_with_threads(sender, config.main_loop_num_threads());
            Handle { handle, receiver }
        };
        GlobalState {
            sender,
            config: Arc::new(config.clone()),
            schema_cache: SchemaCache::default(),

            req_queue: ReqQueue::default(),
            task_pool,

            shutdown_requested: false,
        }
    }

    pub(crate) fn send_request<R: lsp_types::request::Request>(
        &mut self,
        params: R::Params,
        handler: ReqHandler,
    ) {
        let request = self
            .req_queue
            .outgoing
            .register(R::METHOD.to_owned(), params, handler);
        self.send(request.into());
    }

    pub(crate) fn complete_request(&mut self, response: lsp_server::Response) {
        let handler = self
            .req_queue
            .outgoing
            .complete(response.id.clone())
            .expect("received response for unknown request");
        handler(self, response)
    }

    pub(crate) fn send_notification<N: lsp_types::notification::Notification>(
        &self,
        params: N::Params,
    ) {
        let not = lsp_server::Notification::new(N::METHOD.to_owned(), params);
        self.send(not.into());
    }

    pub(crate) fn register_request(
        &mut self,
        request: &lsp_server::Request,
        request_received: Instant,
    ) {
        self.req_queue.incoming.register(
            request.id.clone(),
            (request.method.clone(), request_received),
        );
    }

    pub(crate) fn respond(&mut self, response: lsp_server::Response) {
        if self
            .req_queue
            .incoming
            .complete(response.id.clone())
            .is_some()
        {
            self.send(response.into());
        }
    }

    pub(crate) fn cancel(&mut self, request_id: lsp_server::RequestId) {
        if let Some(response) = self.req_queue.incoming.cancel(request_id) {
            self.send(response.into());
        }
    }

    pub(crate) fn is_completed(&self, request: &lsp_server::Request) -> bool {
        self.req_queue.incoming.is_completed(&request.id)
    }

    fn send(&self, message: lsp_server::Message) {
        self.sender.send(message).unwrap()
    }
}
