//! What the server knows of a file: the analysis of its project, or of the
//! built-in module it is, over the documents open in the editor.

use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io;
use std::path::{MAIN_SEPARATOR, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};
use std::time::Instant;

#[cfg(feature = "wasm-backends")]
use flick_core::diagnostics::Diagnostic;
use flick_core::ide::Ide;
use flick_core::plugin::Expander;
use flick_core::plugin::reach::Outside;
#[cfg(feature = "wasm-backends")]
use flick_core::project::Foreign;
#[cfg(feature = "wasm-backends")]
use flick_core::project::ResolvedEnvironment;
#[cfg(feature = "wasm-backends")]
use flick_core::project::config::BackendEnvironment;
#[cfg(feature = "wasm-backends")]
use flick_core::project::target::Wanted;
use flick_core::project::{
    self, Analysis, Analyzed, BackendTags, Compilation, Host, ModuleCache, Project,
};
use flick_core::source::{FileId, SourceMap};
use flick_core::syntax::cst::Node;
use flick_core::syntax::kind::SyntaxKind as K;
use flick_core::types::{ProofSearchRequest, ProofSearchStatus};
use flick_core::{resolve, syntax, types};
use tracing::{debug, debug_span};

#[cfg(feature = "wasm-backends")]
use crate::backends;
use crate::backends::options;
use crate::{plugins, sources};

pub(super) static BUILTIN_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    let temp = env::temp_dir();
    temp.canonicalize().unwrap_or(temp).join("flick-lsp")
});

pub(super) fn canonical(path: &Path) -> PathBuf {
    path.canonicalize()
        .unwrap_or_else(|_| match (path.parent(), path.file_name()) {
            (Some(parent), Some(name)) => canonical(parent).join(name),
            _ => project::normalize(path),
        })
}

/// Files read from the open buffers first.
pub(super) struct Overlay<'a> {
    docs: HashMap<PathBuf, (&'a Path, &'a str)>,
}

pub(super) fn overlay(docs: &HashMap<PathBuf, String>) -> Overlay<'_> {
    Overlay {
        docs: docs
            .iter()
            .map(|(path, text)| (canonical(path), (path.as_path(), text.as_str())))
            .collect(),
    }
}

