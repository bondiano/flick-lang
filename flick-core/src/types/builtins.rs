//! Built-in modules, written in Flick and checked by this checker: `Prelude`
//! (the built-in types every module sees) and small modules a program may
//! qualify without an import (`Ref.make`, `String.trim`, `Layer.succeed`, …).
//! The core modules are part of the compiler; the modules of `std` come from
//! the `std` package the host hands over with [`set_std`] before any
//! module is checked.

#[cfg(all(not(feature = "embedded-std"), feature = "native-project"))]
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
use crate::clock::timed;
use crate::iface::Interface;
use crate::resolve::{self, Module};
use crate::source::SourceMap;
use crate::syntax;

static STD: OnceLock<&'static [(&'static str, &'static str)]> = OnceLock::new();

fn leak(sources: Vec<(String, String)>) -> &'static [(&'static str, &'static str)] {
    let leaked: Vec<(&'static str, &'static str)> = sources
        .into_iter()
        .map(|(module, text)| (&*module.leak(), &*text.leak()))
        .collect();
    leaked.leak()
}

/// Uses `sources`, as `(module, text)` in the order of the package's
/// `modules.txt`, as the modules of `std`; false once they are set.
pub fn set_std(sources: Vec<(String, String)>) -> bool {
    STD.set(leak(sources)).is_ok()
}

/// Whether the modules of `std` are known: set, built in or found at
/// `FLICK_STD_PATH`.
pub fn has_std() -> bool {
    !std_sources().is_empty()
}

/// The modules of `std`: those set, else the ones built in, else those of
/// the package at `FLICK_STD_PATH`, else none.
fn std_sources() -> &'static [(&'static str, &'static str)] {
    STD.get_or_init(|| {
        #[cfg(feature = "embedded-std")]
        return super::builtin_sources::STD;
        #[cfg(all(not(feature = "embedded-std"), feature = "native-project"))]
        return env::var_os("FLICK_STD_PATH")
            .and_then(|dir| read_std(Path::new(&dir)).ok())
            .map_or(&[], leak);
        #[allow(unreachable_code, reason = "a host without a project hands std over")]
        &[]
    })
}

/// The modules of the `std` package in `dir`, in the order of its
/// `src/modules.txt`.
#[cfg(feature = "native-project")]
pub fn read_std(dir: &Path) -> io::Result<Vec<(String, String)>> {
    let sources = dir.join("src");
    let order = fs::read_to_string(sources.join("modules.txt"))?;
    order
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|module| {
            let file = sources.join(format!("{}.fl", module.replace('.', "/")));
            Ok((module.to_owned(), fs::read_to_string(file)?))
        })
        .collect()
}

/// Names and sources of the built-in modules, in dependency order: the
/// core modules, then those of `std`.
pub fn builtin_sources() -> &'static [(&'static str, &'static str)] {
    static ALL: OnceLock<Vec<(&str, &str)>> = OnceLock::new();
    ALL.get_or_init(|| CORE.iter().chain(std_sources()).copied().collect())
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
    let mut done: Vec<Interface> = Vec::new();
    let mut known = Builtins::default();
    for &(name, text) in modules {
        let mut sources = SourceMap::default();
        let file = sources.add(format!("<builtin>/{name}.fl"), text);
        let parse = syntax::parse(text, file);
        let module_syntax = resolve::module_syntax(&parse.tree, text, file);
        let module = Module {
            name,
            file,
            text,
            tree: &parse.tree,
            syntax: &module_syntax,
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
        let (checked, _, _) = super::run_in(&module, &imported, &known, options, &mut interface);
        diagnostics.extend(checked);
        assert!(
            diagnostics.is_empty(),
            "built-in module {name} does not check: {diagnostics:?}"
        );
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
