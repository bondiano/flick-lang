//! Built-in modules, written in Flick and checked by this checker: `Prelude`
//! (the built-in types every module sees) and small modules a program may
//! qualify without an import (`Ref.make`, `String.trim`, `Layer.succeed`, …).
//! The core modules are part of the compiler; the others come from the
//! implicit packages the host hands over with [`set_implicit_packages`]
//! before any module is checked.

#[cfg(feature = "native-project")]
use std::env;
#[cfg(feature = "native-project")]
use std::path::Path;
use std::ptr;
use std::sync::OnceLock;
#[cfg(feature = "native-project")]
use std::{fs, io};

use tracing::debug;

use super::builtin_sources::CORE;
use super::env::Builtins;
use super::{Typing, Watched};
use crate::clock::{micros, timed};
use crate::diagnostics::{Code, Diagnostic, codes};
use crate::iface::Interface;
#[cfg(feature = "native-project")]
use crate::implicit_packages::IMPLICIT_PACKAGES;
use crate::plugin::derive::builtin_expansion;
#[cfg(feature = "native-project")]
use crate::project::{CONFIG_FILE, config};
use crate::resolve::{self, Module};
#[cfg(feature = "native-project")]
use crate::source::FileId;
use crate::source::SourceMap;
use crate::syntax;

/// The files of a package, as `(name, text)`.
type Files = &'static [(&'static str, &'static str)];

/// What the compiler reads of an implicit package: its modules, in the order
/// of its `src/modules.txt`, the tables of `src/go/` that bind its foreign
/// declarations on the go target, and the `[js.dependencies]` of its manifest
/// as `(name, version)`.
#[derive(Debug, Default)]
pub struct ImplicitPackage {
    pub name: String,
    pub modules: Vec<(String, String)>,
    pub go_bindings: Vec<(String, String)>,
    pub js_dependencies: Vec<(String, String)>,
}

/// An implicit package this run uses.
struct Implicit {
    name: &'static str,
    modules: Files,
    go_bindings: Files,
    js_dependencies: Files,
}

static IMPLICIT: OnceLock<Vec<Implicit>> = OnceLock::new();

fn leak(files: Vec<(String, String)>) -> Files {
    let leaked: Vec<(&'static str, &'static str)> = files
        .into_iter()
        .map(|(name, text)| (&*name.leak(), &*text.leak()))
        .collect();
    leaked.leak()
}

fn leak_all(packages: Vec<ImplicitPackage>) -> Vec<Implicit> {
    packages
        .into_iter()
        .map(|package| Implicit {
            name: package.name.leak(),
            modules: leak(package.modules),
            go_bindings: leak(package.go_bindings),
            js_dependencies: leak(package.js_dependencies),
        })
        .collect()
}

/// Uses `packages` as the implicit ones, in that order; false once they are
/// set.
pub fn set_implicit_packages(packages: Vec<ImplicitPackage>) -> bool {
    IMPLICIT.set(leak_all(packages)).is_ok()
}

/// Whether the modules of the implicit packages are known: set, built in or
/// found at `FLICK_STD_PATH`.
pub fn has_implicit_packages() -> bool {
    !implicit().is_empty()
}

/// The implicit package `module` belongs to.
pub fn implicit_package_of(module: &str) -> Option<&'static str> {
    (implicit().iter())
        .find(|package| package.modules.iter().any(|(name, _)| *name == module))
        .map(|package| package.name)
}

/// The tables that bind the foreign declarations of the implicit packages on
/// the go target.
pub fn implicit_go_bindings() -> impl Iterator<Item = &'static (&'static str, &'static str)> {
    implicit().iter().flat_map(|package| package.go_bindings)
}

/// The npm packages each implicit package names, with its name.
pub fn implicit_js_dependencies() -> impl Iterator<Item = (&'static str, &'static str, &'static str)>
{
    implicit().iter().flat_map(|package| {
        (package.js_dependencies.iter()).map(|(name, version)| (package.name, *name, *version))
    })
}

/// The implicit packages: those set, else those at `FLICK_STD_PATH`, else the
/// ones built in, else none. The variable comes before what is built in: a
/// workspace build unifies the playground's `embedded-std` into every binary.
fn implicit() -> &'static [Implicit] {
    IMPLICIT.get_or_init(|| {
        #[cfg(feature = "native-project")]
        if let Some(packages) = env::var_os("FLICK_STD_PATH")
            .filter(|dir| !dir.is_empty())
            .and_then(|dir| read_implicit(Path::new(&dir)).ok())
        {
            return leak_all(packages);
        }
        #[cfg(feature = "embedded-std")]
        return (super::builtin_sources::IMPLICIT.iter())
            .map(|&(name, modules, go_bindings)| Implicit {
                name,
                modules,
                go_bindings,
                js_dependencies: &[],
            })
            .collect();
        #[allow(unreachable_code, reason = "a host without a project hands them over")]
        Vec::new()
    })
}

