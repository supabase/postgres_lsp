//! Analysis-level representation of file-system paths.
//!
//! The primary goal of this is to losslessly represent paths like
//!
//! ```
//! \include_relative ./file.sql
//! ```
//!
//! The first approach one might reach for is to use `PathBuf`. The problem here
//! is that `PathBuf` depends on host target (windows or linux), but
//! postgres_lsp should be capable to process `\include_relative C:\file.sql` on Unix.
//!
//! The second try is to use a `String`. This also fails, however. Consider a
//! hypothetical scenario, where postgres_lsp operates in a
//! networked/distributed mode. There's one global instance of rust-analyzer,
//! which processes requests from different machines. Now, the semantics of
//! `\include_relative ./file.sql` actually depends on which file-system we are at!
//! That is, even absolute paths exist relative to a file system!
//!
//! A more realistic scenario here is virtual VFS paths we use for testing. More
//! generally, there can be separate "universes" of VFS paths.
//!
//! That's why we use anchored representation -- each path carries an info about
//! a file this path originates from. We can fetch fs/"universe" information
//! from the anchor than.
use crate::FileId;

/// Path relative to a file.
///
/// Owned version of [`AnchoredPath`].
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AnchoredPathBuf {
    /// File that this path is relative to.
    pub anchor: FileId,
    /// Path relative to `anchor`'s containing directory.
    pub path: String,
}

/// Path relative to a file.
///
/// Borrowed version of [`AnchoredPathBuf`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AnchoredPath<'a> {
    /// File that this path is relative to.
    pub anchor: FileId,
    /// Path relative to `anchor`'s containing directory.
    pub path: &'a str,
}
