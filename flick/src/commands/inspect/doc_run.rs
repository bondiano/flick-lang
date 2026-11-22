//! Checking and running package documentation examples through the shared
//! backend-neutral `run-flick` engine, remapping every diagnostic span back
//! into the original `///` comment in the `.fl` source file.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::doc::DocComment;
use flick_core::markdown::{self, Block, Mode};
use flick_core::plugin::expansion::Expansion;
use flick_core::project::{self, Compilation, Host as _, Project};
use flick_core::source::{FileId, SourceMap, Span};
use flick_core::syntax;
use flick_core::types::builtin_sources;

use super::Checked;
use crate::commands::run_flick::{self, PreparedTarget, RunStatus};

/// Location metadata linking a `///` documentation comment back to the `.fl`
/// source file that contains it.
pub(super) struct DocOrigin {
    /// Display label such as `Lib.Api` or `Lib.Api.answer`.
    pub(super) owner: String,
    /// Source file in `Checked::sources`.
    pub(super) file: FileId,
    /// Macro expansion if the doc comment was extracted from expanded source.
    pub(super) expansion: Option<Arc<Expansion>>,
    /// Extracted Markdown text and per-line byte ranges in the source file.
    pub(super) comment: DocComment,
}

/// Checks every ```` ```flick ```` block in `origin.comment`, executing
/// runnable `Mode::Check` examples through the selected backend.
pub(super) fn check_doc_examples(
    checked: &mut Checked,
    prepared: &PreparedTarget,
    origin: &DocOrigin,
) -> Vec<Diagnostic> {
    for block in markdown::blocks(&origin.comment.text) {
        let diags = check_block(checked, prepared, origin, &block);
        if diags.iter().any(|d| d.severity == Severity::Error) {
            return diags;
        }
    }
    Vec::new()
}

fn check_ignored_block(origin: &DocOrigin, block: &Block) -> Vec<Diagnostic> {
    let mut synthetic = SourceMap::default();
    let file = synthetic.add("documentation example", block.text.as_str());
    let parsed = syntax::parse(&block.text, file);
    parsed
        .diagnostics
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|mut d| {
            let mapped = d
                .span
                .and_then(|s| map_text_range(origin, block, 0, s.start as usize, s.end as usize))
                .or_else(|| block_span(origin, block));
            d.span = mapped;
            d.related.clear();
            d.fixes.clear();
            d.message = format!(
                "documentation example of `{}` (line {}) does not parse: {}",
                origin.owner, block.line, d.message
            );
            d
        })
        .collect()
}

fn check_fail_block(
    checked: &Checked,
    origin: &DocOrigin,
    block: &Block,
    errors: &[Diagnostic],
) -> Vec<Diagnostic> {
    let matched = !errors.is_empty()
        && block.codes.iter().all(|code| {
            errors.iter().any(|d| {
                d.code.to_string().trim_start_matches("FLICK-") == code.trim_start_matches("FLICK-")
            })
        });
    if matched {
        return Vec::new();
    }
    if let Some((package, start, end)) = undeclared_import(checked, &block.text) {
        return vec![undeclared_diagnostic(origin, block, &package, start, end)];
    }
    let detail = if errors.is_empty() {
        "expected a compiler error, but it compiled without errors".to_owned()
    } else {
        let found: Vec<String> = errors.iter().map(|d| d.code.to_string()).collect();
        format!(
            "expected error code(s) {}, got {}",
            block.codes.join(", "),
            found.join(", ")
        )
    };
    let mut diag = Diagnostic::new(
        codes::P062,
        Severity::Error,
        format!(
            "documentation example of `{}` (line {}) did not satisfy `fail`: {detail}",
            origin.owner, block.line
        ),
    );
    if let Some(span) = block_span(origin, block) {
        diag = diag.with_span(span);
    }
    vec![diag]
}

