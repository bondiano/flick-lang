//! Checking a set of modules together: loads the given files and, through
//! their imports, every module they depend on; checks the module ↔ path
//! mapping, name collisions and import cycles, then resolves each module
//! against the interfaces of its imports.

use std::collections::{BTreeMap, BTreeSet};
use std::io;
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
mod output;
mod plugins;
// the plugin of a module is its package's, which needs packages
#[cfg(all(test, feature = "native-project"))]
mod plugins_tests;
mod precompiled;
mod runner;
mod target;
mod unknown;

pub use checking::ModuleCache;
pub(crate) use checking::check_file;
#[cfg(feature = "native-project")]
pub use checking::{ModuleStore, SavedModule};
pub use entry::{MainKind, host_services, main_kind};
pub use format_plugins::FormatPlugins;
#[cfg(feature = "ide")]
pub use ide::{Analysis, Analyzed, analysis_fingerprint, analyze, analyze_rt, analyze_with};
pub use output::{Compilation, Compiled};
pub use precompiled::{CachedModule, Fresh, PackageCache};
pub use target::stages_by_class;

use super::environment::{self, TargetOverride};
#[cfg(feature = "native-project")]
use super::packages::{Packages, project_name};
use super::{Host, Project, module_path, normalize};
use crate::diagnostics::{Diagnostic, Edit, Severity, codes};
use crate::iface::{
    BindingOrigin, CalleeOrigin, ExpansionFrame, LexicalScope, StaticArgument, StaticBinding,
    StaticCapture, StaticMember,
};
use crate::ir;
use crate::plugin::EditorBlock;
use crate::plugin::expansion::Expansion;
use crate::resolve::{Interface, ModuleSyntax};
use crate::source::{FileId, SourceMap, Span};
use crate::syntax::Parse;
#[cfg(feature = "ide")]
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
    #[cfg(feature = "ide")]
    foreign_backend: types::ForeignBackendSpec,
}

struct Loader<'a> {
    expression_serial: usize,
    expressions_active: BTreeSet<String>,
    host: &'a dyn Host,
    sources: &'a mut SourceMap,
    project: Option<&'a Project>,
    /// The mode every module of the run and the built-in modules are checked in.
    stages_by_class: bool,
    /// Resolved environment context, including active and known backend foreign metadata.
    env_ctx: &'a environment::EnvironmentContext,
    #[cfg(not(target_arch = "wasm32"))]
    package_when: &'a BTreeMap<String, String>,
    #[cfg(not(target_arch = "wasm32"))]
    own_package_when: Option<&'a str>,
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
}

/// A module's IR and what the reference model reads of it.
type Lowered = (ir::Module, Arc<Interface>, Vec<ir::Proven>);

use output::compiled;

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
/// giving those to save; `rt` overrides whether stages follow class derivation
/// (`Some(true)` for RT, `Some(false)` for Effect/Go, `None` for the project
/// default); a command that builds again and again keeps what it compiled in
/// `cache`.
pub fn compile_with(
    host: &dyn Host,
    sources: &mut SourceMap,
    project: Option<&Project>,
    roots: &[PathBuf],
    packages: Option<&PackageCache>,
    rt: Option<bool>,
    cache: Option<&mut ModuleCache>,
) -> (Vec<Diagnostic>, Compilation, Vec<Fresh>) {
    let target = rt.map(|rt| TargetOverride {
        rt: Some(rt),
        ..TargetOverride::default()
    });
    compile_for(
        host,
        sources,
        project,
        roots,
        packages,
        target.as_ref(),
        cache,
    )
}

