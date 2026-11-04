//! Checking a set of modules together: loads the given files and, through
//! their imports, every module they depend on; checks the module ↔ path
//! mapping, name collisions and import cycles, then resolves each module
//! against the interfaces of its imports.

use std::collections::BTreeMap;
use std::io;
use std::mem;
use std::path::{Path, PathBuf};
use std::sync::Arc;

mod checking;
#[cfg(test)]
mod checking_tests;
mod entry;
mod format_plugins;
#[cfg(feature = "ide")]
mod ide;
mod plugins;
// the plugin of a module is its package's, which needs packages
#[cfg(all(test, feature = "native-project"))]
mod plugins_tests;
mod precompiled;
mod runner;
mod target;
mod unimported;
mod unknown;

pub use checking::ModuleCache;
pub use entry::{MainKind, main_kind};
pub use format_plugins::FormatPlugins;
#[cfg(feature = "ide")]
pub use ide::{Analysis, Analyzed, analysis_fingerprint, analyze, analyze_with};
pub use precompiled::{CachedModule, Fresh, PackageCache};
#[cfg(all(test, feature = "build-artifacts"))]
pub(crate) use target::js_only;

#[cfg(feature = "native-project")]
use super::packages::Packages;
use super::{Host, Project, module_name, module_path, normalize};
use crate::diagnostics::{Diagnostic, Edit, Severity, codes};
use crate::ir;
use crate::plugin::EditorBlock;
use crate::plugin::expansion::Expansion;
use crate::resolve::{self, Interface, ModuleSyntax};
use crate::source::{FileId, SourceMap, Span};
use crate::syntax::{self, Parse};
use crate::types;
use precompiled::Precompiled;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Root {
    Source,
    Test,
    /// The source directory of the named source package.
    Package(String),
    /// A file outside the project roots; the directory its module path
    /// starts in, when its header matches its path.
    Other(Option<PathBuf>),
}

struct Loaded {
    path: PathBuf,
    file: FileId,
    root: Root,
    parse: Parse,
    syntax: ModuleSyntax,
    /// From the header, else from the path; empty for neither.
    name: String,
    /// The module with its plugin blocks expanded, when it has any.
    expansion: Option<Arc<Expansion>>,
    /// Its plugin blocks, as editors ask their plugins about them.
    editing: Vec<EditorBlock>,
}

/// How far a run goes past checking.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Check,
    /// Also lowers every module to the IR when nothing has errors.
    Compile,
    #[cfg(feature = "ide")]
    /// [`Mode::Compile`], keeping what editors need of each module.
    Analyze,
}

struct Output {
    diagnostics: Vec<Diagnostic>,
    compilation: Compilation,
    /// The modules of source packages to save, after a clean run.
    fresh: Vec<Fresh>,
    #[cfg(feature = "ide")]
    analyzed: Vec<Analyzed>,
}

struct Loader<'a> {
    host: &'a dyn Host,
    sources: &'a mut SourceMap,
    project: Option<&'a Project>,
    #[cfg(feature = "native-project")]
    packages: Packages,
    modules: Vec<Loaded>,
    by_name: BTreeMap<String, usize>,
    /// The built-in modules a run reads, by name.
    from_packages: BTreeMap<String, Interface>,
    diagnostics: Vec<Diagnostic>,
    /// With `Some`, the IR of each module that checks without errors.
    lowered: Option<Vec<Option<ir::Module>>>,
    /// Lowers only the modules of source packages, to save them.
    lower_packages_only: bool,
    /// Modules of source packages saved by earlier runs.
    precompiled: Precompiled,
    #[cfg(feature = "ide")]
    /// With `Some`, what editors need of each module that parses.
    ide: Option<Vec<Analyzed>>,
    /// With `Some`, the outputs of earlier runs to reuse.
    cache: Option<&'a mut ModuleCache>,
}

/// The modules of a build: the project's and those of its libraries.
#[derive(Default)]
pub struct Compilation {
    pub modules: Vec<Compiled>,
}

