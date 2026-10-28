//! Diagnostic renderers: human-readable (rustc style) and versioned NDJSON.

#[cfg(feature = "terminal-diagnostics")]
mod human;
mod json;

#[cfg(feature = "terminal-diagnostics")]
pub use human::render_human;
pub use json::{Event, SCHEMA_VERSION, json_events, render_json};

#[cfg(test)]
mod tests;
