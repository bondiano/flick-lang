//! Checking the loaded modules: each after the modules it imports, those
//! of one level of the import graph in parallel, reusing what a
//! [`ModuleCache`] kept of earlier runs.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tracing::debug;

#[cfg(feature = "ide")]
use super::Analyzed;
use super::precompiled::Precompiled;
use super::{Loaded, Loader, Root};
use crate::clock::timed;
use crate::diagnostics::{Diagnostic, Severity};
use crate::ir;
use crate::plugin::calls as plugin_calls;
#[cfg(feature = "ide")]
use crate::plugin::expansion::Expansion;
use crate::project::Project;
use crate::project::config::{Backend, Config};
use crate::resolve::{self, Interface, ModuleSyntax};
#[cfg(feature = "ide")]
use crate::source::Span;
use crate::source::{FileId, SourceMap};
use crate::syntax::Parse;
use crate::types;

mod graph;

pub(super) use graph::levels;
use graph::{package_of, parallel};

/// The output of checking each module in earlier runs, reused while the
/// module's text, the interfaces of its imports and the configuration stay
/// the same.
#[derive(Default)]
pub struct ModuleCache {
    entries: HashMap<PathBuf, (Key, Output)>,
    /// What plugins gave in the last run, by the key of each call.
    pub(super) plugins: HashMap<Vec<u8>, Vec<u8>>,
    /// The parse of each module, and the file and the text it is of.
    parses: HashMap<PathBuf, (FileId, String, Parsed)>,
}

type Parsed = (Arc<Parse>, Arc<ModuleSyntax>);

impl ModuleCache {
    /// Drops the modules a run no longer loaded.
    pub(super) fn retain_loaded(&mut self, modules: &[Loaded]) {
        self.entries
            .retain(|path, _| modules.iter().any(|m| m.path == *path));
        self.parses
            .retain(|path, _| modules.iter().any(|m| m.path == *path));
    }

    /// The parse kept of `path`, if it is of this file and this text.
    pub(super) fn parsed(&self, path: &Path, file: FileId, text: &str) -> Option<Parsed> {
        self.parses
            .get(path)
            .filter(|(kept, kept_text, _)| *kept == file && kept_text == text)
            .map(|(_, _, parsed)| parsed.clone())
    }

    pub(super) fn keep_parsed(&mut self, path: &Path, file: FileId, text: &str, parsed: Parsed) {
        let entry = (file, text.to_owned(), parsed);
        self.parses.insert(path.to_path_buf(), entry);
    }
}

#[derive(PartialEq)]
struct Key {
    file: FileId,
    text: String,
    /// The effective configuration, including programmatically supplied values.
    config: Option<Config>,
    lower: bool,
    /// Each import's interface, without where its names are declared and
    /// their docs, which checking does not read, and whether it is a module
    /// of the project.
    imports: Vec<(Option<Arc<Interface>>, bool)>,
    /// What tells the watching plugins apart: a module without blocks is
    /// checked again when one of them changes.
    watching: Vec<String>,
}

