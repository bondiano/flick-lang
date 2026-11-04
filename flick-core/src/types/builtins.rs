//! Built-in modules, written in Flick and checked by this checker: `Prelude`
//! (the built-in types every module sees) and small modules a program may
//! qualify without an import (`Ref.make`, `String.trim`, `Layer.succeed`, …).
//! The core modules are part of the compiler; the modules of `std` come from
//! the `std` package the host hands over with [`set_std`] before any
//! module is checked.

#[cfg(feature = "native-project")]
use std::env;
#[cfg(feature = "native-project")]
use std::path::Path;
use std::ptr;
use std::sync::OnceLock;
#[cfg(feature = "native-project")]
use std::{fs, io};

use tracing::debug;

use super::Typing;
use super::builtin_sources::CORE;
use super::env::Builtins;
use crate::clock::timed;
use crate::iface::Interface;
use crate::plugin::derive::builtin_expansion;
use crate::resolve::{self, Module};
use crate::source::SourceMap;
use crate::syntax;

/// The files of the `std` package, as `(name, text)`.
type Files = &'static [(&'static str, &'static str)];

/// What the compiler reads of the `std` package: its modules, in the order of
/// its `src/modules.txt`, and the tables of `src/go/` that bind its foreign
/// declarations on the go target.
#[derive(Debug, Default)]
pub struct StdPackage {
    pub modules: Vec<(String, String)>,
    pub go_bindings: Vec<(String, String)>,
}

static STD: OnceLock<(Files, Files)> = OnceLock::new();

fn leak(files: Vec<(String, String)>) -> Files {
    let leaked: Vec<(&'static str, &'static str)> = files
        .into_iter()
        .map(|(name, text)| (&*name.leak(), &*text.leak()))
        .collect();
    leaked.leak()
}

/// Uses `package` as `std`; false once it is set.
pub fn set_std(package: StdPackage) -> bool {
    STD.set((leak(package.modules), leak(package.go_bindings)))
        .is_ok()
}

/// Whether the modules of `std` are known: set, built in or found at
/// `FLICK_STD_PATH`.
pub fn has_std() -> bool {
    !std_files().0.is_empty()
}

/// Whether `module` is a module of `std`.
pub fn is_std_module(module: &str) -> bool {
    std_files().0.iter().any(|(name, _)| *name == module)
}

/// The tables that bind the foreign declarations of `std` on the go target.
pub fn std_go_bindings() -> Files {
    std_files().1
}

/// The files of `std`: those set, else those of the package at
/// `FLICK_STD_PATH`, else the ones built in, else none. The variable comes
/// before what is built in: a workspace build unifies the playground's
/// `embedded-std` into every binary.
fn std_files() -> (Files, Files) {
    *STD.get_or_init(|| {
        #[cfg(feature = "native-project")]
        if let Some(package) = env::var_os("FLICK_STD_PATH")
            .filter(|dir| !dir.is_empty())
            .and_then(|dir| read_std(Path::new(&dir)).ok())
        {
            return (leak(package.modules), leak(package.go_bindings));
        }
        #[cfg(feature = "embedded-std")]
        return (super::builtin_sources::STD, super::builtin_sources::STD_GO);
        #[allow(unreachable_code, reason = "a host without a project hands std over")]
        (&[], &[])
    })
}

/// The `std` package in `dir`.
#[cfg(feature = "native-project")]
pub fn read_std(dir: &Path) -> io::Result<StdPackage> {
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
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
                go_bindings.push((name.unwrap_or_default(), fs::read_to_string(&path)?));
            }
        }
    }
    go_bindings.sort();
    Ok(StdPackage {
        modules,
        go_bindings,
    })
}

/// Names and sources of the built-in modules, in dependency order: the
/// core modules, then those of `std`.
pub fn builtin_sources() -> &'static [(&'static str, &'static str)] {
    static ALL: OnceLock<Vec<(&str, &str)>> = OnceLock::new();
    ALL.get_or_init(|| CORE.iter().chain(std_files().0).copied().collect())
}

static BUILTINS: OnceLock<Vec<Interface>> = OnceLock::new();

/// Interfaces of the built-in modules.
pub fn builtins() -> &'static [Interface] {
    BUILTINS.get_or_init(|| {
        let (done, elapsed) = timed(check_builtins);
        debug!(modules = done.len(), ?elapsed, "built-in modules checked");
        done
    })
}

fn check_builtins() -> Vec<Interface> {
    check_sources(builtin_sources())
}

/// Checks the built-in modules `modules`, as `(module, text)` in
/// dependency order.
pub(crate) fn check_sources(modules: &[(&str, &str)]) -> Vec<Interface> {
    check_each(modules, |_, _| {})
}

/// [`check_sources`], also giving each module with its typing to `each`.
/// What the modules before it export grows by one module at a time instead
/// of being learned again for each module.
pub fn check_each(
    modules: &[(&str, &str)],
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
            sql: Default::default(),
            no_let_assert: false,
            thresholds: super::Thresholds::default(),
            effects: true,
        };
        let (checked, typing, _) =
            super::run_in(&module, &imported, &known, options, &mut interface);
        diagnostics.extend(checked);
        if let Some(expansion) = &expansion {
            expansion.restore(&mut diagnostics, None);
        }
        assert!(
            diagnostics.is_empty(),
            "built-in module {name} does not check: {diagnostics:?}"
        );
        each(&module, &typing);
        known.add(&interface);
        done.push(interface);
    }
    done
}

/// Uses `interfaces`, saved from an earlier [`builtins`] of the same
/// sources, instead of checking the built-in modules again; false once
/// they are already in use.
pub fn install_builtins(interfaces: Vec<Interface>) -> bool {
    BUILTINS.set(interfaces).is_ok()
}

/// Whether `interfaces` are the built-in modules [`builtins`] holds, without
/// checking them when they are not checked yet.
pub(super) fn are_builtins(interfaces: &[Interface]) -> bool {
    BUILTINS
        .get()
        .is_some_and(|b| ptr::eq(b.as_slice(), interfaces))
}

/// What every module knows of the built-in modules.
pub(super) fn builtin_env() -> &'static Builtins {
    static ENV: OnceLock<Builtins> = OnceLock::new();
    ENV.get_or_init(|| Builtins::new(builtins()))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    #[test]
    fn builtins_check_cleanly() {
        let names: Vec<&str> = super::builtins()
            .iter()
            .map(|i| i.module.as_str())
            .collect();
        let sources: Vec<&str> = super::builtin_sources().iter().map(|(n, _)| *n).collect();
        assert_eq!(names, sources);
        assert_eq!(names.iter().collect::<BTreeSet<_>>().len(), names.len());
        assert_eq!(names[..2], ["Prelude", "Instances"]);
    }
}
