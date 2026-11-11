//! Modules of source packages that earlier runs checked and lowered. A host
//! keeps them under a key of everything their checking depends on and
//! hands them back; a run takes their interfaces and IR instead of parsing
//! and checking their sources again.

use std::collections::BTreeMap;
#[cfg(feature = "native-project")]
use std::collections::HashMap;
#[cfg(feature = "native-project")]
use std::hash::{DefaultHasher, Hash, Hasher};
#[cfg(feature = "native-project")]
use std::io::ErrorKind;
#[cfg(feature = "native-project")]
use std::mem;
use std::path::{Path, PathBuf};
#[cfg(feature = "native-project")]
use std::str;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Compiled, Loader, Mode};
#[cfg(feature = "native-project")]
use super::{Loaded, Root, Virtual, module_path};
#[cfg(feature = "native-project")]
use crate::diagnostics::{Diagnostic, Severity, codes};
#[cfg(feature = "native-project")]
use crate::ir;
#[cfg(feature = "native-project")]
use crate::project::Project;
#[cfg(feature = "native-project")]
use crate::project::cache::Key;
#[cfg(feature = "native-project")]
use crate::project::packages::{Library, Packages};
use crate::resolve::Interface;
#[cfg(feature = "native-project")]
use crate::source::Span;
#[cfg(feature = "native-project")]
use crate::source::{FileId, SourceMap};
#[cfg(feature = "native-project")]
use crate::{resolve, syntax};

/// A module of a source package as a run checked and lowered it.
#[derive(Clone, Serialize, Deserialize)]
pub struct CachedModule {
    pub name: String,
    /// Its path under the source directory of its package.
    pub path: PathBuf,
    pub imports: Vec<String>,
    pub interface: Interface,
    /// Every top-level entity with its type, for the reference model.
    declarations: Arc<Interface>,
    /// The IR with its spans in file 0; a run moves them to the module's file.
    ir: Value,
    /// A hash of the text it was checked from, which its spans point into.
    source: u64,
    /// Checked source for portable template instantiation and source-map
    /// locations when the immutable package's original files are unavailable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source_text: Option<String>,
}

/// What a host keeps of source packages between runs.
pub struct PackageCache<'a> {
    /// What checking any package depends on besides its sources and the
    /// project: the compiler and its `std`, as the host identifies them.
    pub salt: &'a str,
    /// What identifies the sources of the package in a source directory: the
    /// checksum `flick.lock` pins for a fetched one, else its files.
    pub identity: &'a dyn Fn(&Path, Option<&str>) -> Option<String>,
    /// The modules an earlier run saved under a key.
    pub saved: &'a dyn Fn(&str) -> Option<Vec<CachedModule>>,
}

/// The modules of a package after a run that checked some of them from
/// their sources, to save under the package's key.
pub struct Fresh {
    pub key: String,
    pub modules: Vec<CachedModule>,
}

/// The saved modules a run may use.
#[derive(Default)]
pub(super) struct Precompiled {
    /// The key and source directory of each package that has one.
    #[cfg(feature = "native-project")]
    packages: BTreeMap<String, (String, PathBuf)>,
    /// The saved modules by name, with their package.
    saved: BTreeMap<String, (String, CachedModule)>,
    /// The saved modules the run used, in the order it took them.
    #[cfg(feature = "native-project")]
    used: Vec<String>,
}

impl Precompiled {
    /// The libraries of `packages` with their keys and saved modules. A
    /// library's key covers the compiler, the configuration of the project,
    /// its sources and the keys of the libraries it depends on. A `--rt` build
    /// keeps the configuration of the project, so the mode counts on its own.
    #[cfg(feature = "native-project")]
    pub(super) fn new(
        packages: &Packages,
        project: &Project,
        cache: &PackageCache,
        stages_by_class: bool,
    ) -> Self {
        let libraries: HashMap<&str, _> = packages
            .libraries()
            .map(|(name, dir, library)| (name, (dir, library)))
            .collect();
        let config = format!("{:?} {stages_by_class}", project.config);
        let mut keys: HashMap<String, Option<String>> = HashMap::new();
        let mut out = Precompiled::default();
        for &name in libraries.keys() {
            if let Some(key) = key_of(name, &libraries, &config, cache, &mut keys) {
                let dir = libraries[name].0.to_path_buf();
                out.packages.insert(name.to_owned(), (key, dir));
            }
        }
        for (package, (key, _)) in &out.packages {
            for module in (cache.saved)(key).unwrap_or_default() {
                out.saved
                    .insert(module.name.clone(), (package.clone(), module));
            }
        }
        out
    }