/// A module that checked cleanly, lowered to the IR.
pub struct Compiled {
    pub name: String,
    pub path: PathBuf,
    pub file: FileId,
    /// Whether it is in the test root.
    pub test: bool,
    /// The source package it belongs to; `None` for the project's own.
    pub package: Option<String>,
    /// The modules it imports.
    pub imports: Vec<String>,
    pub ir: ir::Module,
    pub interface: Interface,
    /// The module with its plugin blocks expanded, when it has any.
    pub expansion: Option<Arc<Expansion>>,
}

/// Checks `roots` (absolute `.fl` paths) and their transitive imports.
pub fn check(
    host: &dyn Host,
    sources: &mut SourceMap,
    project: Option<&Project>,
    roots: &[PathBuf],
) -> Vec<Diagnostic> {
    run(host, sources, project, roots, Mode::Check, None, None).diagnostics
}

/// [`check`], taking the modules of source packages `packages` saved and
/// giving those to save.
pub fn check_with(
    host: &dyn Host,
    sources: &mut SourceMap,
    project: Option<&Project>,
    roots: &[PathBuf],
    packages: Option<&PackageCache>,
) -> (Vec<Diagnostic>, Vec<Fresh>) {
    let output = run(host, sources, project, roots, Mode::Check, None, packages);
    (output.diagnostics, output.fresh)
}

/// [`check`], also lowering every module to the IR when nothing has errors.
pub fn compile(
    host: &dyn Host,
    sources: &mut SourceMap,
    project: Option<&Project>,
    roots: &[PathBuf],
) -> (Vec<Diagnostic>, Compilation) {
    let Output {
        diagnostics,
        compilation,
        ..
    } = run(host, sources, project, roots, Mode::Compile, None, None);
    (diagnostics, compilation)
}

/// [`compile`], taking the modules of source packages `packages` saved and
/// giving those to save.
pub fn compile_with(
    host: &dyn Host,
    sources: &mut SourceMap,
    project: Option<&Project>,
    roots: &[PathBuf],
    packages: Option<&PackageCache>,
) -> (Vec<Diagnostic>, Compilation, Vec<Fresh>) {
    let output = run(host, sources, project, roots, Mode::Compile, None, packages);
    (output.diagnostics, output.compilation, output.fresh)
}

