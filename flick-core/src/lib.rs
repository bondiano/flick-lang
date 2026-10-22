//! Shared Rust core for the Flick compiler and LSP.
//!
//! Source of truth: `Flick спецификация ML языка с HKT и Effect.md` in the
//! repository root. Implemented so far: diagnostics, syntax and `fmt`.

pub mod bootstrap;
pub mod consteval;
pub mod diagnostics;
pub mod fmt;
pub mod source;
pub mod syntax;

/// Version of the Flick language this core targets (spec header: "Flick 0.7").
pub const LANGUAGE_VERSION: &str = "0.7";

/// Checks one source file: currently lexical and syntax diagnostics; the
/// later phases of the pipeline (§11) add theirs here.
pub fn check(sources: &source::SourceMap, file: source::FileId) -> Vec<diagnostics::Diagnostic> {
    syntax::parse(&sources.get(file).text, file).diagnostics
}