    /// The package of the saved module `name`.
    pub(super) fn package_of(&self, name: &str) -> Option<&str> {
        self.saved.get(name).map(|(package, _)| package.as_str())
    }

    /// Whether the saved module `name` belongs to the package in `dir`.
    #[cfg(feature = "native-project")]
    fn covers(&self, dir: &Path, name: &str) -> bool {
        self.saved.get(name).is_some_and(|(package, _)| {
            self.packages
                .get(package)
                .is_some_and(|(_, at)| at.as_path() == dir)
        })
    }

    #[cfg(feature = "native-project")]
    /// What to save after a clean run: for each package with a module the
    /// run compiled from source, its saved modules and those.
    pub(super) fn fresh(
        &self,
        compiled: &[Compiled],
        sources: &SourceMap,
        expanded: &[&str],
    ) -> Vec<Fresh> {
        let mut by_package: BTreeMap<&str, BTreeMap<String, CachedModule>> = BTreeMap::new();
        for c in compiled {
            // shortcut: a package with plugin blocks is checked each time; its
            // key would also need its plugin, not only its sources
            let Some(package) = c.package.as_deref().filter(|p| !expanded.contains(p)) else {
                continue;
            };
            let Some((_, dir)) = self.packages.get(package) else {
                continue;
            };
            if let Some(module) = cached(c, dir, &sources.get(c.file).text) {
                by_package
                    .entry(package)
                    .or_default()
                    .insert(module.name.clone(), module);
            }
        }
        by_package
            .into_iter()
            .map(|(package, mut modules)| {
                for (owner, module) in self.saved.values() {
                    if owner == package {
                        modules
                            .entry(module.name.clone())
                            .or_insert_with(|| module.clone());
                    }
                }
                Fresh {
                    key: self.packages[package].0.clone(),
                    modules: modules.into_values().collect(),
                }
            })
            .collect()
    }
}

/// The key of library `name`, once for each library; `None` when its
/// sources or a library it depends on cannot be identified.
#[cfg(feature = "native-project")]
fn key_of(
    name: &str,
    libraries: &HashMap<&str, (&Path, &Library)>,
    config: &str,
    cache: &PackageCache,
    keys: &mut HashMap<String, Option<String>>,
) -> Option<String> {
    if let Some(known) = keys.get(name) {
        return known.clone();
    }
    // a cycle of libraries gets no key
    keys.insert(name.to_owned(), None);
    let &(dir, library) = libraries.get(name)?;
    let mut key = Key::default();
    key.add("salt", cache.salt);
    key.add("config", config);
    key.add("name", name);
    key.add(
        "sources",
        (cache.identity)(dir, library.checksum.as_deref())?,
    );
    for dependency in &library.dependencies {
        let found = key_of(dependency, libraries, config, cache, keys)?;
        key.add(format!("dependency {dependency}"), found);
    }
    let key = key.finish();
    keys.insert(name.to_owned(), Some(key.clone()));
    Some(key)
}

#[cfg(feature = "native-project")]
/// `compiled`, checked from `text`, to save, unless its IR has spans in
/// another file.
fn cached(compiled: &Compiled, dir: &Path, text: &str) -> Option<CachedModule> {
    let mut ir = serde_json::to_value(&compiled.ir).ok()?;
    let own = serde_json::to_value(compiled.file).ok()?;
    let zero = serde_json::to_value(FileId::default()).ok()?;
    if !move_spans(&mut ir, Some(&own), &zero) {
        return None;
    }
    Some(CachedModule {
        name: compiled.name.clone(),
        path: compiled.path.strip_prefix(dir).ok()?.to_path_buf(),
        imports: compiled.imports.clone(),
        interface: compiled.interface.clone(),
        declarations: compiled.declarations.clone(),
        ir,
        source: hash(text),
        source_text: compiled
            .interface
            .template
            .is_none()
            .then(|| text.to_owned()),
    })
}

