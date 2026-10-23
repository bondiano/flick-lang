//! Checking a set of modules together: loads the given files and, through
//! their imports, every module they depend on; checks the module ↔ path
//! mapping, name collisions and import cycles, then resolves each module
//! against the interfaces of its imports.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

mod entry;
mod ide;

pub use ide::{Analysis, Analyzed, analysis_fingerprint, analyze};

use super::packages::Packages;
use super::{Host, Project, module_name, module_path, normalize};
use crate::diagnostics::{Diagnostic, Edit, Severity, codes};
use crate::ir;
use crate::resolve::{self, Interface, ModuleSyntax};
use crate::source::{FileId, SourceMap, Span};
use crate::syntax::{self, Parse};
use crate::types;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Root {
    Source,
    Test,
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
}

/// How far a run goes past checking.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Check,
    /// Also lowers every module to the IR when nothing has errors.
    Compile,
    /// [`Mode::Compile`], keeping what editors need of each module.
    Analyze,
}

struct Loader<'a> {
    host: &'a dyn Host,
    sources: &'a mut SourceMap,
    project: Option<&'a Project>,
    packages: Packages,
    modules: Vec<Loaded>,
    by_name: BTreeMap<String, usize>,
    from_packages: BTreeMap<String, Interface>,
    /// The package that provides each module of `from_packages`.
    owners: BTreeMap<String, String>,
    diagnostics: Vec<Diagnostic>,
    /// With `Some`, the IR of each module that checks without errors.
    lowered: Option<Vec<Option<ir::Module>>>,
    /// With `Some`, what editors need of each module that parses.
    ide: Option<Vec<Analyzed>>,
}

/// The modules of a build and the package modules they use.
#[derive(Default)]
pub struct Compilation {
    pub modules: Vec<Compiled>,
    /// Modules from installed packages: the module, its package, its
    /// interface and the import specifier of its compiled JS without the
    /// extension.
    pub packages: Vec<(String, String, Interface, String)>,
}

/// A module that checked cleanly, lowered to the IR.
pub struct Compiled {
    pub name: String,
    pub path: PathBuf,
    pub file: FileId,
    /// Whether it is in the test root.
    pub test: bool,
    /// The modules it imports.
    pub imports: Vec<String>,
    pub ir: ir::Module,
    pub interface: Interface,
}

/// Checks `roots` (absolute `.fl` paths) and their transitive imports.
pub fn check(
    host: &dyn Host,
    sources: &mut SourceMap,
    project: Option<&Project>,
    roots: &[PathBuf],
) -> Vec<Diagnostic> {
    run(host, sources, project, roots, Mode::Check).0
}

/// [`check`], also lowering every module to the IR when nothing has errors.
pub fn compile(
    host: &dyn Host,
    sources: &mut SourceMap,
    project: Option<&Project>,
    roots: &[PathBuf],
) -> (Vec<Diagnostic>, Compilation) {
    let (diagnostics, compilation, _) = run(host, sources, project, roots, Mode::Compile);
    (diagnostics, compilation)
}

fn run(
    host: &dyn Host,
    sources: &mut SourceMap,
    project: Option<&Project>,
    roots: &[PathBuf],
    mode: Mode,
) -> (Vec<Diagnostic>, Compilation, Vec<Analyzed>) {
    let mut diagnostics = Vec::new();
    let packages = project.map_or_else(Packages::default, |p| {
        Packages::load(host, &p.root, &mut diagnostics)
    });
    let mut loader = Loader {
        host,
        sources,
        project,
        packages,
        modules: Vec::new(),
        by_name: BTreeMap::new(),
        from_packages: BTreeMap::new(),
        owners: BTreeMap::new(),
        diagnostics,
        lowered: (mode != Mode::Check).then(Vec::new),
        ide: (mode == Mode::Analyze).then(Vec::new),
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
            loader
                .diagnostics
                .push(Diagnostic::new(codes::P001, Severity::Error, message));
        }
    }
    let edges = loader.load_imports();
    loader.import_cycles(&edges);
    let stale = super::packages::stale(&loader.from_packages, &loader.owners);
    loader.diagnostics.extend(stale);
    if let Some(lowered) = &mut loader.lowered {
        lowered.resize_with(loader.modules.len(), || None);
    }
    let interfaces = loader.resolve_all(&edges);
    loader.check_entry(&interfaces);
    let failed = loader
        .diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error);
    let compiled = match loader.lowered.take() {
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
                    imports: m.syntax.imports.iter().map(|i| i.module.clone()).collect(),
                    ir: ir?,
                    interface: interface?,
                })
            })
            .collect(),
        _ => Vec::new(),
    };
    let packages = loader
        .owners
        .iter()
        .filter_map(|(m, p)| {
            let interface = loader.from_packages.get(m)?.clone();
            let js = loader.packages.js_specifier(p, &m.replace('.', "/"));
            Some((m.clone(), p.clone(), interface, js))
        })
        .collect();
    let compilation = Compilation {
        modules: compiled,
        packages,
    };
    let analyzed = loader.ide.take().unwrap_or_default();
    let mut diagnostics = loader.diagnostics;
    // by path, not by load order, which follows the order of imports
    let sources = &*loader.sources;
    diagnostics.sort_by_cached_key(|d| {
        (
            d.span.is_none(),
            d.span
                .map(|s| (sources.get(s.file).path.clone(), s.start, s.end)),
        )
    });
    (diagnostics, compilation, analyzed)
}