impl Loader<'_> {
    /// Resolves every module after the modules it imports; an import that
    /// closes a cycle sees no interface. Without cycles, the modules of one
    /// level of the import graph are checked in parallel.
    pub(super) fn resolve_all(&mut self, edges: &[Vec<usize>]) -> Vec<Option<Interface>> {
        let mut known = Known {
            interfaces: vec![None; self.modules.len()],
            comparable: vec![None; self.modules.len()],
        };
        let Some(levels) = levels(edges) else {
            let mut visiting = vec![false; self.modules.len()];
            for i in 0..self.modules.len() {
                self.resolve_one(i, edges, &mut known, &mut visiting);
            }
            return known.interfaces;
        };
        for level in levels {
            let checking = self.checking();
            let outputs = parallel(&level, |i| {
                checking.module(i, &known.interfaces, &known.comparable)
            });
            for (i, output) in level.into_iter().zip(outputs) {
                self.apply(i, output, &mut known);
            }
        }
        known.interfaces
    }

    fn resolve_one(
        &mut self,
        i: usize,
        edges: &[Vec<usize>],
        known: &mut Known,
        visiting: &mut Vec<bool>,
    ) {
        if known.interfaces[i].is_some() || visiting[i] {
            return;
        }
        visiting[i] = true;
        for &j in &edges[i] {
            self.resolve_one(j, edges, known, visiting);
        }
        let output = self
            .checking()
            .module(i, &known.interfaces, &known.comparable);
        self.apply(i, output, known);
        visiting[i] = false;
    }

    fn checking(&self) -> Checking<'_> {
        Checking {
            modules: &self.modules,
            sources: self.sources,
            by_name: &self.by_name,
            from_packages: &self.from_packages,
            project: self.project,
            watching: &self.watchers.plugins,
            watched: &self.watchers.watched,
            stages_by_class: self.stages_by_class,
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

    fn apply(&mut self, i: usize, (output, key): (Output, Option<Key>), known: &mut Known) {
        if let (Some(cache), Some(key)) = (&mut self.cache, key) {
            known.comparable[i] = Some(comparable_copy(&output.exported));
            cache
                .entries
                .insert(self.modules[i].path.clone(), (key, output.clone()));
        }
        self.diagnostics.extend(output.diagnostics);
        if let (Some(lowered), Some(ir)) = (&mut self.lowered, output.ir) {
            lowered[i] = Some((ir, output.declarations));
        }
        #[cfg(feature = "ide")]
        if let Some(ide) = &mut self.ide {
            ide.push(Analyzed::new(
                &self.modules[i],
                output.typing,
                output.interface,
            ));
        }
        known.interfaces[i] = Some(output.exported);
    }
}

/// What the modules checked so far show their importers.
struct Known {
    interfaces: Vec<Option<Interface>>,
    /// Each one as a cache key compares it, one copy for all its importers;
    /// only with a cache.
    comparable: Vec<Option<Arc<Interface>>>,
}

/// What checking a module reads of the loader; shared by parallel checks.
struct Checking<'a> {
    modules: &'a [Loaded],
    sources: &'a SourceMap,
    by_name: &'a BTreeMap<String, usize>,
    from_packages: &'a BTreeMap<String, Interface>,
    project: Option<&'a Project>,
    stages_by_class: bool,
    lower: bool,
    lower_packages_only: bool,
    precompiled: &'a Precompiled,
    cache: Option<&'a ModuleCache>,
    /// The plugins that watch names of every module.
    watching: &'a [super::plugins::Watching],
    /// Every name they watch, which the checker collects.
    watched: &'a types::Watched,
}