fn check_block(
    checked: &mut Checked,
    prepared: &PreparedTarget,
    origin: &DocOrigin,
    block: &Block,
) -> Vec<Diagnostic> {
    if block.mode == Mode::Ignore {
        return check_ignored_block(origin, block);
    }

    let has_module_header = block.text.lines().any(|line| line.starts_with("module "));
    let declared = block.text.lines().find_map(|line| {
        line.strip_prefix("module ")
            .and_then(|s| s.split_whitespace().next())
            .filter(|name| project::is_module_name(name))
    });
    let name = declared.unwrap_or("DocExample");
    let (overlay_source, prefix_len) = if has_module_header {
        (block.text.clone(), 0)
    } else {
        let prefix = format!("module {name} exposing (..)\n");
        let len = prefix.len();
        (format!("{prefix}{}", block.text), len)
    };

    let root = checked
        .project
        .as_ref()
        .map_or_else(|| checked.host.cwd.clone(), Project::source_root);
    let entry_path = project::normalize(&root.join(project::module_path(name)));

    let (mut compile_sources, diagnostics, compilation, overlay_paths) = run_flick::compile_overlay(
        &checked.host,
        checked.project.as_ref(),
        prepared.rt,
        name,
        &[(name.to_owned(), overlay_source.clone())],
    );
    let overlay_files: Vec<FileId> = overlay_paths
        .iter()
        .map(|p| checked.host.display(p))
        .flat_map(|displayed| {
            compile_sources
                .files()
                .enumerate()
                .filter(move |(_, f)| f.path == displayed)
                .map(|(i, _)| FileId::new(u32::try_from(i).expect("source count fits u32")))
        })
        .collect();

    let errors: Vec<Diagnostic> = diagnostics
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();

    if block.mode == Mode::Fail {
        return check_fail_block(checked, origin, block, &errors);
    }

    if !errors.is_empty() {
        // A missing dependency is what the reader must add, not the resolution
        // error that follows from it.
        if let Some((package, start, end)) = undeclared_import(checked, &block.text) {
            return vec![undeclared_diagnostic(origin, block, &package, start, end)];
        }
        return remap_or_merge_diagnostics(
            checked,
            &compile_sources,
            &overlay_files,
            origin,
            block,
            prefix_len,
            errors,
        );
    }

    // Only `Mode::Check` blocks with an exported `main` are executed; `keep`
    // blocks are compile-checked only.
    if block.mode != Mode::Check {
        return Vec::new();
    }
    let Some(module) = compilation.modules.iter().find(|m| m.path == entry_path) else {
        return Vec::new();
    };
    if !module.interface.exports_value("main") {
        return Vec::new();
    }

    execute_example(
        checked,
        prepared,
        &compilation,
        &mut compile_sources,
        &overlay_files,
        origin,
        block,
        &overlay_source,
        prefix_len,
        &entry_path,
        &module.name,
    )
}

#[allow(clippy::too_many_arguments)]
fn execute_example(
    checked: &mut Checked,
    prepared: &PreparedTarget,
    compilation: &Compilation,
    compile_sources: &mut SourceMap,
    overlay_files: &[FileId],
    origin: &DocOrigin,
    block: &Block,
    overlay_source: &str,
    prefix_len: usize,
    entry_path: &Path,
    entry_module: &str,
) -> Vec<Diagnostic> {
    let output = match run_flick::run_compiled(
        &checked.host,
        checked.project.as_ref(),
        prepared,
        compilation,
        compile_sources,
        entry_module,
        &[],
        #[cfg(feature = "wasm-backends")]
        None,
    ) {
        Ok(output) => output,
        Err(emit_diags) => {
            let emit_errors: Vec<Diagnostic> = emit_diags
                .into_iter()
                .filter(|d| d.severity == Severity::Error)
                .collect();
            return remap_or_merge_diagnostics(
                checked,
                compile_sources,
                overlay_files,
                origin,
                block,
                prefix_len,
                emit_errors,
            );
        }
    };

    match output.status {
        RunStatus::Success => Vec::new(),
        status => {
            let span = runtime_example_span(
                origin,
                block,
                overlay_source,
                prefix_len,
                entry_path,
                &output.stderr,
            );
            let subject = format!(
                "documentation example of `{}` (line {})",
                origin.owner, block.line
            );
            let message = run_flick::runtime_message(&subject, &status, &output.stderr);
            let mut diag = Diagnostic::new(codes::P061, Severity::Error, message);
            if let Some(span) = span {
                diag = diag.with_span(span);
            }
            vec![diag]
        }
    }
}

