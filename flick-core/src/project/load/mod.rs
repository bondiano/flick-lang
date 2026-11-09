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
mod files;
mod format_plugins;
#[cfg(feature = "ide")]
mod ide;
mod imports;
mod loader;
mod plugins;
// the plugin of a module is its package's, which needs packages
#[cfg(all(test, feature = "native-project"))]
mod plugins_tests;
mod precompiled;
mod runner;
mod target;
mod unknown;

pub use checking::ModuleCache;
pub(crate) use checking::check_inline_file;
pub use entry::{MainKind, main_kind};
pub use format_plugins::FormatPlugins;
#[cfg(feature = "ide")]
pub use ide::{Analysis, Analyzed, analysis_fingerprint, analyze, analyze_with};
pub use precompiled::{CachedModule, Fresh, PackageCache};
#[cfg(all(test, feature = "build-artifacts"))]
pub(crate) use target::js_only;

use super::config::Backend;
#[cfg(feature = "native-project")]
use super::packages::Packages;
use super::{Host, Project, module_path, normalize};
use crate::diagnostics::{Diagnostic, Edit, Severity, codes};
use crate::ir;
use crate::plugin::EditorBlock;
use crate::plugin::expansion::Expansion;
use crate::resolve::{self, Interface, ModuleSyntax};
use crate::source::{FileId, SourceMap, Span};
use crate::syntax::{self, Parse};
use crate::types;
use files::expected_name;
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

impl Root {
    fn same_package(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Source | Self::Test, Self::Source | Self::Test) => true,
            (Self::Package(a), Self::Package(b)) => a == b,
            (Self::Other(a), Self::Other(b)) => a == b,
            _ => false,
        }
    }
}

struct Loaded {
    path: PathBuf,
    file: FileId,
    root: Root,
    parse: Arc<Parse>,
    syntax: Arc<ModuleSyntax>,
    /// From the header, else from the path; empty for neither.
    name: String,
    /// The module with its plugin blocks expanded, when it has any.
    expansion: Option<Arc<Expansion>>,
    /// Its plugin blocks, as editors ask their plugins about them.
    editing: Vec<EditorBlock>,
    /// The package whose plugin gives its code, for a file of another language.
    loader: Option<String>,
    /// A named application compiled as a separate generated module.
    virtual_of: Option<Virtual>,
}

#[derive(Clone)]
struct Virtual {
    at: Span,
    parameters: Vec<(String, String, String)>,
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
    /// The mode every module of the run and the built-in modules are checked in.
    stages_by_class: bool,
    #[cfg(feature = "native-project")]
    packages: Packages,
    modules: Vec<Loaded>,
    by_name: BTreeMap<String, usize>,
    /// The built-in modules a run reads, by name.
    from_packages: BTreeMap<String, Interface>,
    diagnostics: Vec<Diagnostic>,
    /// With `Some`, the IR of each module that checks without errors.
    lowered: Option<Vec<Option<Lowered>>>,
    /// Lowers only the modules of source packages, to save them.
    lower_packages_only: bool,
    /// Modules of source packages saved by earlier runs.
    precompiled: Precompiled,
    #[cfg(feature = "ide")]
    /// With `Some`, what editors need of each module that parses.
    ide: Option<Vec<Analyzed>>,
    /// With `Some`, the outputs of earlier runs to reuse.
    cache: Option<&'a mut ModuleCache>,
    /// The plugins that watch names of the modules, each with the names its
    /// package declares; filled while the blocks expand.
    watchers: plugins::Watchers,
}

/// A module's IR and what the reference model reads of it.
type Lowered = (ir::Module, Arc<Interface>);

/// The module `m` of the build, lowered.
fn compiled(m: &Loaded, (ir, declarations): Lowered, interface: Interface) -> Compiled {
    Compiled {
        name: m.name.clone(),
        path: m.path.clone(),
        file: m.file,
        test: m.root == Root::Test,
        package: match &m.root {
            Root::Package(name) => Some(name.clone()),
            _ => None,
        },
        imports: m
            .syntax
            .imports
            .iter()
            .chain(&m.syntax.inline_imports)
            .map(|i| i.module.clone())
            .collect(),
        ir,
        declarations,
        interface,
        expansion: m.expansion.clone(),
    }
}

/// The modules of a build: the project's and those of its libraries.
#[derive(Default)]
pub struct Compilation {
    pub modules: Vec<Compiled>,
    /// The libraries of `[dependencies]`, each with its source directory.
    pub libraries: Vec<(String, PathBuf)>,
    /// The npm packages its dependency closure asks for, which a js build
    /// needs installed.
    #[cfg(feature = "native-project")]
    pub js_dependencies: Vec<super::js::Wanted>,
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
    /// Every top-level entity with its type, for the reference model.
    pub declarations: Arc<Interface>,
    pub interface: Interface,
    /// The module with its plugin blocks expanded, when it has any.
    pub expansion: Option<Arc<Expansion>>,
}

