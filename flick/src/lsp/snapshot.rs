//! What the server knows of a file: the analysis of its project, or of the
//! built-in module it is, over the documents open in the editor.

use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io;
use std::path::{MAIN_SEPARATOR, Path, PathBuf};
use std::sync::{Arc, LazyLock};
use std::time::Instant;

use flick_core::plugin::Expander;
use flick_core::plugin::reach::Outside;
use flick_core::project::config::Emit;
use flick_core::project::{self, Analysis, Analyzed, Compilation, Host, ModuleCache};
use flick_core::source::{FileId, SourceMap};
use flick_core::{resolve, syntax, types};
use tracing::{debug, debug_span};

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
    /// The project's root, where `node_modules` is.
    pub root: PathBuf,
    /// The target selected by the project's build configuration.
    pub emit: Emit,
    pub(super) context: Context,
}

impl Snapshot {
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
    let directories = found.as_ref().map(|p| [p.source_root(), p.test_root()]);
    if let Some(directories) = &directories {
        for root in directories {
            let mut files = Vec::new();
            if root.is_dir() {
                sources::source_files(&root.to_string_lossy(), &mut files);
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
    let emit = found.as_ref().map_or(Emit::Js, |p| p.config.build.emit);
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
        emit,
        context,
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
        effects: true,
        watched: types::Watched::none(),
        stages_by_class: false,
        regions: types::RegionMode::Captures,
    };
    let (checked, typing) =
        types::check_builtin_typed(&module, &imported, dependencies, options, &mut interface);
    diagnostics.extend(checked);
    Snapshot {
        analysis: Analysis {
            modules: vec![Analyzed {
                name: name.to_owned(),
                path: path.to_path_buf(),
                file,
                test: false,
                tree: parsed.tree,
                typing: typing.into(),
                interface,
                editing: Vec::new(),
                expansion: None,
            }],
            compilation: Compilation::default(),
            diagnostics,
        },
        sources,
        root: path.parent().expect("source directory").to_path_buf(),
        emit: Emit::Js,
        context: Context::Builtin(path.to_path_buf()),
    }
}
