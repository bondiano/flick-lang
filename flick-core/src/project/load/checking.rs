//! Checking the loaded modules: each after the modules it imports, those
//! of one level of the import graph in parallel, reusing what a
//! [`ModuleCache`] kept of earlier runs.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tracing::debug;

#[cfg(feature = "ide")]
use super::Analyzed;
use super::plugins::Watching;
use super::precompiled::Precompiled;
use super::{BindingOrigin, CalleeOrigin, Loaded, Loader, Root, Virtual};
use crate::clock::{micros, timed};
use crate::diagnostics::{Diagnostic, Severity, codes};
use crate::iface::Namespace;
use crate::ir;
use crate::plugin::calls as plugin_calls;
use crate::plugin::expansion::Expansion;
use crate::plugin::first_party;
use crate::project::Project;
use crate::project::config::{Backend, Config};
use crate::resolve::{self, Interface, ModuleSyntax};
use crate::resolve::{Header, Import, Item, ItemKind};
use crate::source::Span;
use crate::source::{FileId, SourceMap};
use crate::syntax::cst::{Element, Node};
use crate::syntax::{Parse, kind::SyntaxKind as K};
use crate::types;

/// A file checked on its own takes the path of a module of a project run,
/// while resolving only built-in imports.
pub(crate) fn check_file(
    sources: &SourceMap,
    file: FileId,
    parse: Parse,
    syntax: ModuleSyntax,
    expansion: Option<Expansion>,
    mut derived: Vec<Diagnostic>,
) -> Vec<Diagnostic> {
    let name = syntax
        .header
        .as_ref()
        .map_or(String::new(), |h| h.name.clone());
    let loaded = Loaded {
        path: PathBuf::from(&sources.get(file).path),
        file,
        root: Root::Other(None),
        parse: Arc::new(parse),
        syntax: Arc::new(syntax),
        name: name.clone(),
        expansion: expansion.map(Arc::new),
        editing: Vec::new(),
        loader: None,
        virtual_of: None,
    };
    let modules = [loaded];
    let mut by_name = BTreeMap::new();
    by_name.insert(name, 0);
    let from_packages = BTreeMap::new();
    let precompiled = Precompiled::default();
    let watched = types::Watched::new(first_party::WATCH);
    let watching = first_party::plugin(first_party::PACKAGE).map(|plugin| Watching {
        package: first_party::PACKAGE.to_owned(),
        plugin,
        watched: types::Watched::new(first_party::WATCH),
    });
    let watching: Vec<_> = watching.into_iter().collect();
    let checking = Checking {
        modules: &modules,
        sources,
        by_name: &by_name,
        from_packages: &from_packages,
        project: None,
        stages_by_class: false,
        lower: false,
        cached_ir: CachedIr::Dropped,
        lower_packages_only: false,
        precompiled: &precompiled,
        cache: None,
        watching: &watching,
        watched: &watched,
    };
    let builtin = |name: &str| {
        types::builtins()
            .iter()
            .find(|i| i.module == name && !i.private)
            .cloned()
    };
    let mut diagnostics = checking.check(&modules[0], &builtin).diagnostics;
    diagnostics.append(&mut derived);
    // a lone file has no package, so no private module of one is its own
    for import in &modules[0].syntax.imports {
        let private = |i: &Interface| i.module == import.module && i.private;
        if types::builtins().iter().any(private) {
            diagnostics.push(Diagnostic::error(
                codes::R017,
                format!(
                    "module `{}` is private to its package; import its public facade instead",
                    import.module
                ),
                import.span,
            ));
        }
    }
    diagnostics.sort_by_key(|d| d.span.map(|s| (s.start, s.end)));
    diagnostics
}

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
type ContractView = (
    BTreeMap<String, Interface>,
    Vec<(types::Type, types::Dict)>,
    Vec<Diagnostic>,
);

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
    /// An editor's entry, without the IR, is none a build reads.
    cached_ir: CachedIr,
    /// Each import's interface, without where its names are declared and
    /// their docs, which checking does not read, and whether it is a module
    /// of the project.
    imports: Vec<(Option<Arc<Interface>>, bool)>,
    virtual_contracts: Vec<Option<Arc<Interface>>>,
    virtual_parameters: Vec<(String, String, String)>,
    virtual_bindings: Vec<BindingOrigin>,
    virtual_callee: Option<CalleeOrigin>,
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
            // an editor's run reads no IR; a check and a build do
            #[cfg(feature = "ide")]
            cached_ir: if self.ide.is_some() {
                CachedIr::Dropped
            } else {
                CachedIr::Kept
            },
            #[cfg(not(feature = "ide"))]
            cached_ir: CachedIr::Kept,
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
            ide.extend(output.inline_ide);
        }
        known.interfaces[i] = Some(output.exported);
    }
}

/// What a cached output does with a module's IR: an editor reads its
/// diagnostics only, and keeping the IR would copy all of it on every
/// keystroke; a check or a build that runs again reads it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum CachedIr {
    Kept,
    Dropped,
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
    cached_ir: CachedIr,
    lower_packages_only: bool,
    precompiled: &'a Precompiled,
    cache: Option<&'a ModuleCache>,
    /// The plugins that watch names of every module.
    watching: &'a [super::plugins::Watching],
    /// Every name they watch, which the checker collects.
    watched: &'a types::Watched,
}

include!("checking_core.rs");
include!("checking_contracts.rs");

include!("checking_inline.rs");
include!("checking_relocate.rs");
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
    #[cfg(feature = "ide")]
    inline_ide: Vec<Analyzed>,
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
