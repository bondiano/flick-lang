//! Functions and calls: a known top-level function applied to all its
//! arguments is a direct Go call; any other application goes through the
//! curried value.

use std::{mem, slice};

use super::{Gen, Shape, indent, names, param_list};
use crate::codegen::tail::Target;
use crate::ir::{Expr, ExprKind, Lit, Param, Pat, Stage};
use crate::types::{Dict, Rep};

/// How to build a call from the fiber in scope and its Go arguments.
type Call = Box<dyn Fn(Option<&str>, &[String]) -> String>;

/// The call of the Go function `f`, with the fiber first when it takes one.
fn callee(f: String, fiber: bool) -> Call {
    Box::new(move |fb, a: &[String]| {
        let fb = fb
            .filter(|_| fiber)
            .map(|fb| format!("{fb}, "))
            .unwrap_or_default();
        format!("{f}({fb}{})", a.join(", "))
    })
}

/// The curried function that makes `call` of `given`, then of `n` more
/// arguments taken one at a time; an arrow whose bit in `effects` is set
/// gives a task, the last one running the call in the task's fiber.
fn curried(
    call: &dyn Fn(Option<&str>, &[String]) -> String,
    given: Vec<String>,
    n: usize,
    effects: u64,
    temp: &mut dyn FnMut() -> String,
) -> String {
    let heads: Vec<String> = (0..n).map(|_| temp()).collect();
    let args: Vec<String> = given.into_iter().chain(heads.iter().cloned()).collect();
    let last = n - 1;
    let mut body = if effects >> last & 1 == 1 {
        format!(
            "flickrt.Eff(func(fb *flickrt.Fiber) flickrt.Value {{ return {} }})",
            call(Some("fb"), &args)
        )
    } else {
        call(None, &args)
    };
    for (i, h) in heads.iter().enumerate().rev() {
        if i < last && effects >> i & 1 == 1 {
            // an effectful stage before the last gives the next function, as Effect's `succeed`
            body = format!("flickrt.Eff(func(*flickrt.Fiber) flickrt.Value {{ return {body} }})");
        }
        body = format!("flickrt.Fn(func({h} flickrt.Value) flickrt.Value {{ return {body} }})");
    }
    body
}