fn run(
    host: &dyn Host,
    sources: &mut SourceMap,
    project: Option<&Project>,
    roots: &[PathBuf],
    mode: Mode,
    cache: Option<&mut ModuleCache>,
    saved: Option<&PackageCache>,
) -> Output {
    #[cfg(feature = "native-project")]
    let mut diagnostics = Vec::new();
    #[cfg(feature = "native-project")]
    let packages = project.map_or_else(Packages::default, |p| {
        Packages::load(host, sources, p, &mut diagnostics)
    });
    #[cfg(feature = "native-project")]
    let precompiled = project
        .zip(saved)
        .map(|(p, saved)| Precompiled::new(&packages, p, saved))
        .unwrap_or_default();
    #[cfg(not(feature = "native-project"))]
    let (diagnostics, precompiled) = (Vec::new(), Precompiled::default());
    // a check lowers the modules of source packages only to save them
    let lower_packages_only = mode == Mode::Check && saved.is_some();
    let mut loader = Loader {
        host,
        sources,
        project,
        #[cfg(feature = "native-project")]
        packages,
        modules: Vec::new(),
        by_name: BTreeMap::new(),
        from_packages: BTreeMap::new(),
        diagnostics,
        lowered: (mode != Mode::Check || lower_packages_only).then(Vec::new),
        lower_packages_only,
        precompiled,
        #[cfg(feature = "ide")]
        ide: (mode == Mode::Analyze).then(Vec::new),
        cache,
    };
    let mut roots: Vec<(Root, PathBuf)> = roots
        .iter()
        .map(|p| (loader.root_of(&normalize(p)), normalize(p)))
        .collect();
    roots.sort();
    roots.dedup();
    for (_, path) in roots {
        if let Err(err) = loader.load(&path) {
            let message = format!("cannot read `{}`: {err}", loader.host.display(&path));
            let diagnostic = Diagnostic::new(codes::P001, Severity::Error, message);
            loader.diagnostics.push(diagnostic);
        }
    }
    let edges = loader.load_imports();
    loader.import_cycles(&edges);
    loader.expand_plugins();
    if let Some(lowered) = &mut loader.lowered {
        lowered.resize_with(loader.modules.len(), || None);
    }
    let interfaces = loader.resolve_all(&edges);
    if let Some(cache) = &mut loader.cache {
        // modules no longer loaded are dropped
        cache.retain_loaded(&loader.modules);
    }
    loader.check_entry(&interfaces);
    loader.check_targets();
    let failed = loader
        .diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error);
    let mut compiled = match loader.lowered.take() {
        Some(lowered) if !failed => lowered
            .into_iter()
            .zip(interfaces)
            .zip(&loader.modules)
            .filter_map(|((ir, interface), m)| {
                Some(Compiled {
                    name: m.name.clone(),
                    path: m.path.clone(),
                    file: m.file,
                    test: m.root == Root::Test,
                    package: match &m.root {
                        Root::Package(name) => Some(name.clone()),
                        _ => None,
                    },
                    imports: m.syntax.imports.iter().map(|i| i.module.clone()).collect(),
                    ir: ir?,
                    interface: interface?,
                    expansion: m.expansion.clone(),
                })
            })
            .collect(),
        _ => Vec::new(),
    };
    let fresh = loader.finish_packages(&mut compiled, mode, saved.is_some() && !failed);
    let mut diagnostics = loader.reported();
    // by path, not by load order, which follows the order of imports
    diagnostics.sort_by_cached_key(|d| {
        (
            d.span.is_none(),
            d.span
                .map(|s| (loader.sources.get(s.file).path.clone(), s.start, s.end)),
        )
    });
    Output {
        diagnostics,
        compilation: Compilation { modules: compiled },
        fresh,
        #[cfg(feature = "ide")]
        analyzed: loader.ide.take().unwrap_or_default(),
    }
}

