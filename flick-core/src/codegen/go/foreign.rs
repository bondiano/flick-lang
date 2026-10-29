//! `foreign` bindings: each JS export the target supports has a Go function
//! in the runtime that takes the binding's dictionaries and arguments at
//! once and works on Flick values (callbacks stay curried, `Option` stays
//! an `Option`). A binding the program reaches without one is reported.

use std::collections::HashMap;
use std::sync::OnceLock;

use super::{Gen, names, param_list};
use crate::ir::Foreign;
use crate::source::Span;

/// The binding tables, by area: under `[module]` (or `[global Module]` for
/// a JS global, which may mean different things for different types), an
/// export (or binding name) and its function of package `flickrt`.
const TABLES: &[&str] = &[
    include_str!("bindings/runtime.txt"),
    include_str!("bindings/effect.txt"),
    include_str!("bindings/globals.txt"),
];

fn table() -> &'static HashMap<(String, String), String> {
    static TABLE: OnceLock<HashMap<(String, String), String>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut out = HashMap::new();
        for text in TABLES {
            let mut module = "";
            for line in text.lines().map(str::trim) {
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Some(section) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                    module = section;
                    continue;
                }
                let (export, go) = line.split_once(' ').expect("an export and a Go function");
                out.insert(
                    (module.to_owned(), export.to_owned()),
                    format!("flickrt.{go}"),
                );
            }
        }
        out
    })
}

/// The Go function of the binding `name` of `module` to `export` of `from`.
fn lookup(module: &str, name: &str, from: Option<&str>, export: &str) -> Option<&'static str> {
    let key = match from {
        Some(from) => (from.to_owned(), export.to_owned()),
        None => (format!("global {module}"), name.to_owned()),
    };
    table().get(&key).map(String::as_str)
}

/// How many parameters the Go function of a binding takes after its
/// dictionaries: every stage, though the JS function takes `arity` and
/// returns a function of the rest, as Effect's data-last functions do.
pub(super) fn parameters(f: &Foreign) -> usize {
    if f.arity == 0 {
        f.info.stages_taken()
    } else {
        f.stages.len().max(f.arity)
    }
}

impl Gen<'_> {
    /// The Go function or variable of a binding, `None` for one without a
    /// Go form, which is reported.
    pub(super) fn foreign(&mut self, f: &Foreign, span: Span) -> Option<String> {
        let Some(target) = lookup(self.module, &f.name, f.module.as_deref(), &f.export) else {
            let from = f
                .module
                .as_deref()
                .map_or_else(|| "global".to_owned(), |m| format!("from `{m}`"));
            self.errors.push((
                format!(
                    "`{}.{}` (foreign `{}` {from}) is not available on the go target",
                    self.module, f.name, f.export
                ),
                Some(span),
            ));
            return None;
        };
        let taken = parameters(f);
        if f.dicts == 0 && taken == 0 {
            let var = names::var(self.module, &f.name);
            return Some(format!("var {var} flickrt.Value = {target}"));
        }
        let heads: Vec<String> = (0..f.dicts)
            .map(|i| format!("d{i}"))
            .chain((0..taken).map(|i| format!("a{i}")))
            .collect();
        // a JS function of no arguments behind a `Unit` parameter
        let args = if f.arity == 0 {
            &heads[..f.dicts]
        } else {
            &heads[..]
        };
        Some(format!(
            "func {}({}) flickrt.Value {{\n\treturn {target}({})\n}}",
            names::func(self.module, &f.name),
            param_list(&heads),
            args.join(", ")
        ))
    }
}
