//! Validate everything before the host executes a step or publishes a file.

mod graph;
mod path;

use std::collections::BTreeSet;
use std::io::{self, Write};
use std::str;

use crate::{BuildRequest, BuildResult, Diagnostic, Inputs, Severity, Toolchain, ToolchainStep};

/// Host limits, independent of a backend's claims about its capabilities.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub request_bytes: usize,
    pub result_bytes: usize,
    pub artifact_bytes: usize,
    pub files: usize,
    pub steps: usize,
    pub diagnostics: usize,
    pub string_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            request_bytes: 256 << 20,
            result_bytes: 128 << 20,
            artifact_bytes: 32 << 20,
            files: 20_000,
            steps: 1024,
            diagnostics: 4096,
            string_bytes: 16 << 10,
        }
    }
}

/// Actual project policy, not policy supplied by the guest. Only the pinned
/// tools and environment names listed here may be used by a toolchain step.
pub struct Policy<'a> {
    pub toolchains: &'a [Toolchain],
    pub environment: &'a [String],
    /// Outputs owned by the shared compiler, such as `.iface` and `.d.ts`.
    pub reserved_paths: &'a [String],
    /// Approve the command's argument grammar, declared file access and
    /// network policy in a host-owned tool adapter. A tool pin alone never
    /// authorizes arbitrary arguments.
    pub authorize_step: &'a dyn Fn(&ToolchainStep) -> Result<(), String>,
    pub limits: Limits,
}

/// A non-allocating upper bound on the complete serialized result, including
/// metadata, strings and diagnostics as well as artifact payloads.
pub(super) struct Budget(pub(super) usize);

impl Write for Budget {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 = self
            .0
            .checked_sub(bytes.len())
            .ok_or_else(|| io::Error::other("backend result exceeds its total output limit"))?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(super) fn string(s: &str, limits: Limits) -> Result<(), String> {
    if s.len() > limits.string_bytes || s.contains('\0') {
        return Err("backend string exceeds its limit or contains NUL".into());
    }
    Ok(())
}

pub(super) fn id<'a>(
    value: &'a str,
    ids: &mut BTreeSet<&'a str>,
    limits: Limits,
) -> Result<(), String> {
    string(value, limits)?;
    if value.is_empty() || !ids.insert(value) {
        return Err(format!("empty or duplicate backend id `{value}`"));
    }
    Ok(())
}

pub(super) fn diagnostics(
    request: &BuildRequest,
    diagnostics: &[Diagnostic],
    inputs: &mut dyn Inputs,
    limits: Limits,
) -> Result<(), String> {
    ciborium::into_writer(diagnostics, Budget(limits.result_bytes))
        .map_err(|_| "backend diagnostics exceed their total output limit")?;
    if diagnostics.len() > limits.diagnostics {
        return Err("backend diagnostic count exceeds its limit".into());
    }
    for diagnostic in diagnostics {
        string(&diagnostic.code, limits)?;
        string(&diagnostic.message, limits)?;
        for note in &diagnostic.notes {
            string(note, limits)?;
        }
        if let Some(span) = diagnostic.span {
            let source = request
                .sources
                .get(span.file as usize)
                .ok_or("backend diagnostic references an unknown source file")?;
            let bytes = inputs
                .read(&source.input)
                .map_err(|_| "backend diagnostic source is unavailable")?;
            let text =
                str::from_utf8(&bytes).map_err(|_| "backend diagnostic source is not UTF-8")?;
            if span.start > span.end
                || !text.is_char_boundary(span.start as usize)
                || !text.is_char_boundary(span.end as usize)
            {
                return Err("backend diagnostic span is outside its source or splits UTF-8".into());
            }
        }
    }
    Ok(())
}

/// Return the DAG's execution order. A failure rejects the entire result;
/// callers must not execute steps or publish individual files before this.
pub fn validate_result(
    request: &BuildRequest,
    result: &BuildResult,
    policy: &Policy,
    inputs: &mut dyn Inputs,
) -> Result<Vec<usize>, String> {
    let limits = policy.limits;
    ciborium::into_writer(result, Budget(limits.result_bytes))
        .map_err(|_| "backend result exceeds its total output limit")?;
    diagnostics(request, &result.diagnostics, inputs, limits)?;
    if result
        .diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error)
    {
        return Err("backend generation returned error diagnostics".into());
    }
    graph::validate(request, result, policy)
}
