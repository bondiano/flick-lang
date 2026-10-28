//! Checking the loaded modules: each after the modules it imports, those
//! of one level of the import graph in parallel, reusing what a
//! [`ModuleCache`] kept of earlier runs.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;

use super::{Analyzed, Loaded, Loader};
use crate::diagnostics::{Diagnostic, Severity};
use crate::ir;
use crate::project::Project;
use crate::project::config::Backend;
use crate::resolve::{self, Interface};
use crate::source::{FileId, SourceMap};
use crate::types;

/// The output of checking each module in earlier runs, reused while the
/// module's text, the interfaces of its imports and the configuration stay
/// the same.
#[derive(Default)]
pub struct ModuleCache {
    entries: HashMap<PathBuf, (Key, Output)>,
}

impl ModuleCache {
    /// Drops the modules a run no longer loaded.
    pub(super) fn retain_loaded(&mut self, modules: &[Loaded]) {
        self.entries
            .retain(|path, _| modules.iter().any(|m| m.path == *path));
    }
}

#[derive(PartialEq)]
struct Key {
    file: FileId,
    text: String,
    /// The text of the project's configuration.
    config: Option<String>,
    lower: bool,
    /// Each import's interface, without where its names are declared, which
    /// checking does not read, and whether it is a module of the project.
    imports: Vec<(Option<Interface>, bool)>,
}

impl Loader<'_> {
    /// Resolves every module after the modules it imports; an import that
    /// closes a cycle sees no interface. Without cycles, the modules of one
    /// level of the import graph are checked in parallel.
    pub(super) fn resolve_all(&mut self, edges: &[Vec<usize>]) -> Vec<Option<Interface>> {
        let mut interfaces: Vec<Option<Interface>> = vec![None; self.modules.len()];
        let Some(levels) = levels(edges) else {
            let mut visiting = vec![false; self.modules.len()];
            for i in 0..self.modules.len() {
                self.resolve_one(i, edges, &mut interfaces, &mut visiting);
            }
            return interfaces;
        };
        for level in levels {
            let checking = self.checking();
            let outputs = parallel(&level, |i| checking.module(i, &interfaces));
            for (i, output) in level.into_iter().zip(outputs) {
                self.apply(i, output, &mut interfaces);
            }
        }
        interfaces
    }

    fn resolve_one(
        &mut self,
        i: usize,
        edges: &[Vec<usize>],
        interfaces: &mut Vec<Option<Interface>>,
        visiting: &mut Vec<bool>,
    ) {
        if interfaces[i].is_some() || visiting[i] {
            return;
        }
        visiting[i] = true;
        for &j in &edges[i] {
            self.resolve_one(j, edges, interfaces, visiting);
        }
        let output = self.checking().module(i, interfaces);
        self.apply(i, output, interfaces);
        visiting[i] = false;
    }

    fn checking(&self) -> Checking<'_> {
        Checking {
            modules: &self.modules,
            sources: self.sources,
            by_name: &self.by_name,
            from_packages: &self.from_packages,
            project: self.project,
            lower: self.lowered.is_some(),
            cache: self.cache.as_deref(),
        }
    }

    fn apply(
        &mut self,
        i: usize,
        (output, key): (Output, Option<Key>),
        interfaces: &mut [Option<Interface>],
    ) {
        if let (Some(cache), Some(key)) = (&mut self.cache, key) {
            cache
                .entries
                .insert(self.modules[i].path.clone(), (key, output.clone()));
        }
        self.diagnostics.extend(output.diagnostics);
        if let (Some(lowered), Some(ir)) = (&mut self.lowered, output.ir) {
            lowered[i] = Some(ir);
        }
        if let Some(ide) = &mut self.ide {
            ide.push(Analyzed::new(
                &self.modules[i],
                output.typing,
                output.interface,
            ));
        }
        interfaces[i] = Some(output.exported);
    }
}

/// What checking a module reads of the loader; shared by parallel checks.
struct Checking<'a> {
    modules: &'a [Loaded],
    sources: &'a SourceMap,
    by_name: &'a BTreeMap<String, usize>,
    from_packages: &'a BTreeMap<String, Interface>,
    project: Option<&'a Project>,
    lower: bool,
    cache: Option<&'a ModuleCache>,
}