impl Host for Overlay<'_> {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        match self
            .docs
            .get(path)
            .or_else(|| self.docs.get(&canonical(path)))
        {
            Some((_, text)) => Ok(text.as_bytes().to_vec()),
            None => fs::read(path),
        }
    }

    fn display(&self, path: &Path) -> String {
        self.docs
            .get(path)
            .or_else(|| self.docs.get(&canonical(path)))
            .map_or(path, |(original, _)| *original)
            .display()
            .to_string()
    }

    fn plugin(&self, package: &str, component: Option<&Path>) -> Result<Arc<dyn Expander>, String> {
        plugins::plugin(package, component)
    }

    fn cached(&self, key: &[u8]) -> Option<Vec<u8>> {
        plugins::cached(key)
    }

    fn save(&self, key: &[u8], value: &[u8]) {
        plugins::save(key, value);
    }

    /// The editor reaches no network: an answer comes from a snapshot.
    fn outside(&self) -> Option<Arc<dyn Outside>> {
        Some(plugins::outside(true))
    }

    #[cfg(feature = "wasm-backends")]
    fn foreign(&self, project: &Project) -> Foreign {
        backends::foreign(project)
    }

    #[cfg(feature = "wasm-backends")]
    fn verify(&self, project: &Project, wanted: &[Wanted]) -> Vec<Diagnostic> {
        backends::verify(project, wanted, false)
    }

    #[cfg(feature = "wasm-backends")]
    fn stages_by_class(&self, project: &Project) -> bool {
        backends::stages_by_class(project)
    }

    #[cfg(feature = "wasm-backends")]
    fn handles_service(
        &self,
        project: &Project,
        environment: &BackendEnvironment,
        service: &str,
    ) -> Option<bool> {
        backends::handles_service(project, environment, service)
    }

    fn backend_tags(&self, project: &Project) -> Vec<BackendTags> {
        options::project_backend_tags(project, true)
    }

    #[cfg(feature = "wasm-backends")]
    fn check_target(
        &self,
        project: &Project,
        compilation: &Compilation,
        sources: &SourceMap,
        environment: &ResolvedEnvironment,
        stages_by_class: bool,
    ) -> Vec<Diagnostic> {
        backends::check_target_for_project(
            project,
            compilation,
            sources,
            environment,
            stages_by_class,
        )
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub enum Context {
    Project(PathBuf),
    Standalone(PathBuf),
    Builtin(PathBuf),
}

impl Context {
    pub(super) fn of(docs: &HashMap<PathBuf, String>, path: &Path) -> Self {
        if builtin_index(path).is_some() {
            return Self::Builtin(canonical(path));
        }
        for dir in path.parent().unwrap_or(Path::new("/")).ancestors() {
            let config = dir.join(project::CONFIG_FILE);
            if docs.contains_key(&config) || config.is_file() {
                return Self::Project(canonical(dir));
            }
        }
        Self::Standalone(canonical(path))
    }
}

/// One analysis and the sources it points into.
pub struct Snapshot {
    pub analysis: Analysis,
    pub sources: SourceMap,
    /// The project's root directory.
    #[cfg_attr(not(feature = "wasm-backends"), allow(dead_code))]
    pub root: PathBuf,
    /// Alternate companion file extension for local foreign bindings, if any.
    pub foreign_extension: Option<String>,
    pub(super) context: Context,
    pub(super) generation: u64,
}

impl Snapshot {
    /// An analyzed module (including aliases) or another source, such as a manifest.
    pub(super) fn reads(&self, path: &Path) -> bool {
        self.sources
            .files()
            .any(|source| Path::new(&source.path) == path)
            || self.file(path).is_some()
    }

    pub fn file(&self, path: &Path) -> Option<FileId> {
        self.analysis
            .modules
            .iter()
            .find(|m| m.path == path)
            .or_else(|| {
                let path = canonical(path);
                self.analysis
                    .modules
                    .iter()
                    .find(|m| canonical(&m.path) == path)
            })
            .map(|m| m.file)
    }

    pub(super) fn drop_compilation(&mut self) {
        self.analysis.compilation = Compilation::default();
    }
}

/// What each context kept of its earlier analyses.
pub type Caches = HashMap<Context, ModuleCache>;

/// Analyzes the project of `path` with `docs` over the files, reusing the
/// modules that did not change since the context's earlier analyses.
pub fn analyze(docs: &HashMap<PathBuf, String>, path: &Path, caches: &mut Caches) -> Snapshot {
    let started = Instant::now();
    let _span =
        debug_span!("analysis", path = %path.display(), open_documents = docs.len()).entered();
    if let Some(index) = builtin_index(path) {
        let snapshot = analyze_builtin(&canonical(path), index);
        debug!(
            modules = snapshot.analysis.modules.len(),
            diagnostics = snapshot.analysis.diagnostics.len(),
            elapsed = ?started.elapsed(),
            "built-in analysis completed"
        );
        return snapshot;
    }
    let host = overlay(docs);
    let mut sources = SourceMap::default();
    let dir = path
        .parent()
        .map_or_else(|| PathBuf::from("/"), Path::to_path_buf);
    // a manifest that does not read says why
    let (found, unread) = match project::discover(&host, &mut sources, &dir, None) {
        Ok(found) => (found, None),
        Err(why) => (None, Some(why)),
    };
    let context = Context::of(docs, path);
    let mut roots: Vec<PathBuf> = Vec::new();
    let directories = found
        .as_ref()
        .map(|p| [p.source_root(), p.test_root(), p.proof_root()]);
    let skip = sources::skip_dirs(found.as_ref());
    if let Some(directories) = &directories {
        for root in directories {
            let mut files = Vec::new();
            if root.is_dir() {
                sources::source_files_skipping(&root.to_string_lossy(), &skip, &mut files);
            }
            roots.extend(files.into_iter().map(PathBuf::from));
        }
    }
    roots.extend(
        docs.keys()
            .filter(|p| p.extension().is_some_and(|e| e == "fl"))
            .cloned(),
    );
    if path.extension().is_some_and(|e| e == "fl") {
        roots.push(path.to_path_buf());
    }
    roots.retain(|root| Context::of(docs, root) == context);
    // Aliases of one project must keep the same source/test module paths.
    if let Some(directories) = &directories {
        let physical = directories.each_ref().map(|dir| canonical(dir));
        for root in &mut roots {
            let path = canonical(root);
            for (dir, physical) in directories.iter().zip(&physical) {
                if let Ok(relative) = path.strip_prefix(physical) {
                    *root = dir.join(relative);
                    break;
                }
            }
        }
    }
    roots.sort();
    let mut seen = HashSet::new();
    roots.retain(|root| seen.insert(canonical(root)));
    let cache = caches.entry(context.clone()).or_default();
    let mut analysis = project::analyze_with(&host, &mut sources, found.as_ref(), &roots, cache);
    analysis.diagnostics.extend(unread);
    #[cfg(feature = "wasm-backends")]
    let foreign_extension = found.as_ref().and_then(backends::foreign_extension);
    #[cfg(not(feature = "wasm-backends"))]
    let foreign_extension = None;
    let root = found.map_or(dir, |p| p.root);
    debug!(
        root = %root.display(),
        modules = analysis.modules.len(),
        diagnostics = analysis.diagnostics.len(),
        elapsed = ?started.elapsed(),
        "analysis completed"
    );
    Snapshot {
        analysis,
        sources,
        root,
        foreign_extension,
        context,
        generation: 0,
    }
}

pub(super) struct ProofSearchResult {
    pub status: &'static str,
    pub suggestions: Vec<(String, (u32, u32))>,
}

impl ProofSearchResult {
    fn empty(status: &'static str) -> Self {
        Self {
            status,
            suggestions: Vec::new(),
        }
    }
}

/// Reruns the current overlays on the search lane with a bounded search at
/// just the selected obligation.
pub(super) fn search_proof(
    docs: &HashMap<PathBuf, String>,
    path: &Path,
    offset: u32,
    cancelled: &Arc<AtomicBool>,
) -> ProofSearchResult {
    if cancelled.load(Ordering::Relaxed) {
        return ProofSearchResult::empty("cancelled");
    }
    let Some(text) = docs.get(path) else {
        return ProofSearchResult::empty("no-candidate");
    };
    let parsed = syntax::parse(text, FileId::default());
    let Some(decl) = search_declaration(&parsed.tree, text, offset) else {
        return ProofSearchResult::empty("no-candidate");
    };
    let request = ProofSearchRequest {
        declaration: decl.range(),
        offset,
        cancelled: Arc::clone(cancelled),
    };
    let context = Context::of(docs, path);
    let mut caches = Caches::from([(
        context,
        ModuleCache::for_proof_search(canonical(path), request),
    )]);
    let snapshot = analyze(docs, path, &mut caches);
    if cancelled.load(Ordering::Relaxed) {
        return ProofSearchResult::empty("cancelled");
    }
    let ide = Ide {
        analysis: &snapshot.analysis,
        sources: &snapshot.sources,
    };
    let result = snapshot.file(path).and_then(|file| {
        if ide.ordinary_value_in_proof(file, offset) {
            return None;
        }
        snapshot
            .analysis
            .by_file(file)?
            .typing
            .proof_evidence
            .iter()
            .find(|proof| proof.at == decl.range())?
            .search
            .as_ref()
    });
    let Some(result) = result else {
        return ProofSearchResult::empty("no-candidate");
    };
    debug!(
        candidates = result.candidates,
        normalization_steps = result.normalization_steps,
        ?result.status,
        "proof search finished",
    );
    let status = match result.status {
        ProofSearchStatus::Found => "found",
        ProofSearchStatus::NoCandidate => "no-candidate",
        ProofSearchStatus::BudgetExhausted => "budget-exhausted",
        ProofSearchStatus::Cancelled => "cancelled",
    };
    let range = result
        .span
        .map_or((offset, offset), |span| (span.start, span.end));
    let range = search_range(decl, text, offset, range);
    let suggestions = range.map_or_else(Vec::new, |(range, nested)| {
        let prefix = if range.0 == range.1 {
            let before = &text[..range.0 as usize];
            let line = before.rsplit_once('\n').map_or(before, |(_, line)| line);
            if line.ends_with('=') {
                " "
            } else if line.is_empty() {
                "  "
            } else {
                ""
            }
        } else {
            ""
        };
        result
            .suggestions
            .iter()
            .map(|term| {
                let text = if !prefix.is_empty() {
                    format!("{prefix}{term}")
                } else if nested && term.contains(char::is_whitespace) {
                    format!("({term})")
                } else {
                    term.clone()
                };
                (text, range)
            })
            .collect()
    });
    let status = if status == "found" && suggestions.is_empty() {
        "no-candidate"
    } else {
        status
    };
    ProofSearchResult {
        status,
        suggestions,
    }
}

fn search_declaration<'a>(tree: &'a Node, text: &str, at: u32) -> Option<&'a Node> {
    let decl = tree
        .nodes()
        .take_while(|node| node.range().0 <= at)
        .last()?;
    if decl.kind != K::ProofDecl {
        return None;
    }
    let equal = decl.child_tokens().find(|token| token.kind == K::Eq)?;
    if at < equal.end {
        return None;
    }
    let end = decl.range().1;
    (at <= end
        || text
            .get(end as usize..at as usize)
            .is_some_and(|gap| gap.trim().is_empty()))
    .then_some(decl)
}