impl Gen<'_> {
    /// A constrained value given its dictionaries.
    pub(super) fn given(&mut self, inner: &Expr, dicts: &[Dict]) -> String {
        let dicts: Vec<String> = dicts
            .iter()
            .map(|d| {
                let d = self.dict(d);
                self.spill(d)
            })
            .collect();
        if let ExprKind::Global { module, name } = &inner.kind {
            return self.global_value(module, name, dicts);
        }
        let f = self.expr(inner);
        format!("flickrt.Inst({f}, {})", dicts.join(", "))
    }

    /// A value that takes the dictionaries `shown` before it is computed.
    pub(super) fn abstraction(&mut self, shown: &[String], inner: &Expr) -> String {
        let mark = self.givens.len();
        let dicts = self.bind_givens(shown);
        let saved = (
            self.tail.take(),
            mem::replace(&mut self.generator, false),
            self.fiber.take(),
        );
        let (mut stmts, v) = self.block(|g| g.expr(inner));
        (self.tail, self.generator, self.fiber) = saved;
        self.givens.truncate(mark);
        let takes: Vec<String> = dicts
            .iter()
            .enumerate()
            .map(|(i, d)| format!("{d} := ds[{i}]\n_ = {d}"))
            .collect();
        stmts.splice(0..0, takes);
        stmts.push(format!("return {v}"));
        format!(
            "flickrt.Poly(func(ds ...flickrt.Value) flickrt.Value {{\n{}}})",
            indent(&stmts)
        )
    }

    /// A top-level value of `module` with `dicts` given: a function of
    /// parameters becomes a curried value.
    pub(super) fn global_value(&mut self, module: &str, name: &str, dicts: Vec<String>) -> String {
        self.reads(module);
        let shape = self.shape_of(module, name);
        match shape {
            Shape::Var => names::var(module, name),
            Shape::Func {
                dicts: n,
                params,
                effects,
            } if dicts.len() == n => {
                let f = names::func(module, name);
                match params {
                    0 => format!("{f}({})", dicts.join(", ")),
                    1 if n == 0 && effects == 0 => f,
                    _ => curried(
                        &callee(f, shape.fiber()),
                        dicts,
                        params,
                        effects,
                        &mut || self.temp(),
                    ),
                }
            }
            // a constrained value passed on before its dictionaries
            Shape::Func {
                dicts: n,
                params,
                effects,
            } => {
                let given: Vec<String> = (0..n).map(|i| format!("ds[{i}]")).collect();
                let f = names::func(module, name);
                let inner = if params == 0 {
                    format!("{f}({})", given.join(", "))
                } else {
                    curried(
                        &callee(f, shape.fiber()),
                        given,
                        params,
                        effects,
                        &mut || self.temp(),
                    )
                };
                format!(
                    "flickrt.Poly(func(ds ...flickrt.Value) flickrt.Value {{ return {inner} }})"
                )
            }
        }
    }

    /// An application, or a constructor: a known function of all its
    /// arguments is called directly; an application of an effectful stage
    /// of any other value runs the task it gives.
    pub(super) fn app(&mut self, e: &Expr) -> String {
        let mut head = e;
        let mut first = None;
        if let ExprKind::App {
            func,
            arg,
            arg_first: true,
            stage,
        } = &e.kind
        {
            let a = self.expr(arg);
            first = Some((self.spill(a), *stage));
            head = func;
        }
        let mut args = Vec::new();
        while let ExprKind::App {
            func,
            arg,
            arg_first: false,
            stage,
        } = &head.kind
        {
            args.push((&**arg, *stage));
            head = func;
        }
        args.reverse();
        let (call, given, arity, effects): (Call, Vec<String>, usize, u64) = match &head.kind {
            ExprKind::Con(c) => {
                let tag = names::string(&self.tag(c));
                let call: Call = match c.rep {
                    Rep::Tagged if c.arity == 0 => {
                        Box::new(move |_, _| format!("flickrt.Nullary({tag})"))
                    }
                    Rep::Tagged => Box::new(move |_, a: &[String]| {
                        format!("flickrt.NewData({tag}, {})", a.join(", "))
                    }),
                    Rep::Unboxed => Box::new(|_, a: &[String]| a[0].clone()),
                    Rep::Typeof(_) => {
                        self.unsupported(head.span, "an `untagged` type");
                        Box::new(|_, a: &[String]| a[0].clone())
                    }
                };
                (call, Vec::new(), c.arity, 0)
            }
            ExprKind::Global { module, name } => self.direct(module, name, Vec::new()),
            ExprKind::Dicts(inner, dicts) if matches!(inner.kind, ExprKind::Global { .. }) => {
                let ExprKind::Global { module, name } = &inner.kind else {
                    unreachable!("a global, as the guard tested");
                };
                let dicts: Vec<String> = dicts.iter().map(|d| self.dict(d)).collect();
                self.direct(module, name, dicts)
            }
            _ => {
                let f = self.expr(head);
                let f = self.spill(f);
                (Box::new(move |_, _| f.clone()), Vec::new(), 0, 0)
            }
        };
        let exprs: Vec<&Expr> = args.iter().map(|(a, _)| *a).collect();
        let mut values: Vec<(String, Stage)> = self
            .ordered(&exprs)
            .into_iter()
            .zip(args.iter().map(|(_, s)| *s))
            .collect();
        values.extend(first);
        if values.len() < arity {
            // a partial application keeps what it was given
            let taken = values.len();
            let given: Vec<String> = given
                .into_iter()
                .chain(values.into_iter().map(|(v, _)| v))
                .map(|v| self.spill(v))
                .collect();
            let fb = self.fiber;
            return curried(
                &|inner, a| call(inner.or(fb), a),
                given,
                arity - taken,
                effects >> taken,
                &mut || self.temp(),
            );
        }
        let rest = values.split_off(arity);
        let all: Vec<String> = given
            .into_iter()
            .chain(values.into_iter().map(|(v, _)| v))
            .collect();
        let mut v = call(self.fiber, &all);
        for (a, stage) in rest {
            v = format!("flickrt.Apply({v}, {a})");
            if stage != Stage::Pure {
                v = self.run(&v);
            }
        }
        v
    }

    /// The task `task` run in the fiber in scope.
    pub(super) fn run(&mut self, task: &str) -> String {
        let fb = self.fiber.expect("effects run where a fiber is in scope");
        format!("flickrt.Run({fb}, {task})")
    }

    /// How to call a top-level value of `module`: its Go function, how many
    /// parameters it takes after `dicts` and which run effects, or the
    /// value to apply.
    fn direct(
        &mut self,
        module: &str,
        name: &str,
        dicts: Vec<String>,
    ) -> (Call, Vec<String>, usize, u64) {
        self.reads(module);
        let shape = self.shape_of(module, name);
        match shape {
            Shape::Func {
                dicts: n,
                params,
                effects,
            } if n == dicts.len() && params > 0 && (self.fiber.is_some() || !shape.fiber()) => {
                let f = names::func(module, name);
                (callee(f, shape.fiber()), dicts, params, effects)
            }
            _ => {
                let f = self.global_value(module, name, dicts);
                (Box::new(move |_, _| f.clone()), Vec::new(), 0, 0)
            }
        }
    }

    /// Binds the parameters of a function to Go parameter names; a pattern
    /// is destructured into the current statements.
    pub(super) fn bind_params(&mut self, params: &[Param]) -> Vec<String> {
        let mut heads = Vec::new();
        for p in params {
            let name = match &p.pat {
                Pat::Bind(n) => names::local(n),
                Pat::Wild | Pat::Lit(Lit::Unit) => "_".to_owned(),
                pat => {
                    let t = self.temp();
                    self.destructure(pat, &t);
                    t
                }
            };
            heads.push(name);
        }
        heads
    }

    /// A curried function: one Go function per stage, the body at the last.
    pub(super) fn lambda(
        &mut self,
        params: &[Param],
        body: &Expr,
        target: Option<Target>,
    ) -> String {
        let effectful = |p: &Param| p.stage != Stage::Pure;
        let last_effectful = params.last().is_some_and(effectful);
        let saved = mem::replace(&mut self.fiber, last_effectful.then_some("fb"));
        let (heads, stmts) = self.function_body(params, body, target);
        self.fiber = saved;
        let body = if last_effectful {
            let task = format!(
                "flickrt.Eff(func(fb *flickrt.Fiber) flickrt.Value {{\n{}}})",
                indent(&stmts)
            );
            format!("\treturn {}\n", task.replace('\n', "\n\t"))
        } else {
            indent(&stmts)
        };
        let mut text = format!(
            "func({}) flickrt.Value {{\n{body}}}",
            param_list(&heads[heads.len() - 1..]),
        );
        for (h, p) in heads[..heads.len() - 1].iter().zip(params).rev() {
            let mut next = format!("flickrt.Fn({})", text.replace('\n', "\n\t"));
            if effectful(p) {
                // an effectful stage before the last gives the next function, as Effect's `succeed`
                next =
                    format!("flickrt.Eff(func(*flickrt.Fiber) flickrt.Value {{ return {next} }})");
            }
            text = format!(
                "func({}) flickrt.Value {{\n\treturn {next}\n}}",
                param_list(slice::from_ref(h)),
            );
        }
        format!("flickrt.Fn({text})")
    }
}