/// Each implicit package, the first in the directory `first` and the others
/// beside it.
#[cfg(feature = "native-project")]
fn read_implicit(first: &Path) -> io::Result<Vec<ImplicitPackage>> {
    (IMPLICIT_PACKAGES.iter().enumerate())
        .map(|(i, name)| {
            if i == 0 {
                return read_package(name, first);
            }
            let parent = first.parent().unwrap_or(first);
            read_package(name, &parent.join(name))
        })
        .collect()
}

/// The implicit package `name` in `dir`.
#[cfg(feature = "native-project")]
pub fn read_package(name: &str, dir: &Path) -> io::Result<ImplicitPackage> {
    let sources = dir.join("src");
    let order = fs::read_to_string(sources.join("modules.txt"))?;
    let modules = order
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|module| {
            let file = sources.join(format!("{}.fl", module.replace('.', "/")));
            Ok((module.to_owned(), fs::read_to_string(file)?))
        })
        .collect::<io::Result<_>>()?;
    let mut go_bindings = Vec::new();
    if let Ok(entries) = fs::read_dir(sources.join("go")) {
        for entry in entries {
            let path = entry?.path();
            if path.extension().is_some_and(|e| e == "txt") {
                let file = path.file_name().map(|n| n.to_string_lossy().into_owned());
                go_bindings.push((file.unwrap_or_default(), fs::read_to_string(&path)?));
            }
        }
    }
    go_bindings.sort();
    Ok(ImplicitPackage {
        name: name.to_owned(),
        modules,
        go_bindings,
        js_dependencies: manifest_js_dependencies(dir),
    })
}

/// The `[js.dependencies]` of the manifest in `dir`. A manifest that does not
/// parse names none: its own build reports it.
#[cfg(feature = "native-project")]
fn manifest_js_dependencies(dir: &Path) -> Vec<(String, String)> {
    let Ok(text) = fs::read_to_string(dir.join(CONFIG_FILE)) else {
        return Vec::new();
    };
    let Ok(config) = config::parse(&text, FileId::default()) else {
        return Vec::new();
    };
    (config.js_dependencies.into_iter())
        .map(|d| (d.name, d.version))
        .collect()
}

/// Names and sources of the built-in modules, in dependency order: the
/// core modules, then those of each implicit package.
pub fn builtin_sources() -> &'static [(&'static str, &'static str)] {
    static ALL: OnceLock<Vec<(&str, &str)>> = OnceLock::new();
    ALL.get_or_init(|| {
        let implicit = implicit().iter().flat_map(|package| package.modules);
        CORE.iter().chain(implicit).copied().collect()
    })
}

/// The interfaces of the built-in modules in each mode of `stages_by_class`.
static BUILTINS: [OnceLock<Vec<Interface>>; 2] = [OnceLock::new(), OnceLock::new()];

/// Interfaces of the built-in modules.
pub fn builtins() -> &'static [Interface] {
    builtins_of(false)
}

/// Interfaces of the built-in modules, checked with `stages_by_class`.
pub fn builtins_of(stages_by_class: bool) -> &'static [Interface] {
    BUILTINS[usize::from(stages_by_class)].get_or_init(|| {
        let (done, elapsed) = timed(|| check_builtins(stages_by_class));
        debug!(
            modules = done.len(),
            ?elapsed,
            micros = micros(elapsed),
            "built-in modules checked"
        );
        done
    })
}

fn check_builtins(stages_by_class: bool) -> Vec<Interface> {
    check_each(builtin_sources(), stages_by_class, |_, _| {})
}

/// [`builtins_of`], or what the first module of an implicit package that does not
/// check says: a project or `FLICK_STD_PATH` may name a package of another
/// compiler. A core module that does not check is still a bug of this one.
pub fn try_builtins(stages_by_class: bool) -> Result<&'static [Interface], String> {
    let slot = &BUILTINS[usize::from(stages_by_class)];
    if let Some(done) = slot.get() {
        return Ok(done);
    }
    let mut problem = None;
    let report = |name: &str, mut diagnostics: Vec<Diagnostic>| {
        diagnostics.retain(|d| !ALLOWED.contains(&d.code));
        let Some(first) = diagnostics.first() else {
            return;
        };
        let Some(package) = implicit_package_of(name) else {
            panic!("built-in module {name} does not check: {diagnostics:?}");
        };
        problem.get_or_insert_with(|| format!("`{name}` of `{package}`: {}", first.message));
    };
    let done = check_reporting(builtin_sources(), report, stages_by_class, |_, _| {});
    match problem {
        Some(problem) => Err(problem),
        None => Ok(slot.get_or_init(|| done)),
    }
}

/// The warnings the built-in modules may have: `flick check --fix` has to get
/// past them to fix them, and the import a fixed signature no longer uses; a
/// test below says the modules here have none.
const ALLOWED: [Code; 2] = [codes::T059, codes::R012];

