//! This module is responsible to manage paths inside Biome.
//! It is a small wrapper around [path::PathBuf] but it is also able to
//! give additional information around the file that holds:
//! - the [FileHandlers] for the specific file
//! - shortcuts to open/write to the file
use std::fs::read_to_string;
use std::io::Read;
use std::{fs::File, io, io::Write, ops::Deref, path::PathBuf};

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FilePath {
    path: PathBuf,
}

impl Deref for FilePath {
    type Target = PathBuf;

    fn deref(&self) -> &Self::Target {
        &self.path
    }
}

impl FilePath {
    pub fn new(path_to_file: impl Into<PathBuf>) -> Self {
        Self {
            path: path_to_file.into(),
        }
    }
}
