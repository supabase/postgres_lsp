use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
/// Generic errors thrown during biome operations
pub enum WorkspaceError {
    /// The file does not exist in the [crate::Workspace]
    NotFound(NotFound),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NotFound;
