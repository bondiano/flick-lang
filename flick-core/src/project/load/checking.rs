//! Checking the loaded modules: each after the modules it imports, those
//! of one level of the import graph in parallel, reusing what a
//! [`ModuleCache`] kept of earlier runs.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
#[cfg(feature = "ide")]
use std::sync::Arc;

use tracing::debug;

#[cfg(feature = "ide")]
use super::Analyzed;
use super::precompiled::Precompiled;
use super::{Loaded, Loader, Root};
use crate::clock::timed;
use crate::diagnostics::{Diagnostic, Severity};
use crate::ir;
use crate::project::Project;
use crate::project::config::{Backend, Config};
use crate::resolve::{self, Interface};
use crate::source::{FileId, SourceMap};
use crate::types;

/// The output of checking each module in earlier runs, reused while the
/// module's text, the interfaces of its imports and the configuration stay
/// the same.
#[derive(Default)]
pub struct ModuleCache {
    entries: HashMap<PathBuf, (Key, Output)>,
    /// What plugins gave in the last run, by the key of each call.
    pub(super) plugins: HashMap<Vec<u8>, Vec<u8>>,
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
    /// The effective configuration, including programmatically supplied values.
    config: Option<Config>,
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
            // a failed import leaves names unresolved, and the build fails anyway
            lower: self.lowered.is_some()
                && !self
                    .diagnostics
                    .iter()
                    .any(|d| d.severity == Severity::Error),
            lower_packages_only: self.lower_packages_only,
            precompiled: &self.precompiled,
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
        #[cfg(feature = "ide")]
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
    lower_packages_only: bool,
    precompiled: &'a Precompiled,
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
            return (self.timed(loaded, &lookup), None);
        };
        let key = Key {
            file: loaded.file,
            // the code of its blocks too, which other modules' blocks change
            text: self.text_of(loaded).to_owned(),
            config: self.project.map(|p| p.config.clone()),
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
                debug!(module = %loaded.name, "module reused");
                (output, Some(key))
            }
            _ => (self.timed(loaded, &lookup), Some(key)),
        }
    }

    /// [`Checking::check`], logging how long it took.
    fn timed(&self, loaded: &Loaded, lookup: &dyn Fn(&str) -> Option<Interface>) -> Output {
        let (output, elapsed) = timed(|| self.check(loaded, lookup));
        debug!(module = %loaded.name, ?elapsed, "module checked");
        output
    }

    /// The text checking reads: the module's own, followed by the code of
    /// its plugin blocks.
    fn text_of<'l>(&'l self, loaded: &'l Loaded) -> &'l str {
        loaded
            .expansion
            .as_ref()
            .map_or(&self.sources.get(loaded.file).text, |e| &e.text)
    }

    fn check(&self, loaded: &Loaded, lookup: &dyn Fn(&str) -> Option<Interface>) -> Output {
        let tree = loaded
            .expansion
            .as_ref()
            .map_or(&loaded.parse.tree, |e| &e.tree);
        let module = resolve::Module {
            name: &loaded.name,
            file: loaded.file,
            text: self.text_of(loaded),
            tree,
            syntax: &loaded.syntax,
        };
        let (mut interface, resolved) = resolve::resolve(&module, lookup);
        // a library's modules are another package's: they own their own instances
        let local = |name: &str| match self.by_name.get(name) {
            Some(&i) => package_of(&self.modules[i].root) == package_of(&loaded.root),
            // a saved module of the same package is as local as its source
            None => self
                .precompiled
                .package_of(name)
                .is_some_and(|p| package_of(&loaded.root) == Some(p)),
        };
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
        // Keep the resolved exports before checking: a failed module may leave
        // store-local type variables in its inferred schemes. Importers must
        // not read those variables with their own unification store.
        let broken = !loaded.parse.diagnostics.is_empty();
        let mut untyped = interface.clone();
        let (checked, typing) =
            types::check_typed(&module, lookup, types::builtins(), options, &mut interface);
        let checked = types::beside_syntax_errors(tree, checked);
        let clean =
            !broken && resolved_cleanly && !checked.iter().any(|d| d.severity == Severity::Error);
        diagnostics.extend(checked);
        let mut ir = None;
        let lowers = !self.lower_packages_only || matches!(loaded.root, Root::Package(_));
        if clean && self.lower && lowers {
            match ir::lower(&module, &typing) {
                Ok(lowered) => ir = Some(lowered),
                Err(d) => diagnostics.extend(d),
            }
        }
        if let Some(expansion) = &loaded.expansion {
            expansion.restore(&mut diagnostics, ir.as_mut());
            expansion.place_interface(&mut interface);
            expansion.place_interface(&mut untyped);
        }
        // editors find and rename names of the module's own text only
        #[cfg(feature = "ide")]
        let typing = match &loaded.expansion {
            Some(expansion) => {
                let mut typing = typing;
                typing.names.retain(|&(start, _), _| start < expansion.len);
                typing
                    .type_names
                    .retain(|&(start, _), _| start < expansion.len);
                typing
            }
            None => typing,
        };
        Output {
            diagnostics,
            #[cfg(feature = "ide")]
            typing: Arc::new(typing),
            exported: if clean { interface.clone() } else { untyped },
            interface,
            ir,
        }
    }
}

/// What checking one module produces.
#[derive(Clone)]
struct Output {
    diagnostics: Vec<Diagnostic>,
    #[cfg(feature = "ide")]
    typing: Arc<types::Typing>,
    /// The checked interface, which editors see.
    interface: Interface,
    /// What importers see: without types when the module has errors.
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

/// The library a module of `root` belongs to; `None` for the project's own.
fn package_of(root: &Root) -> Option<&str> {
    match root {
        Root::Package(name) => Some(name),
        _ => None,
    }
}