impl Checking<'_> {
    /// Resolves, checks and, when asked and clean, lowers module `i` against
    /// the interfaces known so far, or reuses the cached output; with a
    /// cache, also the key to cache it by.
    fn module(
        &self,
        i: usize,
        interfaces: &[Option<Interface>],
        comparable: &[Option<Arc<Interface>>],
    ) -> (Output, Option<Key>) {
        let loaded = &self.modules[i];
        let known = |name: &str| match self.by_name.get(name) {
            Some(&j) => interfaces[j].as_ref(),
            None => self.from_packages.get(name),
        };
        let lookup = |name: &str| known(name).cloned();
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
                    let interface = match self.by_name.get(&i.module) {
                        Some(&j) => comparable[j].clone(),
                        None => self.from_packages.get(&i.module).map(comparable_copy),
                    };
                    (interface, self.by_name.contains_key(&i.module))
                })
                .collect(),
            watching: (self.watching.iter())
                .map(|w| w.plugin.id().to_owned())
                .collect(),
        };
        match cache.entries.get(&loaded.path) {
            Some((cached, output)) if *cached == key => {
                let mut output = output.clone();
                // re-exported names are declared where their module now says
                for import in &loaded.syntax.imports {
                    if let Some(interface) = known(&import.module) {
                        redeclare(&mut output.interface, interface);
                        redeclare(&mut output.exported, interface);
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
            generated: loaded
                .expansion
                .as_ref()
                .map_or(resolve::NO_GENERATED, |e| e.len),
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
            no_let_assert: self.project.is_some_and(|p| p.config.no_let_assert),
            thresholds: self
                .project
                .map_or_else(Default::default, |p| p.config.thresholds),
            watched: self.watched,
            effects: build.is_none_or(|b| b.effect != Backend::None),
            stages_by_class: self.stages_by_class,
        };
        let mut diagnostics = loaded.parse.diagnostics.clone();
        let resolved_cleanly = !resolved.iter().any(|d| d.severity == Severity::Error);
        diagnostics.extend(resolved);
        // Keep the resolved exports before checking: a failed module may leave
        // store-local type variables in its inferred schemes. Importers must
        // not read those variables with their own unification store.
        let broken = !loaded.parse.diagnostics.is_empty();
        let mut untyped = interface.clone();
        let (mut checked, typing) = types::check_typed(
            &module,
            lookup,
            types::builtins_of(self.stages_by_class),
            options,
            &mut interface,
        );
        // what the plugins find from the types of their code stops lowering too
        if let Some(expansion) = &loaded.expansion {
            checked.extend(expansion.check_blocks(&loaded.editing, &typing.wildcards));
        }
        checked.extend(self.watched_calls(loaded, &typing));
        let checked = types::beside_syntax_errors(tree, checked);
        let clean =
            !broken && resolved_cleanly && !checked.iter().any(|d| d.severity == Severity::Error);
        diagnostics.extend(checked);
        let declarations = typing.declarations.clone();
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
        #[cfg(feature = "ide")]
        let typing = match &loaded.expansion {
            Some(expansion) => {
                let own = &self.sources.get(loaded.file).text;
                editor_names(typing, expansion, loaded.file, own)
            }
            None => typing,
        };
        Output {
            diagnostics,
            #[cfg(feature = "ide")]
            typing: Arc::new(typing),
            exported: if clean { interface.clone() } else { untyped },
            interface,
            // editors, the runs with a cache, read its diagnostics only, and
            // keeping the IR would copy all of it on every keystroke
            ir: ir.filter(|_| self.cache.is_none()),
            declarations,
        }
    }
}

impl Checking<'_> {
    /// What the watching plugins find wrong with the places of `loaded` that
    /// call the names each watches, at spans of the module's own text; a
    /// plugin gets what it collected from this module's blocks, if any.
    fn watched_calls(&self, loaded: &Loaded, typing: &types::Typing) -> Vec<Diagnostic> {
        if typing.call_sites.is_empty() {
            return Vec::new();
        }
        let expansion = loaded.expansion.as_deref();
        let sites = plugin_calls::placed_sites(&typing.call_sites, expansion, loaded.file);
        (self.watching.iter())
            .flat_map(|w| {
                // Each block of a collected group holds the same state,
                // already collected from all its blocks, not just this one.
                let state = (loaded.editing.iter())
                    .find(|e| e.plugin.id() == w.plugin.id())
                    .map(|e| Arc::clone(&e.state));
                let state = state.as_deref().unwrap_or(&[]);
                plugin_calls::check_calls(
                    w.plugin.as_ref(),
                    &w.package,
                    &w.watched,
                    &sites,
                    state,
                    loaded.file,
                )
            })
            .collect()
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
    /// Every top-level entity with its type, for the reference model.
    declarations: Arc<Interface>,
}

/// `interface` as a cache key compares it: without where its names are
/// declared and their docs, which checking does not read.
fn comparable_copy(interface: &Interface) -> Arc<Interface> {
    let mut copy = interface.clone();
    for export in &mut copy.exports {
        export.at = None;
    }
    copy.docs.clear();
    Arc::new(copy)
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

/// The names editors find and rename: those of the module's own text, and
/// those a plugin's code writes where the user wrote them, as a table
/// `Users` of a query that stands for `Users.table`.
#[cfg(feature = "ide")]
fn editor_names(
    mut typing: types::Typing,
    expansion: &Expansion,
    file: FileId,
    own: &str,
) -> types::Typing {
    let written: Vec<_> = (typing.names.iter())
        .filter(|((start, _), _)| *start >= expansion.len)
        .filter_map(|(&(start, end), r)| {
            let at = expansion.place(Span::new(file, start, end));
            let word = own.get(at.start as usize..at.end as usize)?;
            let types::Ref::Value { name, .. } = r else {
                return None;
            };
            let named = name
                .split_once('$')
                .map_or(name.as_str(), |(owner, _)| owner);
            let last = word.rsplit('.').next().unwrap_or(word);
            (at.end <= expansion.len && last == named).then(|| ((at.start, at.end), r.clone()))
        })
        .collect();
    typing.names.retain(|&(start, _), _| start < expansion.len);
    typing.names.extend(written);
    typing
        .type_names
        .retain(|&(start, _), _| start < expansion.len);
    typing
}
