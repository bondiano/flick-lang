//! Checking a set of modules together: loads the given files and, through
//! their imports, every module they depend on; checks the module ↔ path
//! mapping, name collisions and import cycles, then resolves each module
//! against the interfaces of its imports.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::io;
use std::mem;
use std::path::{Path, PathBuf};
use std::sync::Arc;

mod checking;
#[cfg(test)]
mod checking_tests;
mod entry;
mod expressions;
mod files;
mod format_plugins;
#[cfg(feature = "ide")]
mod ide;
mod imports;
mod lexical;
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
#[cfg(feature = "native-project")]
pub use checking::{ModuleStore, SavedModule};
pub(crate) use checking::{check_file, prove_file};
pub use entry::{MainKind, host_services, main_kind};
pub use format_plugins::FormatPlugins;
#[cfg(feature = "ide")]
pub use ide::{Analysis, Analyzed, analysis_fingerprint, analyze, analyze_rt, analyze_with};
pub use precompiled::{CachedModule, Fresh, PackageCache};
#[cfg(all(test, feature = "native-project"))]
pub(crate) use target::js_only;
pub use target::stages_by_class;

#[cfg(feature = "native-project")]
use super::packages::{Packages, project_name};
use super::{Host, Project, companion_of, module_path, normalize};
use crate::diagnostics::{Diagnostic, Edit, Severity, codes};
use crate::iface::{
    BindingOrigin, CalleeOrigin, ExpansionFrame, LexicalScope, StaticArgument, StaticBinding,
    StaticCapture, StaticMember,
};
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
    /// The project's proof root, which only `flick prove` selects.
    Proof,
    /// The source directory of the named source package.
    Package(String),
    /// A file outside the project roots; the directory its module path
    /// starts in, when its header matches its path.
    Other(Option<PathBuf>),
}

impl Root {
    fn same_package(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Source | Self::Test | Self::Proof, Self::Source | Self::Test | Self::Proof) => {
                true
            }
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
    member_of: Option<StaticMember>,
    scope_of: Option<LexicalScope>,
}

include!("loaded.rs");

#[derive(Clone)]
struct Virtual {
    at: Span,
    capture: StaticCapture,
}

/// How far a run goes past checking.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Check,
    /// Like [`Mode::Check`], also verifying contracts in selected roots.
    Prove,
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
    expression_serial: usize,
    expressions_active: BTreeSet<String>,
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
    /// What a test companion sees of its module, by the module's name: all
    /// it declares. Only a run that loads the companion makes one.
    privates: BTreeMap<String, Interface>,
    /// Whether this run verifies contracts (`Mode::Prove`).
    prove_contracts: bool,
    /// Normalized root paths selected for this run (used to narrow contract
    /// verification to selected modules in `flick prove <path>`).
    selected_roots: HashSet<PathBuf>,
    /// Every `(module, name)` declared with a `where` contract across all
    /// loaded modules in this run.
    all_contracted: HashSet<(String, String)>,
    /// Verified contracts produced per module during this run.
    verified_contracts: Vec<Vec<ir::ProvenContract>>,
}

/// A module's IR and what the reference model reads of it.
type Lowered = (ir::Module, Arc<Interface>, Vec<ir::Proven>);

include!("compiled.rs");

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
/// giving those to save; a command that checks again and again keeps what
/// it checked in `cache`.
pub fn check_with(
    host: &dyn Host,
    sources: &mut SourceMap,
    project: Option<&Project>,
    roots: &[PathBuf],
    packages: Option<&PackageCache>,
    cache: Option<&mut ModuleCache>,
) -> (Vec<Diagnostic>, Vec<Fresh>) {
    let output = run(host, sources, project, roots, Mode::Check, cache, packages);
    (output.diagnostics, output.fresh)
}

/// Checks `roots` (absolute `.fl` paths) and their transitive imports, also
/// verifying proofs and every contract declared in `roots`.
pub fn prove(
    host: &dyn Host,
    sources: &mut SourceMap,
    project: Option<&Project>,
    roots: &[PathBuf],
) -> Vec<Diagnostic> {
    run(host, sources, project, roots, Mode::Prove, None, None).diagnostics
}

