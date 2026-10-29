//! JSON Schemas of command inputs (`alma schema`).

/// Schema of a batch of operations (`alma apply`), version 0.
pub const OPS_SCHEMA: &str = include_str!("ops.schema.json");