/// The edit range and whether it belongs to a term inside the body. An
/// application inserted as an argument needs parentheses to stay one term.
fn search_range(
    decl: &Node,
    text: &str,
    at: u32,
    (start, end): (u32, u32),
) -> Option<((u32, u32), bool)> {
    let body = decl.nodes().find(|node| node.kind == K::Block);
    if let Some(body) = body.filter(|body| body.nodes().next().is_some()) {
        let (from, to) = body.range();
        (from <= start && start <= at && at <= end && end <= to)
            .then_some(((start, end), (start, end) != (from, to)))
    } else {
        let equal = decl.child_tokens().find(|token| token.kind == K::Eq)?;
        let gap = text.get(equal.end as usize..at as usize)?;
        gap.trim().is_empty().then_some(((at, at), false))
    }
}

pub(super) fn builtin_index(path: &Path) -> Option<usize> {
    if path.extension()? != "fl" {
        return None;
    }
    let path = canonical(path);
    let relative = path.strip_prefix(&*BUILTIN_DIR).ok()?;
    let mut parts = relative.components();
    parts.next()?;
    let module = parts.as_path().with_extension("");
    let module = module.to_str()?.replace(MAIN_SEPARATOR, ".");
    types::builtin_sources()
        .iter()
        .position(|(name, _)| *name == module)
}

