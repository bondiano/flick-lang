//! Shared Rust core for the Flick compiler and LSP.
//!
//! Source of truth: `Flick спецификация ML языка с HKT и Effect.md` in the
//! repository root. The language is specified but not yet implemented.

pub mod bootstrap;
pub mod diagnostics;
pub mod source;

/// Version of the Flick language this core targets (spec header: "Flick 0.7").
pub const LANGUAGE_VERSION: &str = "0.7";

/// Checks one source file. Today this is the bootstrap front-end; the real
/// pipeline (§11) replaces it phase by phase.
pub fn check(sources: &source::SourceMap, file: source::FileId) -> Vec<diagnostics::Diagnostic> {
    bootstrap::compile(sources, file).err().unwrap_or_default()
}
