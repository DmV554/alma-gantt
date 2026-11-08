//! Lossless read/write of `*.alma.toml` plan files with `toml_edit` (SPEC.md §8,
//! `docs/format.md`).

pub mod canonical;
pub mod keys;
pub mod parse;
pub mod snapshot;

pub use canonical::{canonicalize, format_text, place_tables};
pub use parse::{FORMAT_VERSION, Parsed, parse};
