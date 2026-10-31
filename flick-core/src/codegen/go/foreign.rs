//! `foreign` bindings: each JS export the target supports has a Go function
//! in the runtime that takes the binding's dictionaries and arguments at
//! once and works on Flick values (callbacks stay curried, `Option` stays
//! an `Option`). A binding the program reaches without one is reported.

use std::collections::HashMap;
use std::sync::OnceLock;

use flick_pretty::{Document, Documentable, docvec};

use super::layout::{call, closure, signature, single};
use super::{Gen, names};
use crate::ir::{Foreign, ForeignMode};
use crate::source::Span;

/// The binding tables, by area: under `[module]` (or `[global Module]` for
/// a JS global, which may mean different things for different types), an
/// export (or binding name) and its function of package `flickrt`.
const TABLES: &[&str] = &[
    include_str!("bindings/runtime.txt"),
    include_str!("bindings/effect.txt"),
    include_str!("bindings/platform.txt"),
    include_str!("bindings/stream.txt"),
    include_str!("bindings/cli.txt"),
    include_str!("bindings/cli_prompt.txt"),
    include_str!("bindings/datetime.txt"),
    include_str!("bindings/http.txt"),
    include_str!("bindings/layer.txt"),
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
                let go = match go.strip_prefix('*') {
                    Some(go) => format!("*flickrt.{go}"),
                    None => format!("flickrt.{go}"),
                };
                out.insert((module.to_owned(), export.to_owned()), go);
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

impl<'a> Gen<'a> {
    /// The Go function or variable of a binding, `None` for one without a
    /// Go form, which is reported.
    pub(super) fn foreign(&mut self, f: &Foreign, span: Span) -> Option<Document<'a>> {
        let a = self.a;
        let Some(target) = lookup(self.module, &f.name, f.module.as_deref(), &f.export) else {
            if let Some(spec) = f.module.as_deref().filter(|m| is_relative(m)) {
                return self.local(f, spec, span);
            }
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
        // `*` marks a runtime function that takes the fiber first
        let (target, wants_fiber) = match target.strip_prefix('*') {
            Some(t) => (t, true),
            None => (target, false),
        };
        let taken = parameters(f);
        if f.dicts == 0 && taken == 0 {
            let var = names::var(self.module, &f.name);
            return Some(docvec![a, "var ", var, " flickrt.Value = ", target]);
        }
        let heads: Vec<String> = (0..f.dicts)
            .map(|i| format!("d{i}"))
            .chain((0..taken).map(|i| format!("a{i}")))
            .collect();
        let mut args = arguments(f, &heads);
        if wants_fiber {
            args.insert(0, "fb".to_owned());
        }
        let go = call(a, target, args.into_iter().map(|arg| arg.to_doc(a)));
        Some(self.wrapper(f, signature(&heads), go))
    }

    /// The Go function of the binding `f` taking `params` and returning `go`.
    fn wrapper(&self, f: &Foreign, params: String, go: Document<'a>) -> Document<'a> {
        let a = self.a;
        let name = names::func(self.module, &f.name);
        let body = single(a, docvec![a, "return ", go]);
        docvec![a, "func ", name, "(", params, ") flickrt.Value ", body]
    }
}

/// The arguments a binding passes of its parameters `heads`: none behind a
/// `Unit` parameter of a JS function of no arguments.
fn arguments(f: &Foreign, heads: &[String]) -> Vec<String> {
    if f.arity == 0 {
        heads[..f.dicts].to_vec()
    } else {
        heads.to_vec()
    }
}

fn is_relative(spec: &str) -> bool {
    spec.starts_with("./") || spec.starts_with("../")
}

/// The Go file beside the foreign JS module `spec`.
fn sibling(spec: &str) -> String {
    let file = spec.rsplit('/').next().unwrap_or(spec);
    let stem = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
    format!("{stem}.go")
}

impl<'a> Gen<'a> {
    /// A binding to the JS module `spec` beside the program's code: on Go,
    /// the function of the same name in the Go file beside that module.
    fn local(&mut self, f: &Foreign, spec: &str, span: Span) -> Option<Document<'a>> {
        let a = self.a;
        let found = self.foreign_file.and_then(|find| find(self.module, spec));
        let Some((file, text)) = found else {
            self.errors.push((
                format!(
                    "`{}.{}` (foreign `{}` from `{spec}`) needs `{}` beside `{spec}` on the go target",
                    self.module,
                    f.name,
                    f.export,
                    sibling(spec)
                ),
                Some(span),
            ));
            return None;
        };
        let info = &f.info;
        if f.catch.is_some()
            || info.this
            || info.variadic
            || !info.optional.is_empty()
            || info.callbacks.iter().any(Option::is_some)
        {
            self.errors.push((
                format!(
                    "`{}.{}`: a foreign `catch`, `this`, optional or variadic arguments or adapted callbacks are not available on the go target yet",
                    self.module, f.name
                ),
                Some(span),
            ));
            return None;
        }
        self.sources.insert(file, text);
        let taken = parameters(f);
        if f.dicts == 0 && taken == 0 {
            let var = names::var(self.module, &f.name);
            return Some(docvec![
                a,
                "var ",
                var,
                " flickrt.Value = ",
                f.export.clone()
            ]);
        }
        let heads: Vec<String> = (0..f.dicts)
            .map(|i| format!("d{i}"))
            .chain((0..taken).map(|i| format!("a{i}")))
            .collect();
        let mut args = arguments(f, &heads);
        let first = match f.mode {
            ForeignMode::Pure | ForeignMode::Sync | ForeignMode::Effect => "fb",
            ForeignMode::Async => "ctx",
        };
        args.insert(0, first.to_owned());
        let export = a.alloc_str(&f.export);
        let mut go = call(a, export, args.into_iter().map(|arg| arg.to_doc(a)));
        if f.mode == ForeignMode::Async {
            let head = "func(ctx flickrt.Context) flickrt.Value";
            let run = closure(a, head, [docvec![a, "return ", go]]);
            go = call(a, "flickrt.ForeignAsync", [a.text("fb"), run]);
        }
        Some(self.wrapper(f, signature(&heads), go))
    }
}
