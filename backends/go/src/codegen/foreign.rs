//! `foreign` bindings: each JS export the target supports has a Go function
//! in the runtime that takes the binding's dictionaries and arguments at
//! once and works on Flick values (callbacks stay curried, `Option` stays
//! an `Option`). A binding the program reaches without one is reported.

use std::collections::HashMap;

use flick_pretty::{Document, Documentable, docvec};

use super::arith::Native;
use super::layout::{call, closure, list, signature, single, typed_signature};
use super::shape::Types;
use super::{Gen, names};
use flick_ir::Span;
use flick_ir::{Foreign, ForeignMode};

/// What a table says of a Go function written `Name(float64, int32) bool`:
/// its name, and the Go types it takes and gives where a binding without
/// them takes and gives values.
fn typed(go: &str) -> (&str, Types) {
    let Some((name, rest)) = go.split_once('(') else {
        return (go, Types::default());
    };
    let (params, result) = rest
        .split_once(')')
        .expect("the result after the parameters");
    let native = |ty: &str| Some(Native::named(ty.trim()).expect("a Go type of a scalar"));
    let params: Vec<Option<Native>> = params.split(',').map(native).collect();
    (name, Types::of(native(result), &params))
}

/// The Go functions of `flickrt` that the tables of the implicit packages
/// bind JS exports to, and the Go types each takes and gives.
struct Binding {
    target: String,
    types: Types,
    /// An opt-in Go function returning the remaining Flick stages as Fn.
    stages: Option<usize>,
}

pub struct Bindings(HashMap<(String, String), Binding>);

impl Bindings {
    /// The tables `texts`, by area: under `[module]` (or `[global Module]`
    /// for a JS global, which may mean different things for different
    /// types), an export (or binding name) and its function of `flickrt`.
    /// `*` passes Fiber first; `@N` keeps N stages and returns Fn for the rest.
    pub fn parse<'t>(texts: impl IntoIterator<Item = &'t str>) -> Bindings {
        let mut out = HashMap::new();
        for text in texts {
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
                let (go, stages) = match go.rsplit_once('@') {
                    Some((go, count)) => {
                        (go, Some(count.parse::<usize>().expect("a staged Go arity")))
                    }
                    None => (go, None),
                };
                let (go, types) = typed(go);
                let go = match go.strip_prefix('*') {
                    Some(go) => format!("*flickrt.{go}"),
                    None => format!("flickrt.{go}"),
                };
                let earlier = out.insert(
                    (module.to_owned(), export.to_owned()),
                    Binding {
                        target: go,
                        types,
                        stages,
                    },
                );
                assert!(earlier.is_none(), "`{export}` of `{module}` is bound twice");
            }
        }
        Bindings(out)
    }

    /// The Go functions and variables the tables name.
    pub fn targets(&self) -> impl Iterator<Item = &str> {
        (self.0.values()).map(|binding| {
            binding
                .target
                .trim_start_matches('*')
                .trim_start_matches("flickrt.")
        })
    }

    /// The Go function of the binding `name` of `module` to `export` of
    /// `from`, and the Go types it takes and gives.
    pub fn lookup(
        &self,
        module: &str,
        name: &str,
        from: Option<&str>,
        export: &str,
    ) -> Option<(&str, Types)> {
        self.binding(module, name, from, export)
            .map(|binding| (binding.target.as_str(), binding.types))
    }

    fn binding(
        &self,
        module: &str,
        name: &str,
        from: Option<&str>,
        export: &str,
    ) -> Option<&Binding> {
        let key = match from {
            Some(from) => (from.to_owned(), export.to_owned()),
            None => (format!("global {module}"), name.to_owned()),
        };
        self.0.get(&key)
    }

    /// The stages the Go function takes. Most bindings flatten all stages;
    /// an explicit `@N` mounts once and returns Fn for its remaining stages.
    pub fn parameters(&self, module: &str, f: &Foreign) -> usize {
        self.binding(
            module,
            f.source_name.as_deref().unwrap_or(&f.name),
            f.binding.from.as_deref(),
            &f.binding.name,
        )
        .and_then(|binding| binding.stages)
        .unwrap_or_else(|| parameters(f))
    }

    /// The Go types of the function the binding `f` of `module` is bound to.
    pub fn types(&self, module: &str, f: &Foreign) -> Types {
        self.lookup(
            module,
            f.source_name.as_deref().unwrap_or(&f.name),
            f.binding.from.as_deref(),
            &f.binding.name,
        )
        .map_or_else(Types::default, |(_, types)| types)
    }
}

