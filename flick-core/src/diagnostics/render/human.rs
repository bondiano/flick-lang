use annotate_snippets::{AnnotationKind, Group, Level, Patch, Renderer, Snippet};

use crate::diagnostics::{Diagnostic, Fix, Severity};
use crate::source::{FileId, SourceMap};

/// Renders diagnostics for a terminal. `color` is already resolved by the
/// caller (`--color auto` depends on the output stream, which the core cannot see).
pub fn render_human(sources: &SourceMap, diagnostics: &[Diagnostic], color: bool) -> String {
    let renderer = if color {
        Renderer::styled()
    } else {
        Renderer::plain()
    };
    diagnostics
        .iter()
        .map(|d| renderer.render(&report(sources, d)) + "\n")
        .collect::<Vec<_>>()
        .join("\n")
}

fn level(severity: Severity) -> Level<'static> {
    match severity {
        Severity::Error => Level::ERROR,
        Severity::Warning => Level::WARNING,
        Severity::Note => Level::NOTE,
    }
}

fn report<'a>(sources: &'a SourceMap, d: &'a Diagnostic) -> Vec<Group<'a>> {
    let title = level(d.severity)
        .primary_title(d.message.as_str())
        .id(d.code.to_string());
    let mut snippets: Vec<(FileId, Snippet<'a, _>)> = Vec::new();
    if let Some(span) = d.span {
        snippets.push((
            span.file,
            snippet(sources, span.file).annotation(AnnotationKind::Primary.span(span.range())),
        ));
    }
    for related in &d.related {
        let annotation = AnnotationKind::Context
            .span(related.span.range())
            .label(related.message.as_str());
        match snippets.iter_mut().find(|(f, _)| *f == related.span.file) {
            Some((_, s)) => *s = std::mem::replace(s, Snippet::source("")).annotation(annotation),
            None => snippets.push((
                related.span.file,
                snippet(sources, related.span.file).annotation(annotation),
            )),
        }
    }
    let main = title.elements(snippets.into_iter().map(|(_, s)| s));
    std::iter::once(main)
        .chain(d.fixes.iter().map(|fix| fix_group(sources, fix)))
        .collect()
}

fn fix_group<'a>(sources: &'a SourceMap, fix: &'a Fix) -> Group<'a> {
    let mut snippets: Vec<(FileId, Snippet<'a, Patch<'a>>)> = Vec::new();
    for edit in &fix.edits {
        let patch = Patch::new(edit.span.range(), edit.replacement.as_str());
        match snippets.iter_mut().find(|(f, _)| *f == edit.span.file) {
            Some((_, s)) => *s = std::mem::replace(s, Snippet::source("")).patch(patch),
            None => snippets.push((
                edit.span.file,
                snippet(sources, edit.span.file).patch(patch),
            )),
        }
    }
    let title = Level::HELP.secondary_title(fix.message.as_str());
    if snippets.is_empty() {
        Group::with_title(title)
    } else {
        title.elements(snippets.into_iter().map(|(_, s)| s))
    }
}

fn snippet<T: Clone>(sources: &SourceMap, file: FileId) -> Snippet<'_, T> {
    let file = sources.get(file);
    Snippet::source(file.text.as_str()).path(file.path.as_str())
}