/// [`prove`], reusing saved package modules and `cache` for non-proved modules
/// while always re-proving proof and contracted modules.
pub fn prove_with(
    host: &dyn Host,
    sources: &mut SourceMap,
    project: Option<&Project>,
    roots: &[PathBuf],
    packages: Option<&PackageCache>,
    cache: Option<&mut ModuleCache>,
) -> (Vec<Diagnostic>, Vec<Fresh>) {
    let output = run(host, sources, project, roots, Mode::Prove, cache, packages);
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
/// project says; a command that builds again and again keeps what it
/// compiled in `cache`.
pub fn compile_with(
    host: &dyn Host,
    sources: &mut SourceMap,
    project: Option<&Project>,
    roots: &[PathBuf],
    packages: Option<&PackageCache>,
    rt: bool,
    cache: Option<&mut ModuleCache>,
) -> (Vec<Diagnostic>, Compilation, Vec<Fresh>) {
    let mode = Mode::Compile;
    let output = run_in(host, sources, project, roots, mode, cache, packages, rt);
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
    let stages_by_class = stages_by_class(project, rt);
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
    // a check lowers the modules of source packages only to save them;
    // a selected environment also needs IR for target preflight
    let check_only = matches!(mode, Mode::Check | Mode::Prove);
    let lower_packages_only = check_only && saved.is_some();
    let environment_preflight = project.is_some_and(|p| p.config.build.environment.is_some());
    let mut loader = Loader {
        expression_serial: 0,
        expressions_active: BTreeSet::new(),
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
        lowered: (!check_only || lower_packages_only || environment_preflight).then(Vec::new),
        lower_packages_only: lower_packages_only && !environment_preflight,
        precompiled,
        #[cfg(feature = "ide")]
        ide: (mode == Mode::Analyze).then(Vec::new),
        cache,
        watchers: plugins::Watchers::default(),
        privates: BTreeMap::new(),
        prove_contracts: mode == Mode::Prove,
        selected_roots: HashSet::new(),
        all_contracted: HashSet::new(),
        verified_contracts: Vec::new(),
    };
    let mut roots: Vec<(Root, PathBuf)> = roots
        .iter()
        .map(|p| (loader.root_of(&normalize(p)), normalize(p)))
        .collect();
    roots.sort();
    roots.dedup();
    loader.selected_roots = roots.iter().map(|(_, path)| path.clone()).collect();
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
    loader.size_lowered();
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
            .filter_map(|((lowered, interface), m)| {
                if m.scope_of.is_some() {
                    return None;
                }
                let interface = interface?;
                let lowered = lowered.or_else(|| {
                    interface.parameterized.as_ref()?;
                    Some((
                        ir::Module {
                            name: m.name.clone(),
                            items: Vec::new(),
                            labels: BTreeMap::new(),
                            initialization: flick_ir::Initialization::default(),
                            records: BTreeMap::new(),
                            package: None,
                            exports: flick_ir::Exports::default(),
                        },
                        Arc::new(interface.clone()),
                        Vec::new(),
                    ))
                })?;
                Some(compiled(m, lowered, interface))
            })
            .collect(),
        _ => Vec::new(),
    };
    let fresh = loader.finish_packages(&mut compiled, mode, saved.is_some() && !failed);
    for c in &mut compiled {
        // the code of a module with a loaded test companion exports to it all
        // it declares; a build never loads a companion, nor makes this code
        let interface = loader.privates.get(&c.name).unwrap_or(&c.interface);
        c.ir.exports = ir::exports(&c.name, interface);
    }
    // a module's package names the tags of its constructors: its source
    // package, else the project's
    #[cfg(feature = "native-project")]
    {
        let own = project.filter(|_| !compiled.is_empty());
        let own = own.and_then(|p| project_name(host, &p.root));
        for c in &mut compiled {
            c.ir.package = c.package.clone().or_else(|| own.clone());
        }
    }
    let mut diagnostics = loader.reported();
    #[cfg(feature = "ide")]
    let analyzed = loader.ide.take().unwrap_or_default();
    drop(loader);
    let compilation = Compilation {
        modules: compiled,
        libraries,
        #[cfg(feature = "native-project")]
        js_dependencies,
    };
    #[cfg(not(target_arch = "wasm32"))]
    target::check_environment(&compilation, project, sources, &mut diagnostics);
    // by path, not by load order, which follows the order of imports
    diagnostics.sort_by_cached_key(|d| {
        (
            d.span.is_none(),
            d.span
                .map(|s| (sources.get(s.file).path.clone(), s.start, s.end)),
        )
    });
    Output {
        diagnostics,
        compilation,
        fresh,
        #[cfg(feature = "ide")]
        analyzed,
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

    /// The parse of a module, from an earlier run while its text stays the same.
    fn parsed(&mut self, path: &Path, file: FileId) -> (Arc<Parse>, Arc<ModuleSyntax>) {
        let text = &self.sources.get(file).text;
        if let Some(found) = self.cache.as_ref().and_then(|c| c.parsed(path, file, text)) {
            return found;
        }
        let parse = Arc::new(syntax::parse(text, file).into_module());
        let module = Arc::new(resolve::module_syntax(&parse.tree, text, file));
        if let Some(cache) = &mut self.cache {
            let parsed = (parse.clone(), module.clone());
            cache.keep_parsed(path, file, text, parsed);
        }
        (parse, module)
    }
}