/// Checks the built-in modules `modules`, as `(module, text)` in
/// dependency order.
#[cfg(test)]
pub(crate) fn check_sources(modules: &[(&str, &str)]) -> Vec<Interface> {
    check_each(modules, false, |_, _| {})
}

/// [`check_sources`], also giving each module with its typing to `each`.
/// What the modules before it export grows by one module at a time instead
/// of being learned again for each module.
pub fn check_each(
    modules: &[(&str, &str)],
    stages_by_class: bool,
    each: impl FnMut(&Module, &Typing),
) -> Vec<Interface> {
    check_allowing(modules, &ALLOWED, stages_by_class, each)
}

/// [`check_each`], with the built-in modules allowed the warnings `allowed`.
fn check_allowing(
    modules: &[(&str, &str)],
    allowed: &[Code],
    stages_by_class: bool,
    each: impl FnMut(&Module, &Typing),
) -> Vec<Interface> {
    let report = |name: &str, mut diagnostics: Vec<Diagnostic>| {
        diagnostics.retain(|d| !allowed.contains(&d.code));
        assert!(
            diagnostics.is_empty(),
            "built-in module {name} does not check: {diagnostics:?}"
        );
    };
    check_reporting(modules, report, stages_by_class, each)
}

/// [`check_each`], giving what each module's check says to `report`.
fn check_reporting(
    modules: &[(&str, &str)],
    mut report: impl FnMut(&str, Vec<Diagnostic>),
    stages_by_class: bool,
    mut each: impl FnMut(&Module, &Typing),
) -> Vec<Interface> {
    let mut done: Vec<Interface> = Vec::new();
    let mut known = Builtins::default();
    for &(name, text) in modules {
        let mut sources = SourceMap::default();
        let file = sources.add(format!("<builtin>/{name}.fl"), text);
        let parse = syntax::parse(text, file);
        let module_syntax = resolve::module_syntax(&parse.tree, text, file);
        // the witnesses its `deriving` names, which std's plugin writes
        let (expansion, derived) = builtin_expansion(text, file, &parse.tree, name, &module_syntax);
        assert!(
            derived.is_empty(),
            "built-in module {name} does not derive: {derived:?}"
        );
        let module = Module {
            name,
            file,
            text: expansion.as_ref().map_or(text, |e| &e.text),
            tree: expansion.as_ref().map_or(&parse.tree, |e| &e.tree),
            syntax: &module_syntax,
            generated: expansion.as_ref().map_or(resolve::NO_GENERATED, |e| e.len),
        };
        // a facade re-exports the names of the modules before it
        let imported = |m: &str| done.iter().find(|i| i.module == m).cloned();
        let (mut interface, mut diagnostics) = resolve::resolve(&module, &imported);
        diagnostics.extend(parse.diagnostics);
        // the built-in modules are one package
        let local = |name: &str| modules.iter().any(|(n, _)| *n == name);
        let options = super::Options {
            local: &local,
            limits: super::Limits::default(),
            no_let_assert: false,
            thresholds: super::Thresholds::default(),
            effects: true,
            watched: Watched::none(),
            stages_by_class,
        };
        let (checked, typing, _) =
            super::run_in(&module, &imported, &known, options, &mut interface, &[]);
        diagnostics.extend(checked);
        if let Some(expansion) = &expansion {
            expansion.restore(&mut diagnostics, None);
        }
        report(name, diagnostics);
        each(&module, &typing);
        known.add(&interface);
        done.push(interface);
    }
    done
}

/// Uses `interfaces`, saved from an earlier [`builtins_of`] of the same
/// sources and mode, instead of checking the built-in modules again; false
/// once they are already in use.
pub fn install_builtins(stages_by_class: bool, interfaces: Vec<Interface>) -> bool {
    BUILTINS[usize::from(stages_by_class)]
        .set(interfaces)
        .is_ok()
}

/// The mode of the built-in modules [`builtins_of`] holds, if `interfaces` are
/// those, without checking them when they are not checked yet.
pub(super) fn are_builtins(interfaces: &[Interface]) -> Option<bool> {
    [false, true].into_iter().find(|&by_class| {
        BUILTINS[usize::from(by_class)]
            .get()
            .is_some_and(|b| ptr::eq(b.as_slice(), interfaces))
    })
}

/// What every module knows of the built-in modules in a mode.
pub(super) fn builtin_env(stages_by_class: bool) -> &'static Builtins {
    static ENV: [OnceLock<Builtins>; 2] = [OnceLock::new(), OnceLock::new()];
    ENV[usize::from(stages_by_class)].get_or_init(|| Builtins::new(builtins_of(stages_by_class)))
}

#[cfg(test)]
#[path = "builtins_tests.rs"]
mod builtins_tests;
