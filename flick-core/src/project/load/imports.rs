//! Following a module's imports: loading the modules they name, from the
//! project's roots and its source packages, and the cycles among them.

use std::path::PathBuf;

use super::{Loader, Root, module_path};
use crate::diagnostics::{Diagnostic, codes};
use crate::resolve;
use crate::source::Span;
use crate::types;

impl Loader<'_> {
    /// Loads imported modules until the set is closed; returns the local
    /// import edges of each module, sorted by module name.
    pub(super) fn load_imports(&mut self) -> Vec<Vec<usize>> {
        let mut edges = Vec::new();
        let mut i = 0;
        while i < self.modules.len() {
            let mut targets = Vec::new();
            for import in self.modules[i].syntax.imports.clone() {
                if let Some(target) = self.find_module(i, &import.module, import.span) {
                    targets.push(target);
                }
            }
            targets.sort_by(|a, b| self.modules[*a].name.cmp(&self.modules[*b].name));
            targets.dedup();
            edges.push(targets);
            i += 1;
        }
        edges
    }

    fn find_module(&mut self, importer: usize, name: &str, span: Span) -> Option<usize> {
        let packages = self.package_sources().into_iter().map(|(_, dir)| dir);
        // the project's own modules come before those of source packages
        let dirs: Vec<PathBuf> = match (&self.modules[importer].root, self.project) {
            (Root::Source, Some(p)) => [p.source_root()].into_iter().chain(packages).collect(),
            (Root::Test, Some(p)) => [p.source_root(), p.test_root()]
                .into_iter()
                .chain(packages)
                .collect(),
            (Root::Package(_), _) => packages.collect(),
            (Root::Other(dir), _) => dir.iter().cloned().collect(),
            _ => Vec::new(),
        };
        if !self.by_name.contains_key(name) {
            for dir in dirs {
                if self.use_saved(&dir, name) {
                    continue;
                }
                // absent files are simply not candidates
                let _ = self.load(&dir.join(module_path(name)));
            }
        }
        let local = self.by_name.get(name).copied();
        if local.is_none() && !self.from_packages.contains_key(name) {
            match types::builtins().iter().find(|i| i.module == name) {
                Some(builtin) => {
                    self.from_packages.insert(name.to_owned(), builtin.clone());
                }
                None => self.diagnostics.push(self.unknown_module(name, span)),
            }
        }
        local
    }

    pub(super) fn import_cycles(&mut self, edges: &[Vec<usize>]) {
        let mut order: Vec<usize> = (0..self.modules.len()).collect();
        order.sort_by(|a, b| self.modules[*a].name.cmp(&self.modules[*b].name));
        let mut reported = vec![false; self.modules.len()];
        for start in order {
            if reported[start] {
                continue;
            }
            let Some(cycle) = resolve::shortest_cycle(start, edges) else {
                continue;
            };
            for &i in &cycle {
                reported[i] = true;
            }
            let next = cycle.get(1).copied().unwrap_or(start);
            let chain: Vec<&str> = cycle
                .iter()
                .chain([&start])
                .map(|&i| self.modules[i].name.as_str())
                .collect();
            let module = &self.modules[start];
            let span = module
                .syntax
                .imports
                .iter()
                .find(|i| i.module == self.modules[next].name)
                .map(|i| i.span);
            self.diagnostics.push(
                Diagnostic::error(
                    codes::R006,
                    format!("import cycle: {}", chain.join(" -> ")),
                    span.expect("edge has an import"),
                )
                .with_fix(|| {
                    (
                        "move what the modules share into a module that both import",
                        Vec::new(),
                    )
                }),
            );
        }
    }
}