impl Compilation {
    /// The library of `[dependencies]` that `module` belongs to, with its
    /// source directory.
    pub fn library_of(&self, module: &str) -> Option<(&str, &Path)> {
        let module = self.modules.iter().find(|m| m.name == module)?;
        let package = module.package.as_deref()?;
        (self.libraries.iter())
            .find(|(name, _)| name == package)
            .map(|(name, dir)| (name.as_str(), dir.as_path()))
    }

    /// The file of the foreign module `spec` when the build ships it beside
    /// `module`: an `.mjs` or `.cjs` of a library, inside its source
    /// directory, which a tarball or a fetched copy of the library has. Any
    /// other relative file stays where it is, and the output refers to it.
    pub fn shipped_foreign(&self, module: &str, spec: &str) -> Option<PathBuf> {
        if !spec.starts_with("./") && !spec.starts_with("../") {
            return None;
        }
        let shipped = Path::new(spec)
            .extension()
            .is_some_and(|e| e == "mjs" || e == "cjs");
        let (_, dir) = self.library_of(module).filter(|_| shipped)?;
        let source = self.modules.iter().find(|m| m.name == module)?;
        let path = normalize(&source.path.parent()?.join(spec));
        path.starts_with(dir).then_some(path)
    }
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
/// giving those to save; `rt` builds for Flick's own runtime whatever the
/// project says.
pub fn compile_with(
    host: &dyn Host,
    sources: &mut SourceMap,
    project: Option<&Project>,
    roots: &[PathBuf],
    packages: Option<&PackageCache>,
    rt: bool,
) -> (Vec<Diagnostic>, Compilation, Vec<Fresh>) {
    let mode = Mode::Compile;
    let output = run_in(host, sources, project, roots, mode, None, packages, rt);
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
    run_in(host, sources, project, roots, mode, cache, saved, false)
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn run_in(
    host: &dyn Host,
    sources: &mut SourceMap,
    project: Option<&Project>,
    roots: &[PathBuf],
    mode: Mode,
    cache: Option<&mut ModuleCache>,
    saved: Option<&PackageCache>,
    rt: bool,
) -> Output {
    let stages_by_class = rt || project.is_some_and(|p| p.config.build.effect == Backend::Rt);
    #[cfg(feature = "native-project")]
    let mut diagnostics = Vec::new();
    #[cfg(feature = "native-project")]
    let packages = project.map_or_else(Packages::default, |p| {
        Packages::load(host, sources, p, &mut diagnostics)
    });
    #[cfg(feature = "native-project")]
    let js_dependencies = project.map_or_else(Vec::new, |p| super::js::closure(p, &packages));
    #[cfg(feature = "native-project")]
    let libraries: Vec<(String, PathBuf)> = packages
        .libraries()
        .map(|(name, dir, _)| (name.to_owned(), dir.to_path_buf()))
        .collect();
    #[cfg(not(feature = "native-project"))]
    let libraries = Vec::new();
    #[cfg(feature = "native-project")]
    let precompiled = project
        .zip(saved)
        .map(|(p, saved)| Precompiled::new(&packages, p, saved, stages_by_class))
        .unwrap_or_default();
    #[cfg(not(feature = "native-project"))]
    let (diagnostics, precompiled) = (Vec::new(), Precompiled::default());
    // a check lowers the modules of source packages only to save them
    let lower_packages_only = mode == Mode::Check && saved.is_some();
    let mut loader = Loader {
        host,
        sources,
        project,
        stages_by_class,
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
        watchers: plugins::Watchers::default(),
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
            .filter_map(|((lowered, interface), m)| Some(compiled(m, lowered?, interface?)))
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
        compilation: Compilation {
            modules: compiled,
            libraries,
            #[cfg(feature = "native-project")]
            js_dependencies,
        },
        fresh,
        #[cfg(feature = "ide")]
        analyzed: loader.ide.take().unwrap_or_default(),
    }
}

impl Loader<'_> {
    /// The interfaces of the built-in modules, in the mode of this run.
    fn builtins(&self) -> &'static [Interface] {
        types::builtins_of(self.stages_by_class)
    }

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
    /// The parse of a module, from an earlier run while its text stays the same.
    fn parsed(&mut self, path: &Path, file: FileId) -> (Arc<Parse>, Arc<ModuleSyntax>) {
        let text = &self.sources.get(file).text;
        if let Some(found) = self.cache.as_ref().and_then(|c| c.parsed(path, file, text)) {
            return found;
        }
        let parse = Arc::new(syntax::parse(text, file));
        let module = Arc::new(resolve::module_syntax(&parse.tree, text, file));
        if let Some(cache) = &mut self.cache {
            let parsed = (parse.clone(), module.clone());
            cache.keep_parsed(path, file, text, parsed);
        }
        (parse, module)
    }
}
