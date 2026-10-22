//! Built-in modules, written in Flick and checked by this checker: `Prelude`
//! (the built-in types every module sees) and small modules a program may
//! qualify without an import (`Ref.make`, `String.trim`, `Layer.succeed`, …).

use std::sync::OnceLock;

use crate::iface::Interface;
use crate::resolve::{self, Module};
use crate::source::SourceMap;
use crate::syntax;

/// In dependency order: each module sees the ones before it.
const SOURCES: [(&str, &str); 6] = [
    ("Prelude", include_str!("builtins/Prelude.fl")),
    ("Ref", include_str!("builtins/Ref.fl")),
    ("String", include_str!("builtins/String.fl")),
    ("Platform", include_str!("builtins/Platform.fl")),
    ("App", include_str!("builtins/App.fl")),
    ("Layer", include_str!("builtins/Layer.fl")),
];

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
            diagnostics.extend(super::check(&module, &imported, &done, &mut interface));
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
            ["Prelude", "Ref", "String", "Platform", "App", "Layer"]
        );
    }
}
