//! Built-in modules, written in Flick and checked by this checker: `Prelude`
//! (the built-in types every module sees) and small modules a program may
//! qualify without an import (`Ref.make`, `String.trim`, `Layer.succeed`, …).

use std::sync::OnceLock;

use crate::iface::Interface;
use crate::resolve::{self, Module};
use crate::source::SourceMap;
use crate::syntax;

/// In dependency order: each module sees the ones before it.
const SOURCES: [(&str, &str); 13] = [
    ("Prelude", include_str!("builtins/Prelude.fl")),
    ("Instances", include_str!("builtins/Instances.fl")),
    ("Record", include_str!("builtins/Record.fl")),
    ("Ref", include_str!("builtins/Ref.fl")),
    ("String", include_str!("builtins/String.fl")),
    ("Number", include_str!("builtins/Number.fl")),
    ("Platform", include_str!("builtins/Platform.fl")),
    ("App", include_str!("builtins/App.fl")),
    ("Layer", include_str!("builtins/Layer.fl")),
    ("Fiber", include_str!("builtins/Fiber.fl")),
    ("RawCause", include_str!("builtins/RawCause.fl")),
    ("Failure", include_str!("builtins/Failure.fl")),
    ("Exit", include_str!("builtins/Exit.fl")),
];

/// Names and sources of the built-in modules, in dependency order.
pub fn builtin_sources() -> &'static [(&'static str, &'static str)] {
    &SOURCES
}

/// Interfaces of the built-in modules.
pub fn builtins() -> &'static [Interface] {
    static BUILTINS: OnceLock<Vec<Interface>> = OnceLock::new();
    BUILTINS.get_or_init(|| {
        let mut done: Vec<Interface> = Vec::new();
        for (name, text) in SOURCES {
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
            let imported = |_: &str| None;
            let (mut interface, mut diagnostics) = resolve::resolve(&module, &imported);
            diagnostics.extend(parse.diagnostics);
            // the built-in modules are one package
            let local = |name: &str| SOURCES.iter().any(|(n, _)| *n == name);
            let options = super::Options {
                local: &local,
                limits: super::Limits::default(),
            };
            diagnostics.extend(super::check(
                &module,
                &imported,
                &done,
                options,
                &mut interface,
            ));
            assert!(
                diagnostics.is_empty(),
                "built-in module {name} does not check: {diagnostics:?}"
            );
            done.push(interface);
        }
        done
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn builtins_check_cleanly() {
        let names: Vec<&str> = super::builtins()
            .iter()
            .map(|i| i.module.as_str())
            .collect();
        assert_eq!(
            names,
            [
                "Prelude",
                "Instances",
                "Record",
                "Ref",
                "String",
                "Number",
                "Platform",
                "App",
                "Layer",
                "Fiber",
                "RawCause",
                "Failure",
                "Exit"
            ]
        );
    }
}