impl Checking<'_> {
    /// Resolves, checks and, when asked and clean, lowers module `i` against
    /// the interfaces known so far, or reuses the cached output; with a
    /// cache, also the key to cache it by.
    fn module(&self, i: usize, interfaces: &[Option<Interface>]) -> (Output, Option<Key>) {
        let loaded = &self.modules[i];
        let lookup = |name: &str| match self.by_name.get(name) {
            Some(&j) => interfaces[j].clone(),
            None => self.from_packages.get(name).cloned(),
        };
        let Some(cache) = self.cache else {
            return (self.check(loaded, &lookup), None);
        };
        let key = Key {
            file: loaded.file,
            text: self.sources.get(loaded.file).text.clone(),
            config: self
                .project
                .map(|p| self.sources.get(p.config_file).text.clone()),
            lower: self.lower,
            imports: loaded
                .syntax
                .imports
                .iter()
                .map(|i| {
                    let interface = lookup(&i.module).map(|mut interface| {
                        for export in &mut interface.exports {
                            export.at = None;
                        }
                        interface
                    });
                    (interface, self.by_name.contains_key(&i.module))
                })
                .collect(),
        };
        match cache.entries.get(&loaded.path) {
            Some((cached, output)) if *cached == key => {
                let mut output = output.clone();
                // re-exported names are declared where their module now says
                for import in &loaded.syntax.imports {
                    if let Some(interface) = lookup(&import.module) {
                        redeclare(&mut output.interface, &interface);
                        redeclare(&mut output.exported, &interface);
                    }
                }
                (output, Some(key))
            }
            _ => (self.check(loaded, &lookup), Some(key)),
        }
    }

    fn check(&self, loaded: &Loaded, lookup: &dyn Fn(&str) -> Option<Interface>) -> Output {
        let module = resolve::Module {
            name: &loaded.name,
            file: loaded.file,
            text: &self.sources.get(loaded.file).text,
            tree: &loaded.parse.tree,
            syntax: &loaded.syntax,
        };
        let (mut interface, resolved) = resolve::resolve(&module, lookup);
        let local = |name: &str| self.by_name.contains_key(name);
        let build = self.project.map(|p| &p.config.build);
        let options = types::Options {
            local: &local,
            limits: build.map_or_else(types::Limits::default, |b| types::Limits {
                steps: b.solver_steps,
                depth: b.solver_depth,
            }),
            sql: self
                .project
                .map_or_else(Default::default, |p| p.config.sql_dialect),
            no_let_assert: self.project.is_some_and(|p| p.config.no_let_assert),
            thresholds: self
                .project
                .map_or_else(Default::default, |p| p.config.thresholds),
            effects: build.is_none_or(|b| b.effect == Backend::Effect),
        };
        let mut diagnostics = loaded.parse.diagnostics.clone();
        let resolved_cleanly = !resolved.iter().any(|d| d.severity == Severity::Error);
        diagnostics.extend(resolved);
        // a module with syntax errors is checked for its other errors, but keeps
        // untyped exports: importers trust them
        let broken = !loaded.parse.diagnostics.is_empty();
        let untyped = broken.then(|| interface.clone());
        let (checked, typing) =
            types::check_typed(&module, lookup, types::builtins(), options, &mut interface);
        let checked = types::beside_syntax_errors(&loaded.parse.tree, checked);
        let clean =
            !broken && resolved_cleanly && !checked.iter().any(|d| d.severity == Severity::Error);
        diagnostics.extend(checked);
        let mut ir = None;
        if clean && self.lower {
            match ir::lower(&module, &typing) {
                Ok(lowered) => ir = Some(lowered),
                Err(d) => diagnostics.extend(d),
            }
        }
        Output {
            diagnostics,
            typing: Arc::new(typing),
            exported: untyped.unwrap_or_else(|| interface.clone()),
            interface,
            ir,
        }
    }
}

/// What checking one module produces.
#[derive(Clone)]
struct Output {
    diagnostics: Vec<Diagnostic>,
    typing: Arc<types::Typing>,
    /// The checked interface, which editors see.
    interface: Interface,
    /// What importers see: without types when the module has syntax errors.
    exported: Interface,
    ir: Option<ir::Module>,
}

/// Takes where `imported` declares the names `interface` re-exports.
fn redeclare(interface: &mut Interface, imported: &Interface) {
    for export in &mut interface.exports {
        if export.entity.module == interface.module {
            continue;
        }
        if let Some(found) = imported.exports.iter().find(|e| e.entity == export.entity) {
            export.at = found.at;
        }
    }
}

/// The modules by level of the import graph, each after every module it
/// imports; `None` when imports form a cycle.
pub(super) fn levels(edges: &[Vec<usize>]) -> Option<Vec<Vec<usize>>> {
    let mut waiting: Vec<usize> = edges.iter().map(Vec::len).collect();
    let mut importers: Vec<Vec<usize>> = vec![Vec::new(); edges.len()];
    for (i, targets) in edges.iter().enumerate() {
        for &j in targets {
            importers[j].push(i);
        }
    }
    let mut level: Vec<usize> = (0..edges.len()).filter(|&i| waiting[i] == 0).collect();
    let mut levels = Vec::new();
    let mut placed = 0;
    while !level.is_empty() {
        placed += level.len();
        let mut next = Vec::new();
        for &i in &level {
            for &k in &importers[i] {
                waiting[k] -= 1;
                if waiting[k] == 0 {
                    next.push(k);
                }
            }
        }
        next.sort_unstable();
        levels.push(level);
        level = next;
    }
    (placed == edges.len()).then_some(levels)
}

/// `f` of each item, in order, spread over the available cores.
#[cfg(not(target_arch = "wasm32"))]
fn parallel<T: Send>(items: &[usize], f: impl Fn(usize) -> T + Sync) -> Vec<T> {
    use std::panic;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;
    // the checker recurses deeply, like on the main thread
    const STACK: usize = 8 << 20;
    let workers = thread::available_parallelism()
        .map_or(1, usize::from)
        .min(items.len());
    if workers <= 1 {
        return items.iter().map(|&i| f(i)).collect();
    }
    let next = AtomicUsize::new(0);
    let mut results: Vec<(usize, T)> = thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                thread::Builder::new()
                    .stack_size(STACK)
                    .spawn_scoped(scope, || {
                        let mut done = Vec::new();
                        loop {
                            let k = next.fetch_add(1, Ordering::Relaxed);
                            let Some(&i) = items.get(k) else {
                                return done;
                            };
                            done.push((k, f(i)));
                        }
                    })
                    .expect("spawn a checker thread")
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap_or_else(|e| panic::resume_unwind(e)))
            .collect()
    });
    results.sort_unstable_by_key(|(k, _)| *k);
    results.into_iter().map(|(_, t)| t).collect()
}

#[cfg(target_arch = "wasm32")]
fn parallel<T>(items: &[usize], f: impl Fn(usize) -> T) -> Vec<T> {
    items.iter().map(|&i| f(i)).collect()
}
