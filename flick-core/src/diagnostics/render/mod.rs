//! Diagnostic renderers: human-readable (rustc style) and versioned NDJSON.

mod human;
mod json;

pub use human::render_human;
pub use json::{SCHEMA_VERSION, render_json};

#[cfg(test)]
mod tests;