impl Loader<'_> {
    /// The diagnostics of the run without the warnings it does not report:
    /// those the project allows, and those of source packages, whose
    /// dependent does not answer for them.
    fn reported(&mut self) -> Vec<Diagnostic> {
        let package_files: Vec<FileId> = self
            .modules
            .iter()
            .filter(|m| matches!(m.root, Root::Package(_)))
            .map(|m| m.file)
            .collect();
        let allowed = self.project.map(|p| &p.config.allow);
        let mut diagnostics = mem::take(&mut self.diagnostics);
        diagnostics.retain(|d| {
            d.severity != Severity::Warning
                || (d.span.is_none_or(|s| !package_files.contains(&s.file))
                    && allowed.is_none_or(|a| !a.contains(&d.code)))
        });
        diagnostics
    }

    fn root_of(&self, path: &Path) -> Root {
        match self.project {
            Some(p) if path.starts_with(p.source_root()) => Root::Source,
            Some(p) if path.starts_with(p.test_root()) => Root::Test,
            _ => self
                .package_sources()
                .into_iter()
                .find(|(_, dir)| path.starts_with(dir))
                .map_or(Root::Other(None), |(name, _)| Root::Package(name)),
        }
    }

    /// The source directory of each source package the project depends on.
    #[cfg_attr(not(feature = "native-project"), allow(clippy::unused_self))]
    fn package_sources(&self) -> Vec<(String, PathBuf)> {
        #[cfg(feature = "native-project")]
        return self
            .packages
            .sources()
            .map(|(name, dir)| (name.to_owned(), dir.to_path_buf()))
            .collect();
        #[cfg(not(feature = "native-project"))]
        Vec::new()
    }

    /// Loads and registers a module file once; `Ok(None)` if it is already loaded.
    fn load(&mut self, path: &Path) -> io::Result<Option<usize>> {
        if self.modules.iter().any(|m| m.path == path) {
            return Ok(None);
        }
        let bytes = self.host.read(path)?;
        let (file, utf8) = self.sources.add_bytes(self.host.display(path), bytes);
        self.diagnostics.extend(utf8);
        let text = &self.sources.get(file).text;
        let parse = syntax::parse(text, file);
        let syntax = resolve::module_syntax(&parse.tree, text, file);
        let mut root = self.root_of(path);
        let root_dir = match (&root, self.project) {
            (Root::Source, Some(p)) => Some(p.source_root()),
            (Root::Test, Some(p)) => Some(p.test_root()),
            (Root::Package(name), _) => self
                .package_sources()
                .into_iter()
                .find(|(n, _)| n == name)
                .map(|(_, dir)| dir),
            _ => None,
        };
        let expected = root_dir
            .as_ref()
            .map(|dir| module_name(path.strip_prefix(dir).unwrap_or(path)));
        let header = syntax.header.as_ref();
        let name = match (header, &expected) {
            (Some(h), _) => h.name.clone(),
            (None, Some(Some(name))) => name.clone(),
            _ => String::new(),
        };
        match (&expected, header) {
            (Some(Some(expected)), Some(h)) if *expected != h.name => self.diagnostics.push(
                Diagnostic::error(
                    codes::R001,
                    format!("`{}` must declare `module {expected}`, not `{}`", self.host.display(path), h.name),
                    h.span,
                )
                .with_fix(|| (
                    format!(
                        "rename the module, or move the file to `{}`",
                        root_dir.as_ref().map_or(module_path(&h.name), |d| PathBuf::from(self.host.display(&d.join(module_path(&h.name))))).display()
                    ),
                    vec![Edit { span: h.span, replacement: expected.clone() }],
                )),
            ),
            (Some(None), _) => self.diagnostics.push(
                Diagnostic::error(
                    codes::R001,
                    format!(
                        "`{}` is not at a module path: each directory and the file name must be a capitalized name",
                        self.host.display(path)
                    ),
                    header.map_or(Span::new(file, 0, 0), |h| h.span),
                )
                .with_fix(|| ("rename the file after its module, e.g. `src/App/Users.fl` for `App.Users`", Vec::new())),
            ),
            _ => {}
        }
        if let (Root::Other(dir), Some(h)) = (&mut root, header) {
            // `x/App/Users.fl` declaring `App.Users` imports siblings from `x/`
            let tail = module_path(&h.name);
            *dir = path
                .to_str()
                .and_then(|p| p.strip_suffix(tail.to_str()?))
                .map(PathBuf::from);
        }
        let index = self.modules.len();
        if !name.is_empty() {
            match self.by_name.get(&name) {
                Some(&first) => {
                    let first = &self.modules[first];
                    let mut diagnostic = Diagnostic::error(
                        codes::R002,
                        format!(
                            "module `{name}` is also declared by `{}`",
                            self.host.display(&first.path)
                        ),
                        header.map_or(Span::new(file, 0, 0), |h| h.span),
                    );
                    if let Some(h) = &first.syntax.header {
                        diagnostic = diagnostic.with_related(h.span, "first declared here");
                    }
                    self.diagnostics.push(diagnostic);
                }
                None => {
                    self.by_name.insert(name.clone(), index);
                }
            }
        }
        self.modules.push(Loaded {
            path: path.to_path_buf(),
            file,
            root,
            parse,
            syntax,
            name,
            expansion: None,
            editing: Vec::new(),
        });
        Ok(Some(index))
    }

    /// Loads imported modules until the set is closed; returns the local
    /// import edges of each module, sorted by module name.
    fn load_imports(&mut self) -> Vec<Vec<usize>> {
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

    fn import_cycles(&mut self, edges: &[Vec<usize>]) {
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
