//! Built-in modules, written in Flick and checked by this checker: `Prelude`
//! (the built-in types every module sees) and small modules a program may
//! qualify without an import (`Ref.make`, `String.trim`, `Layer.succeed`, …).

use std::sync::OnceLock;

use tracing::debug;

use super::builtin_sources::SOURCES;
use super::env::Builtins;
use crate::clock::timed;
use crate::iface::Interface;
use crate::resolve::{self, Module};
use crate::source::SourceMap;
use crate::syntax;

/// Names and sources of the built-in modules, in dependency order.
pub fn builtin_sources() -> &'static [(&'static str, &'static str)] {
    SOURCES
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
    let mut done: Vec<Interface> = Vec::new();
    let mut known = Builtins::default();
    for &(name, text) in SOURCES {
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
        let local = |name: &str| SOURCES.iter().any(|(n, _)| *n == name);
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
        let sources: Vec<&str> = super::SOURCES.iter().map(|(n, _)| *n).collect();
        assert_eq!(names, sources);
        assert_eq!(names.iter().collect::<BTreeSet<_>>().len(), names.len());
        assert_eq!(names[..2], ["Prelude", "Instances"]);
    }
}