fn undeclared_diagnostic(
    origin: &DocOrigin,
    block: &Block,
    package: &str,
    start: usize,
    end: usize,
) -> Diagnostic {
    let mut diag = Diagnostic::new(
        codes::P062,
        Severity::Error,
        format!(
            "documentation example of `{}` (line {}) imports `{package}`, which is not a declared dependency",
            origin.owner, block.line
        ),
    );
    if let Some(span) =
        map_text_range(origin, block, 0, start, end).or_else(|| block_span(origin, block))
    {
        diag = diag.with_span(span);
    }
    diag
}

fn remap_or_merge_diagnostics(
    checked: &mut Checked,
    from_sources: &SourceMap,
    overlay_files: &[FileId],
    origin: &DocOrigin,
    block: &Block,
    prefix_len: usize,
    diagnostics: Vec<Diagnostic>,
) -> Vec<Diagnostic> {
    let mut remapped = Vec::with_capacity(diagnostics.len());
    let mut passthrough = Vec::new();
    for mut diag in diagnostics {
        let in_overlay = diag
            .span
            .is_some_and(|span| overlay_files.contains(&span.file));
        if in_overlay || diag.span.is_none() {
            let mapped = diag
                .span
                .and_then(|s| {
                    map_text_range(origin, block, prefix_len, s.start as usize, s.end as usize)
                })
                .or_else(|| block_span(origin, block));
            diag.related
                .retain(|r| !overlay_files.contains(&r.span.file));
            for fix in &mut diag.fixes {
                fix.edits.retain(|e| !overlay_files.contains(&e.span.file));
            }
            // Merge any remaining related spans that refer to real files in `from_sources`.
            if !diag.related.is_empty() || diag.fixes.iter().any(|f| !f.edits.is_empty()) {
                // Clear `span` before merging: `mapped` already carries a `checked.sources` FileId.
                diag.span = None;
                let mut merged =
                    run_flick::merge_diagnostics(&mut checked.sources, from_sources, vec![diag]);
                if let Some(mut d) = merged.pop() {
                    d.span = mapped;
                    d.message =
                        format!("documentation example of `{}`: {}", origin.owner, d.message);
                    remapped.push(d);
                }
            } else {
                diag.span = mapped;
                diag.message = format!(
                    "documentation example of `{}`: {}",
                    origin.owner, diag.message
                );
                remapped.push(diag);
            }
        } else {
            diag.message = format!(
                "documentation example of `{}`: {}",
                origin.owner, diag.message
            );
            passthrough.push(diag);
        }
    }
    if !passthrough.is_empty() {
        remapped.extend(run_flick::merge_diagnostics(
            &mut checked.sources,
            from_sources,
            passthrough,
        ));
    }
    remapped
}

/// Maps a byte range inside the compiled overlay text back through:
/// 1. synthetic `module DocExample exposing (..)\n` prefix,
/// 2. `markdown::Block::page_offset` (handling hidden `# ` lines),
/// 3. `DocComment::source_span` (mapping `///` lines back to `.fl` offsets),
/// 4. `Expansion::place` (mapping expanded offsets to original source).
fn map_text_range(
    origin: &DocOrigin,
    block: &Block,
    prefix_len: usize,
    compiled_start: usize,
    compiled_end: usize,
) -> Option<Span> {
    if compiled_end <= prefix_len && prefix_len > 0 {
        return block_span(origin, block);
    }
    let text_len = block.text.len();
    let start = compiled_start.saturating_sub(prefix_len).min(text_len);
    let mut end = compiled_end
        .saturating_sub(prefix_len)
        .min(text_len)
        .max(start);
    if end > start && block.text.as_bytes().get(end - 1) == Some(&b'\n') {
        end -= 1;
    }
    let page_start = u32::try_from(block.page_offset(start)).ok()?;
    let page_end = u32::try_from(block.page_offset(end)).ok()?;
    let (src_start, src_end) = origin.comment.source_span(page_start, page_end)?;
    let span = Span::new(origin.file, src_start, src_end);
    Some(origin.expansion.as_ref().map_or(span, |e| e.place(span)))
}