/// How many parameters the Go function of a binding takes after its
/// dictionaries: every stage, though the JS function takes `arity` and
/// returns a function of the rest, the data-last form.
pub(super) fn parameters(f: &Foreign) -> usize {
    if f.arity == 0 {
        f.taken()
    } else {
        f.stages.len().max(f.arity)
    }
}

impl<'a> Gen<'a> {
    /// The Go function or variable of a binding, `None` for one without a
    /// Go form, which is reported.
    pub(super) fn foreign(&mut self, f: &Foreign, span: Span) -> Option<Document<'a>> {
        let a = self.a;
        let Some((target, types)) = self.input.bindings.lookup(
            self.module,
            f.source_name.as_deref().unwrap_or(&f.name),
            f.binding.from.as_deref(),
            &f.binding.name,
        ) else {
            if let Some(spec) = f.binding.from.as_deref().filter(|m| is_relative(m)) {
                return self.local(f, spec, span);
            }
            let from = f
                .binding
                .from
                .as_deref()
                .map_or_else(|| "global".to_owned(), |m| format!("from `{m}`"));
            self.errors.push((
                format!(
                    "`{}.{}` (foreign `{}` {from}) is not available on the go target",
                    self.module, f.name, f.binding.name
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
        let taken = self.input.bindings.parameters(self.module, f);
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
        // a function of Go's own types is given and gives them as they are
        let heads: Vec<&str> = heads.iter().map(String::as_str).collect();
        let result = types.result().map_or("flickrt.Value", Native::go);
        Some(self.wrapper(f, typed_signature(&heads, f.dicts, types), result, go))
    }

    /// The Go function of the binding `f` taking `params` and returning
    /// `go`, a `result`.
    fn wrapper(
        &self,
        f: &Foreign,
        params: String,
        result: &'static str,
        go: Document<'a>,
    ) -> Document<'a> {
        let a = self.a;
        let name = names::func(self.module, &f.name);
        let body = single(a, docvec![a, "return ", go]);
        docvec![a, "func ", name, "(", params, ") ", result, " ", body]
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

/// Whether a foreign module is named relative to the module binding it.
pub fn is_relative(spec: &str) -> bool {
    spec.starts_with("./") || spec.starts_with("../")
}

/// The Go file beside the foreign JS module `spec`.
fn sibling(spec: &str) -> String {
    let file = spec.rsplit('/').next().unwrap_or(spec);
    let stem = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
    format!("{stem}.go")
}

impl<'a> Gen<'a> {
    fn local_receiver_call(
        &self,
        f: &Foreign,
        first: &'a str,
        heads: &[&'a str],
        count: usize,
    ) -> Document<'a> {
        let a = self.a;
        let info = &f.adapter;
        let receiver_name = heads[f.dicts];
        let mut method_args: Vec<Document<'a>> = Vec::new();
        for i in 1..count {
            let param_name = heads[f.dicts + i];
            if info.optional.contains(&i) {
                method_args.push(call(a, "flickrt.ToOptVal", [a.text(param_name)]));
            } else if let Some(Some(n)) = info.callbacks.get(i) {
                method_args.push(call(
                    a,
                    "flickrt.WrapCallback",
                    [
                        a.text("fb"),
                        a.text(param_name),
                        a.text(a.alloc_str(&n.to_string())),
                    ],
                ));
            } else if info.variadic && i + 1 == f.arity {
                method_args.push(call(a, "flickrt.List", [a.text(param_name)]));
            } else {
                method_args.push(a.text(param_name));
            }
        }
        let args_slice = if info.variadic && count > 1 {
            if let Some(last) = method_args.pop() {
                if method_args.is_empty() {
                    last
                } else {
                    let fixed = list(a, "[]flickrt.Value{", method_args, "}");
                    call(a, "append", [fixed, docvec![a, last, "..."]])
                }
            } else {
                list(a, "[]flickrt.Value{", method_args, "}")
            }
        } else {
            list(a, "[]flickrt.Value{", method_args, "}")
        };
        let method_lit = a.alloc_str(&format!("\"{}\"", f.binding.name));
        call(
            a,
            "flickrt.CallMethod",
            [
                a.text(first),
                a.text(receiver_name),
                a.text(method_lit),
                args_slice,
            ],
        )
    }

    fn local_free_call(
        &self,
        f: &Foreign,
        first: &'a str,
        heads: &[&'a str],
        count: usize,
    ) -> Document<'a> {
        let a = self.a;
        let info = &f.adapter;
        let mut call_args: Vec<Document<'a>> = Vec::new();
        call_args.push(a.text(first));
        for &head in heads.iter().take(f.dicts) {
            call_args.push(a.text(head));
        }
        for i in 0..count {
            let param_name = heads[f.dicts + i];
            if info.optional.contains(&i) {
                call_args.push(call(a, "flickrt.ToOptVal", [a.text(param_name)]));
            } else if let Some(Some(n)) = info.callbacks.get(i) {
                call_args.push(call(
                    a,
                    "flickrt.WrapCallback",
                    [
                        a.text("fb"),
                        a.text(param_name),
                        a.text(a.alloc_str(&n.to_string())),
                    ],
                ));
            } else if info.variadic && i + 1 == f.arity {
                call_args.push(docvec![a, "flickrt.List(", param_name, ")..."]);
            } else {
                call_args.push(a.text(param_name));
            }
        }
        let export = a.alloc_str(&f.binding.name);
        call(a, export, call_args)
    }

    /// A binding to the JS module `spec` beside the program's code: on Go,
    /// the function of the same name in the Go file beside that module.
    fn local(&mut self, f: &Foreign, spec: &str, span: Span) -> Option<Document<'a>> {
        let a = self.a;
        let found = (self.input.foreign).get(&(self.module.to_owned(), spec.to_owned()));
        let Some((file, text)) = found else {
            self.errors.push((
                format!(
                    "`{}.{}` (foreign `{}` from `{spec}`) needs `{}` beside `{spec}` on the go target",
                    self.module,
                    f.name,
                    f.binding.name,
                    sibling(spec)
                ),
                Some(span),
            ));
            return None;
        };
        self.sources.insert(file.clone(), text.clone());
        let taken = parameters(f);
        if f.dicts == 0 && taken == 0 {
            let var = names::var(self.module, &f.name);
            return Some(docvec![
                a,
                "var ",
                var,
                " flickrt.Value = ",
                f.binding.name.clone()
            ]);
        }
        let heads: Vec<&'a str> = (0..f.dicts)
            .map(|i| a.alloc_str(&format!("d{i}")))
            .chain((0..taken).map(|i| a.alloc_str(&format!("a{i}"))))
            .collect();

        let info = &f.adapter;
        let first = match f.mode {
            ForeignMode::Pure | ForeignMode::Sync | ForeignMode::Effect => "fb",
            ForeignMode::Async => "ctx",
        };
        let count = if f.arity == 0 { 0 } else { taken };

        let mut go = if info.receiver {
            self.local_receiver_call(f, first, &heads, count)
        } else {
            self.local_free_call(f, first, &heads, count)
        };

        if let Some(catch_expr) = &f.catch {
            let catch_code = self.expr(catch_expr);
            let catch_doc = catch_code.to_doc(a);
            let body = if info.receiver {
                docvec![a, "return ", go, ", nil"]
            } else {
                docvec![a, "return ", go]
            };
            let run = closure(a, "func() (flickrt.Value, error)", [body]);
            go = call(a, "flickrt.CatchForeign", [a.text("fb"), run, catch_doc]);
        }

        if info.result.nullable {
            go = call(a, "flickrt.FromNullable", [go]);
        }

        if info.result.unit {
            go = call(a, "flickrt.ToUnit", [go]);
        }

        if f.mode == ForeignMode::Async {
            let head = "func(ctx flickrt.Context) flickrt.Value";
            let run = closure(a, head, [docvec![a, "return ", go]]);
            go = call(a, "flickrt.ForeignAsync", [a.text("fb"), run]);
        }
        Some(self.wrapper(f, signature(&heads), "flickrt.Value", go))
    }
}
