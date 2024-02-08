mod caps;
mod dispatch;
mod line_index;
mod main_loop;
mod op_queue;

pub mod config;
pub mod lsp;

use serde::de::DeserializeOwned;

pub use crate::{caps::server_capabilities, main_loop::main_loop};

pub fn from_json<T: DeserializeOwned>(
    what: &'static str,
    json: &serde_json::Value,
) -> anyhow::Result<T> {
    serde_json::from_value(json.clone())
        .map_err(|e| anyhow::format_err!("Failed to deserialize {what}: {e}; {json}"))
}