#[cfg(feature = "native-project")]
/// The hash of a source text; saved modules are only read by the binary
/// that saved them, whose hasher does not change.
fn hash(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

#[cfg(feature = "native-project")]
/// Sets the file of every span in `value` to `to`; false when one is not in
/// `from`.
fn move_spans(value: &mut Value, from: Option<&Value>, to: &Value) -> bool {
    match value {
        Value::Object(fields) if is_span(fields) => {
            let file = fields.get_mut("file").expect("a span has a file");
            if from.is_some_and(|from| file != from) {
                return false;
            }
            *file = to.clone();
            true
        }
        Value::Object(fields) => fields.values_mut().all(|v| move_spans(v, from, to)),
        Value::Array(items) => items.iter_mut().all(|v| move_spans(v, from, to)),
        _ => true,
    }
}

#[cfg(feature = "native-project")]
fn is_span(fields: &serde_json::Map<String, Value>) -> bool {
    fields.len() == 3
        && ["file", "start", "end"]
            .iter()
            .all(|k| fields.contains_key(*k))
}

impl Loader<'_> {
    /// Uses the saved module `name` of the package in `dir`, and what it
    /// imports, instead of loading their sources; false when none is saved.
    #[cfg(feature = "native-project")]
    pub(super) fn use_saved(&mut self, dir: &Path, name: &str) -> bool {
        if !self.precompiled.covers(dir, name) {
            return false;
        }
        if self.from_packages.contains_key(name) || self.by_name.contains_key(name) {
            return true;
        }
        if self.restore_template(name) {
            return true;
        }
        let (_, module) = &self.precompiled.saved[name];
        let imports = module.imports.clone();
        self.from_packages
            .insert(name.to_owned(), module.interface.clone());
        self.precompiled.used.push(name.to_owned());
        for import in imports {
            self.find_in_packages(&import);
        }
        true
    }

    #[cfg(feature = "native-project")]
    fn restore_template(&mut self, name: &str) -> bool {
        let (package, module) = &self.precompiled.saved[name];
        let Some(template) = &module.interface.template else {
            return false;
        };
        let path = self.precompiled.packages[package].1.join(&module.path);
        #[cfg(feature = "native-project")]
        if self.host.read(&path).ok().is_some_and(|bytes| {
            str::from_utf8(&bytes).map_or(true, |text| hash(text) != module.source)
        }) {
            self.diagnostics.push(Diagnostic::new(codes::P034, Severity::Error,
                format!("`{}` changed since its package was checked: `flick install` restores the package store", self.host.display(&path))));
        }
        let file = self
            .sources
            .add(self.host.display(&path), template.source.clone());
        let parse = Arc::new(syntax::parse(&template.source, file).into_module());
        let syntax = Arc::new(resolve::module_syntax(&parse.tree, &template.source, file));
        let at = syntax
            .header
            .as_ref()
            .map_or(Span::new(file, 0, 0), |h| h.span);
        let virtual_of = template.capture.clone().map(|mut capture| {
            capture
                .definition_package
                .get_or_insert_with(|| package.clone());
            Virtual { at, capture }
        });
        let index = self.modules.len();
        self.by_name.insert(name.to_owned(), index);
        self.modules.push(Loaded {
            name: name.to_owned(),
            path,
            file,
            root: Root::Package(package.clone()),
            parse,
            syntax,
            virtual_of,
            member_of: module.interface.member_of.clone(),
            scope_of: None,
            expansion: None,
            editing: Vec::new(),
            loader: None,
        });
        true
    }

    /// A saved ordinary module can own a functor's lexical declarations even
    /// when only the nested template is restored as a loaded source module.
    #[cfg(feature = "native-project")]
    pub(super) fn scope_source(&mut self, name: &str) -> Option<Loaded> {
        let (package, saved) = self.precompiled.saved.get(name)?;
        let source = saved
            .interface
            .template
            .as_ref()
            .map(|t| &t.source)
            .or(saved.source_text.as_ref())?
            .clone();
        let path = self.precompiled.packages[package].1.join(&saved.path);
        if self.host.read(&path).ok().is_some_and(|bytes| {
            str::from_utf8(&bytes).map_or(true, |text| hash(text) != saved.source)
        }) {
            self.diagnostics.push(Diagnostic::new(codes::P034, Severity::Error, format!("`{}` changed since its package was checked: `flick install` restores the package store", self.host.display(&path))));
        }
        let file = self.sources.add(self.host.display(&path), source.clone());
        let parse = Arc::new(syntax::parse(&source, file).into_module());
        let syntax = Arc::new(resolve::module_syntax(&parse.tree, &source, file));
        Some(Loaded {
            name: name.into(),
            path,
            file,
            root: Root::Package(package.clone()),
            parse,
            syntax,
            virtual_of: None,
            member_of: None,
            scope_of: None,
            expansion: None,
            editing: Vec::new(),
            loader: None,
        })
    }

    /// Finds `name`, imported by a saved module, as a module of a source
    /// package or a built-in one.
    #[cfg(feature = "native-project")]
    fn find_in_packages(&mut self, name: &str) {
        if self.by_name.contains_key(name) || self.from_packages.contains_key(name) {
            return;
        }
        for (_, dir) in self.package_sources() {
            if !self.use_saved(&dir, name) {
                let _ = self.load(&dir.join(module_path(name)));
            }
        }
        if !self.by_name.contains_key(name)
            && !self.from_packages.contains_key(name)
            && let Some(builtin) = self.builtins().iter().find(|i| i.module == name)
        {
            self.from_packages.insert(name.to_owned(), builtin.clone());
        }
    }

    /// What to save of the modules of source packages in `compiled`, when
    /// `save`; a build also compiles the saved modules it used.
    #[cfg_attr(not(feature = "native-project"), allow(clippy::unused_self))]
    pub(super) fn finish_packages(
        &mut self,
        compiled: &mut Vec<Compiled>,
        mode: Mode,
        save: bool,
    ) -> Vec<Fresh> {
        // without a project of packages nothing is saved: the browser build
        // carries none of the serialization of the IR
        #[cfg(not(feature = "native-project"))]
        let _ = (compiled, mode, save);
        #[cfg(not(feature = "native-project"))]
        return Vec::new();
        #[cfg(feature = "native-project")]
        {
            let fresh = if save {
                let expanded: Vec<&str> = (self.modules.iter())
                    .filter(|m| m.expansion.is_some())
                    .filter_map(|m| match &m.root {
                        super::Root::Package(name) => Some(name.as_str()),
                        _ => None,
                    })
                    .collect();
                self.precompiled.fresh(compiled, self.sources, &expanded)
            } else {
                Vec::new()
            };
            if mode != Mode::Check {
                let saved = self.saved_compiled();
                compiled.extend(saved);
            }
            fresh
        }
    }

    #[cfg(feature = "native-project")]
    /// The saved modules the run used, compiled: each in its file again,
    /// which a source map points into.
    fn saved_compiled(&mut self) -> Vec<Compiled> {
        let used = mem::take(&mut self.precompiled.used);
        let mut out = Vec::new();
        for name in used {
            let (package, module) = &self.precompiled.saved[&name];
            let dir = &self.precompiled.packages[package].1;
            let path = dir.join(&module.path);
            let bytes = match self.host.read(&path) {
                Ok(bytes) => bytes,
                Err(err) if err.kind() == ErrorKind::NotFound && module.source_text.is_some() => {
                    module
                        .source_text
                        .as_ref()
                        .expect("saved source")
                        .as_bytes()
                        .to_vec()
                }
                Err(err) => {
                    let message = format!("cannot read `{}`: {err}", self.host.display(&path));
                    self.diagnostics
                        .push(Diagnostic::new(codes::P001, Severity::Error, message));
                    continue;
                }
            };
            let (file, _) = self.sources.add_bytes(self.host.display(&path), bytes);
            // spans into another text would point past it
            if hash(&self.sources.get(file).text) != module.source {
                let message = format!(
                    "`{}` changed since its package was checked: `flick install` restores the package store",
                    self.host.display(&path)
                );
                self.diagnostics
                    .push(Diagnostic::new(codes::P034, Severity::Error, message));
                continue;
            }
            let mut ir = module.ir.clone();
            let to = serde_json::to_value(file).expect("a file id serializes");
            move_spans(&mut ir, None, &to);
            let ir = match serde_json::from_value::<ir::Module>(ir) {
                Ok(ir) => ir,
                Err(err) => {
                    let message = format!("the saved IR of `{name}` does not load: {err}");
                    self.diagnostics
                        .push(Diagnostic::new(codes::P001, Severity::Error, message));
                    continue;
                }
            };
            out.push(Compiled {
                name: module.name.clone(),
                path,
                file,
                test: false,
                package: Some(package.clone()),
                imports: module.imports.clone(),
                ir,
                declarations: module.declarations.clone(),
                interface: module.interface.clone(),
                expansion: None,
            });
        }
        out
    }
}
