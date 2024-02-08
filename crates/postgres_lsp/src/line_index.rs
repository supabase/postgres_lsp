//! Enhances `ide::LineIndex` with additional info required to convert offsets
//! into lsp positions.
//!
//! We maintain invariant that all internal strings use `\n` as line separator.
//! This module does line ending conversion and detection (so that we can
//! convert back to `\r\n` on the way out).

#[derive(Clone, Copy)]
pub enum PositionEncoding {
    Utf8,
}
