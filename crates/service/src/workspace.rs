use std::panic::RefUnwindSafe;

use fs::FilePath;
use text_size::{TextRange, TextSize};

use crate::diagnostics::WorkspaceError;

mod server;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct OpenFileParams {
    pub path: FilePath,
    pub content: String,
    pub version: i32,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CloseFileParams {
    pub path: FilePath,
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct FileChange {
    /// The range of the file that changed. If `None`, the whole file changed.
    pub range: Option<TextRange>,
    pub text: String,
}

impl FileChange {
    pub fn diff_size(&self) -> TextSize {
        match self.range {
            Some(range) => {
                let range_length: usize = range.len().into();
                let text_length = self.text.chars().count();
                let diff = (text_length as i64 - range_length as i64).abs();
                TextSize::from(u32::try_from(diff).unwrap())
            }
            None => TextSize::from(u32::try_from(self.text.chars().count()).unwrap()),
        }
    }

    pub fn is_addition(&self) -> bool {
        self.range.is_some() && self.text.len() > self.range.unwrap().len().into()
    }

    pub fn is_deletion(&self) -> bool {
        self.range.is_some() && self.text.len() < self.range.unwrap().len().into()
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct FileChangesParams {
    pub path: FilePath,
    pub version: i32,
    pub changes: Vec<FileChange>,
}

pub trait Workspace: Send + Sync + RefUnwindSafe {
    fn open_file(&self, params: OpenFileParams) -> Result<(), WorkspaceError>;

    fn close_file(&self, params: CloseFileParams) -> Result<(), WorkspaceError>;

    fn apply_file_changes(&self, params: FileChangesParams) -> Result<(), WorkspaceError>;
}
