//! Entry runners of the native JavaScript build.

use crate::codegen;
use crate::diagnostics::{Diagnostic, Severity, codes};
use crate::iface::Namespace;
use crate::project::config::Backend;
use crate::project::{Compiled, MainKind, main_kind};
use crate::source::Span;

use super::super::{Artifact, Settings, runner_file, schema_aot_file};
use flick_backend_js::codegen::runner::{Start, runner};

/// Shared frontend classification for a generated entry launcher.
#[derive(Debug, Clone)]
pub struct EntryFacts {
    pub name: String,
    pub defining: String,
    pub pure: bool,
}

/// Validate entry types once in the frontend, including standalone files.
pub fn entry_facts(
    settings: &Settings,
    modules: &[Compiled],
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<EntryFacts> {
    let mut entries = Vec::new();
    for (name, entry) in settings.entries {
        let started_by = settings
            .runners
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, r)| r.as_str());
        let module = modules.iter().find(|m| &m.name == entry);
        let main = module.and_then(|m| m.interface.find(Namespace::Value, "main"));
        // a re-exported `main` is imported from the module that defines it, whose output has it
        let defining = main.map_or(entry.as_str(), |e| e.entity.module.as_str());
        // a project checked its entry already, against its runner if it has one;
        // a file built alone is checked here
        let kind = main
            .filter(|_| started_by.is_none())
            .and_then(|e| Some(main_kind(e.scheme.as_ref()?)));
        match kind {
            Some(Err(message)) => {
                let diagnostic = Diagnostic::new(codes::P026, Severity::Error, message);
                let file = modules.iter().find(|m| m.name == defining).map(|m| m.file);
                let at = file.zip(main.and_then(|e| e.at));
                diagnostics.push(match at {
                    Some((f, [start, end])) => diagnostic.with_span(Span::new(f, start, end)),
                    None => diagnostic,
                });
            }
            kind => entries.push(EntryFacts {
                name: name.clone(),
                defining: defining.to_owned(),
                pure: matches!(kind, Some(Ok(MainKind::Pure))),
            }),
        }
    }
    entries
}

/// The runner of each entry, using the shared frontend classification.
pub(super) fn entry_runners(
    settings: &Settings,
    modules: &[Compiled],
    path: &dyn Fn(&str) -> String,
    out: &mut Vec<Artifact>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for entry in entry_facts(settings, modules, diagnostics) {
        let started_by = settings
            .runners
            .iter()
            .find(|(name, _)| *name == entry.name)
            .map(|(_, runner)| runner.as_str());
        let file = runner_file(&entry.name, settings.typescript);
        let aot = settings
            .extras
            .schema_aot
            .then(|| schema_aot_file(&entry.name));
        let start = match started_by {
            Some(module) => Start::Run(codegen::relative(&file, &path(module))),
            None if entry.pure => Start::Call,
            None if settings.backend == Backend::Rt => Start::RunMainRt,
            None => Start::RunMain,
        };
        let contents = runner(
            &codegen::relative(&file, &path(&entry.defining)),
            &start,
            aot.as_deref(),
        );
        out.push(Artifact {
            path: file,
            contents,
        });
    }
}