fn block_span(origin: &DocOrigin, block: &Block) -> Option<Span> {
    let range = block.range();
    let page_start = u32::try_from(range.start).ok()?;
    let page_end = u32::try_from(range.end).ok()?;
    let (src_start, src_end) = origin.comment.source_span(page_start, page_end)?;
    let span = Span::new(origin.file, src_start, src_end);
    Some(origin.expansion.as_ref().map_or(span, |e| e.place(span)))
}

fn runtime_example_span(
    origin: &DocOrigin,
    block: &Block,
    overlay_source: &str,
    prefix_len: usize,
    entry_path: &Path,
    stderr: &str,
) -> Option<Span> {
    if let Some(line_no) = run_flick::extract_line_from_stderr(stderr, entry_path)
        && let Some((start, end)) = line_byte_range(overlay_source, line_no)
        && let Some(span) = map_text_range(origin, block, prefix_len, start, end)
    {
        return Some(span);
    }
    if let Some((start, end)) = find_main_line(&block.text)
        && let Some(span) = map_text_range(origin, block, 0, start, end)
    {
        return Some(span);
    }
    block_span(origin, block)
}

fn line_byte_range(text: &str, line_no: usize) -> Option<(usize, usize)> {
    let mut offset = 0usize;
    for (idx, raw) in text.split_inclusive('\n').enumerate() {
        if idx + 1 == line_no {
            let trimmed = raw.trim_end_matches(['\r', '\n']);
            return Some((offset, offset + trimmed.len()));
        }
        offset += raw.len();
    }
    None
}

fn find_main_line(text: &str) -> Option<(usize, usize)> {
    let mut sig = None;
    let mut offset = 0usize;
    for raw in text.split_inclusive('\n') {
        let trimmed = raw.trim_end_matches(['\r', '\n']);
        let stripped = trimmed.trim_start();
        if stripped.starts_with("let main") {
            return Some((offset, offset + trimmed.len()));
        }
        if stripped.starts_with("main :") {
            sig.get_or_insert((offset, offset + trimmed.len()));
        }
        offset += raw.len();
    }
    sig
}

/// The first imported module root that the project and the standard library
/// do not provide and that no declared dependency provides from its source
/// directory, together with the byte range of that `import` line inside `text`.
fn undeclared_import(checked: &Checked, text: &str) -> Option<(String, usize, usize)> {
    checked.project.as_ref()?;
    let mut known: HashSet<&str> = checked
        .compilation
        .modules
        .iter()
        .filter_map(|m| m.name.split('.').next())
        .collect();
    for (name, _) in builtin_sources() {
        if let Some(root) = name.split('.').next() {
            known.insert(root);
        }
    }
    // A dependency provides the modules under its source directory: a
    // `Foo.Bar` import needs the directory `Foo` there.
    let libraries: Vec<&Path> = checked
        .compilation
        .libraries
        .iter()
        .map(|(_, dir)| dir.as_path())
        .collect();
    let mut offset = 0usize;
    for raw in text.split_inclusive('\n') {
        let line = raw.trim_end_matches(['\r', '\n']);
        if let Some(rest) = line.trim_start().strip_prefix("import ") {
            let root = rest
                .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                .next()
                .unwrap_or("");
            if !root.is_empty()
                && !known.contains(root)
                && !libraries.iter().any(|dir| dir.join(root).exists())
            {
                let leading = line.len() - line.trim_start().len();
                return Some((root.to_owned(), offset + leading, offset + line.len()));
            }
        }
        offset += raw.len();
    }
    None
}