impl Loader<'_> {
    fn root_of(&self, path: &Path) -> Root {
        match self.project {
            Some(p) if path.starts_with(p.source_root()) => Root::Source,
            Some(p) if path.starts_with(p.test_root()) => Root::Test,
            _ => Root::Other(None),
        }
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
                .with_fix(
                    format!(
                        "rename the module, or move the file to `{}`",
                        root_dir.as_ref().map_or(module_path(&h.name), |d| PathBuf::from(self.host.display(&d.join(module_path(&h.name))))).display()
                    ),
                    vec![Edit { span: h.span, replacement: expected.clone() }],
                ),
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
                .with_fix("rename the file after its module, e.g. `src/App/Users.fl` for `App.Users`", Vec::new()),
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
        let dirs: Vec<PathBuf> = match (&self.modules[importer].root, self.project) {
            (Root::Source, Some(p)) => vec![p.source_root()],
            (Root::Test, Some(p)) => vec![p.source_root(), p.test_root()],
            (Root::Other(dir), _) => dir.iter().cloned().collect(),
            _ => Vec::new(),
        };
        if !self.by_name.contains_key(name) {
            for dir in dirs {
                // absent files are simply not candidates
                let _ = self.load(&dir.join(module_path(name)));
            }
        }
        let local = self.by_name.get(name).copied();
        if !self.from_packages.contains_key(name) {
            let mut found = self.packages.find(self.host, name, &mut self.diagnostics);
            if let Some((package, interface)) = found.pop() {
                if let Some(local) = local {
                    let header = self.modules[local].syntax.header.as_ref();
                    self.diagnostics.push(Diagnostic::error(
                        codes::R002,
                        format!("module `{name}` is also provided by package `{package}`"),
                        header.map_or(Span::new(self.modules[local].file, 0, 0), |h| h.span),
                    ));
                }
                if let Some((other, _)) = found.first() {
                    self.diagnostics.push(Diagnostic::error(
                        codes::R002,
                        format!("module `{name}` is provided by both `{other}` and `{package}`"),
                        span,
                    ));
                }
                self.owners.insert(name.to_owned(), package);
                self.from_packages.insert(name.to_owned(), interface);
            }
        }
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

    fn unknown_module(&self, name: &str, span: Span) -> Diagnostic {
        let missing = self.packages.missing();
        if missing.is_empty() {
            let local = module_path(name);
            return Diagnostic::error(codes::R003, format!("unknown module `{name}`"), span).with_fix(
                format!(
                    "a local module goes in `{}`; if it comes from an npm package, add it with `flick add <package>`",
                    self.project.map_or(local.clone(), |p| Path::new(&p.config.source_dir).join(&local)).display()
                ),
                Vec::new(),
            );
        }
        let list = missing
            .iter()
            .map(|m| format!("`{m}`"))
            .collect::<Vec<_>>()
            .join(", ");
        Diagnostic::error(
            codes::P009,
            format!("cannot find module `{name}`; dependencies in package.json are not installed: {list}"),
            span,
        )
        .with_fix("install them with `flick install`", Vec::new())
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
                .with_fix(
                    "move what the modules share into a module that both import",
                    Vec::new(),
                ),
            );
        }
    }

    /// Resolves every module after the modules it imports; an import that
    /// closes a cycle sees no interface.
    fn resolve_all(&mut self, edges: &[Vec<usize>]) -> Vec<Option<Interface>> {
        let mut interfaces: Vec<Option<Interface>> = vec![None; self.modules.len()];
        let mut visiting = vec![false; self.modules.len()];
        for i in 0..self.modules.len() {
            self.resolve_one(i, edges, &mut interfaces, &mut visiting);
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
        let loaded = &self.modules[i];
        let module = resolve::Module {
            name: &loaded.name,
            file: loaded.file,
            text: &self.sources.get(loaded.file).text,
            tree: &loaded.parse.tree,
            syntax: &loaded.syntax,
        };
        let lookup = |name: &str| match self.by_name.get(name) {
            Some(&j) => interfaces[j].clone(),
            None => self.from_packages.get(name).cloned(),
        };
        let (mut interface, diagnostics) = resolve::resolve(&module, &lookup);
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
        };
        self.diagnostics
            .extend(loaded.parse.diagnostics.iter().cloned());
        let resolved = !diagnostics.iter().any(|d| d.severity == Severity::Error);
        self.diagnostics.extend(diagnostics);
        // a module with syntax errors keeps untyped exports: importers trust them
        if loaded.parse.diagnostics.is_empty() {
            let (checked, typing) =
                types::check_typed(&module, &lookup, types::builtins(), options, &mut interface);
            let clean = resolved && !checked.iter().any(|d| d.severity == Severity::Error);
            self.diagnostics.extend(checked);
            if clean && let Some(lowered) = &mut self.lowered {
                match ir::lower(&module, &typing) {
                    Ok(ir) => lowered[i] = Some(ir),
                    Err(d) => self.diagnostics.extend(d),
                }
            }
            if let Some(ide) = &mut self.ide {
                ide.push(Analyzed::new(loaded, typing, interface.clone()));
            }
        }
        interfaces[i] = Some(interface);
        visiting[i] = false;
    }
}