/// Check library sources in the compiler's dependency order, not as project
/// roots importing their own already-registered instances.
pub(super) fn analyze_builtin(path: &Path, index: usize) -> Snapshot {
    let (name, text) = types::builtin_sources()[index];
    let dependencies = &types::builtins()[..index];
    let mut sources = SourceMap::default();
    let file = sources.add(path.to_string_lossy(), text);
    let parsed = syntax::parse(text, file);
    let module_syntax = resolve::module_syntax(&parsed.tree, text, file);
    let module = resolve::Module {
        name,
        file,
        text,
        tree: &parsed.tree,
        syntax: &module_syntax,
        generated: resolve::NO_GENERATED,
    };
    let imported = |name: &str| dependencies.iter().find(|i| i.module == name).cloned();
    let (mut interface, mut diagnostics) = resolve::resolve(&module, &imported);
    diagnostics.extend(parsed.diagnostics);
    let local = |name: &str| types::builtin_sources().iter().any(|(n, _)| *n == name);
    let options = types::Options {
        local: &local,
        limits: types::Limits::default(),
        no_let_assert: false,
        thresholds: types::Thresholds::default(),
        watched: types::Watched::none(),
        stages_by_class: false,
        foreign_backend: types::ForeignBackendSpec::default_js(),
        validate_foreign_metadata: true,
        native_overrides: None,
    };
    let (checked, typing) =
        types::check_builtin_typed(&module, &imported, dependencies, options, &mut interface);
    diagnostics.extend(checked);
    Snapshot {
        analysis: Analysis {
            modules: vec![Analyzed {
                inline: false,
                name: name.to_owned(),
                path: path.to_path_buf(),
                file,
                test: false,
                package: None,
                tree: parsed.tree,
                typing: typing.into(),
                interface,
                editing: Vec::new(),
                expansion: None,
            }],
            compilation: Compilation::default(),
            diagnostics,
            foreign_backend: types::ForeignBackendSpec::js(),
        },
        sources,
        root: path.parent().expect("source directory").to_path_buf(),
        foreign_extension: None,
        context: Context::Builtin(path.to_path_buf()),
        generation: 0,
    }
}
