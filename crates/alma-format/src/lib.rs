//! Lossless read/write of `*.alma.toml` plan files with `toml_edit` (SPEC.md §8,
//! `docs/format.md`).

pub mod keys;
pub mod parse;

pub use parse::{FORMAT_VERSION, Parsed, parse};