/// [`compile_with`], parameterized by a target/option override (from CLI
/// `--target`, resolved backend options, loaded descriptor tags, or test mode).
pub fn compile_for(
    host: &dyn Host,
    sources: &mut SourceMap,
    project: Option<&Project>,
    roots: &[PathBuf],
    packages: Option<&PackageCache>,
    target: Option<&TargetOverride>,
    cache: Option<&mut ModuleCache>,
) -> (Vec<Diagnostic>, Compilation, Vec<Fresh>) {
    let mode = Mode::Compile;
    let output = run_in(host, sources, project, roots, mode, cache, packages, target);
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
    run_in(host, sources, project, roots, mode, cache, saved, None)
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
    target_override: Option<&TargetOverride>,
) -> Output {
    let rt = target_override.and_then(|o| o.rt);
    let stages_by_class = stages_by_class(host, project, rt);
    let mut diagnostics = Vec::new();
    #[cfg(feature = "native-project")]
    let packages = project.map_or_else(Packages::default, |p| {
        Packages::load(host, sources, p, &mut diagnostics)
    });
    #[cfg(feature = "native-project")]
    let target_dependencies =
        project.map_or_else(Vec::new, |p| super::target::closure(p, &packages));
    #[cfg(feature = "native-project")]
    if let Some(project) = project {
        diagnostics.extend(super::target::conflicts(&target_dependencies));
        diagnostics.extend(host.verify(project, &target_dependencies));
    }
    #[cfg(feature = "native-project")]
    let package_when: BTreeMap<String, String> = packages
        .package_when()
        .map(|(name, cond, _)| (name.to_owned(), cond.to_owned()))
        .collect();
    #[cfg(all(not(feature = "native-project"), not(target_arch = "wasm32")))]
    let package_when = BTreeMap::new();
    #[cfg(not(target_arch = "wasm32"))]
    let own_package_when = project
        .and_then(|p| p.config.package.as_ref())
        .and_then(|p| Some((p.name.clone(), p.when.as_ref()?.0.clone())));
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
    let precompiled = Precompiled::default();
    #[cfg(not(target_arch = "wasm32"))]
    let environment_preflight = project.is_some_and(|p| p.config.has_selected_target())
        || target_override.is_some_and(|o| o.target.is_some() || !o.options.is_empty());
    #[cfg(target_arch = "wasm32")]
    let environment_preflight = false;
    let env_ctx = environment::resolve(
        host,
        project,
        #[cfg(feature = "native-project")]
        &packages,
        #[cfg(feature = "native-project")]
        &target_dependencies,
        target_override,
        environment_preflight,
        sources,
        &mut diagnostics,
    );
    // a check lowers the modules of source packages only to save them;
    // a selected target or environment also needs IR for reachable-use preflight
    let lower_packages_only = mode == Mode::Check && saved.is_some() && !environment_preflight;
    #[cfg(feature = "ide")]
    let ide_foreign_backend = env_ctx.foreign_backend.clone();
    let mut loader = Loader {
        expression_serial: 0,
        expressions_active: BTreeSet::new(),
        host,
        sources,
        project,
        stages_by_class,
        env_ctx: &env_ctx,
        #[cfg(not(target_arch = "wasm32"))]
        package_when: &package_when,
        #[cfg(not(target_arch = "wasm32"))]
        own_package_when: own_package_when.as_ref().map(|(_, cond)| cond.as_str()),
        #[cfg(feature = "native-project")]
        packages,
        modules: Vec::new(),
        by_name: BTreeMap::new(),
        from_packages: BTreeMap::new(),
        diagnostics,
        lowered: (mode != Mode::Check || lower_packages_only || environment_preflight)
            .then(Vec::new),
        lower_packages_only,
        precompiled,
        #[cfg(feature = "ide")]
        ide: (mode == Mode::Analyze).then(Vec::new),
        cache,
        watchers: plugins::Watchers::default(),
        privates: BTreeMap::new(),
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
    loader.size_lowered();
    let interfaces = loader.resolve_all(&edges);
    if let Some(cache) = &mut loader.cache {
        // modules no longer loaded are dropped
        cache.retain_loaded(&loader.modules);
    }
    // Validate `@when` tag names on every loaded and cached module against the vocabulary.
    for m in &loader.modules {
        if let Some((cond, span)) = m.syntax.header.as_ref().and_then(|h| h.when.as_ref()) {
            let text = &loader.sources.get(span.file).text;
            environment::validate_module_when(
                cond,
                Some(*span),
                Some(text),
                &env_ctx,
                &mut loader.diagnostics,
            );
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    for iface in loader.from_packages.values() {
        if let Some(cond) = &iface.when {
            environment::validate_module_when(cond, None, None, &env_ctx, &mut loader.diagnostics);
        }
    }
    loader.check_entry(&interfaces);
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
                            types: Vec::new(),
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
    let fresh = loader.finish_packages(
        &mut compiled,
        mode != Mode::Check || environment_preflight,
        saved.is_some() && !failed,
    );
    #[cfg(not(target_arch = "wasm32"))]
    checking::relocate_inline_importers(&mut compiled);
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
        let own = own.and_then(|p| project_name(host, p));
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
        target_dependencies,
        #[cfg(not(target_arch = "wasm32"))]
        package_when,
        #[cfg(not(target_arch = "wasm32"))]
        own_package_when,
        #[cfg(not(target_arch = "wasm32"))]
        environment: env_ctx.resolved,
    };
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(resolved) = compilation.environment.as_ref() {
        target::check_environment(
            host,
            &compilation,
            project,
            target_override,
            stages_by_class,
            resolved,
            sources,
            &mut diagnostics,
        );
    }
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
        #[cfg(feature = "ide")]
        foreign_backend: ide_foreign_backend,
    }
}
