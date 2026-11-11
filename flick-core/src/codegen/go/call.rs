//! Functions and calls: a known top-level function applied to all its
//! arguments is a direct Go call; any other application goes through the
//! curried value.

use std::{iter, mem, slice};

use flick_pretty::{Document, docvec};

use super::closures::{Call, at_once, callee, curried, poly, quiet, returning};
use super::expr::enum_literal;
use super::layout::{Code, block, call, closure, signature, single};
use super::shape::Types;
use super::{Gen, Shape, names};
use crate::codegen::tail::Target;
use crate::ir::{Expr, ExprKind, Lit, Param, Pat, Stage, operation_arity};
use crate::types::{Constructor, Dict, Rep};

impl<'a> Gen<'a> {
    /// Builds a value of the constructor `c` from its fields, holding the
    /// dictionaries `held` after them.
    fn construct(&mut self, c: &Constructor, held: Vec<Document<'a>>) -> Call<'a> {
        let a = self.a;
        let tag = a.alloc_str(&names::string(&self.tag(c)));
        match &c.rep {
            Rep::Literal(l) => {
                let lit = a.alloc_str(&enum_literal(l));
                Box::new(move |_, _: &[Code<'a>]| a.text(lit).into())
            }
            Rep::Tagged if c.arity == 0 && held.is_empty() => {
                let value = self.once(&format!("flickrt.Nullary({tag})"));
                Box::new(move |_, _: &[Code<'a>]| a.text(value).into())
            }
            Rep::Tagged => Box::new(move |_, args: &[Code<'a>]| {
                let args = args.iter().map(|arg| arg.doc).chain(held.iter().copied());
                call(a, "flickrt.NewData", iter::once(a.text(tag)).chain(args)).into()
            }),
            Rep::Unboxed | Rep::Typeof(_) => Box::new(|_, args: &[Code<'a>]| args[0]),
        }
    }

    /// A constrained value given its dictionaries.
    pub(super) fn given(&mut self, inner: &Expr, dicts: &[Dict]) -> Code<'a> {
        let dicts: Vec<Code<'a>> = dicts
            .iter()
            .map(|d| {
                let d = self.dict(d);
                self.spill(d)
            })
            .collect();
        match &inner.kind {
            ExprKind::Global { module, name } => self.global_value(module, name, dicts),
            // the implementation takes them, once the operation looked it up
            ExprKind::Perform {
                effect: service,
                op: name,
                stages,
            } => {
                let arity = self.a.alloc_str(&operation_arity(stages).to_string());
                let args = iter::once(self.string(service))
                    .chain(iter::once(self.string(name)))
                    .chain(iter::once(self.a.text(arity)))
                    .chain(dicts.iter().map(|d| d.doc));
                call(self.a, "flickrt.OperationGiven", args).into()
            }
            _ => {
                let f = self.expr(inner);
                let args =
                    iter::once(self.context()).chain(iter::once(f).chain(dicts).map(|d| d.doc));
                call(self.a, "flickrt.Inst", args).into()
            }
        }
    }

    /// A value that takes the dictionaries `shown` before it is computed.
    pub(super) fn abstraction(&mut self, shown: &[String], inner: &Expr) -> Code<'a> {
        let mark = self.givens.len();
        let dicts = self.bind_givens(shown);
        let saved = (
            self.tail.take(),
            mem::replace(&mut self.generator, false),
            self.fiber.take(),
            self.context.replace("fb"),
        );
        let (stmts, v) = self.block(|g| g.expr(inner));
        (self.tail, self.generator, self.fiber, self.context) = saved;
        self.givens.truncate(mark);
        poly(self.a, &dicts, stmts, v).into()
    }

    /// A top-level value of `module` with `dicts` given: a function of
    /// parameters becomes a curried value.
    pub(super) fn global_value(
        &mut self,
        module: &str,
        name: &str,
        dicts: Vec<Code<'a>>,
    ) -> Code<'a> {
        let a = self.a;
        self.reads(module);
        let shape = self.shape_of(module, name);
        match shape {
            Shape::Var => self.code(&names::var(module, name)),
            Shape::Func {
                dicts: n,
                params,
                effects,
                types,
            } if dicts.len() == n => {
                let f = names::func(module, name);
                // a function of values is a value as it is; one that takes
                // or gives a Go type is wrapped in one that converts them
                let plain = n == 0 && effects == 0 && !types.any();
                match params {
                    0 => call(
                        a,
                        f,
                        iter::once(self.context()).chain(dicts.into_iter().map(|d| d.doc)),
                    )
                    .into(),
                    1 if plain => self.code(&f),
                    // the Go function itself is the value: nothing wraps it
                    2 | 3 if plain => call(
                        a,
                        at_once(params, 0).expect("two or three"),
                        [a.text(a.alloc_str(&f))],
                    )
                    .into(),
                    _ => curried(
                        a,
                        &callee(a, &f, n, types),
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
                types,
            } => {
                let given: Vec<Code<'a>> = (0..n).map(|i| self.code(&format!("ds[{i}]"))).collect();
                let f = names::func(module, name);
                let inner = if params == 0 {
                    call(
                        a,
                        f,
                        iter::once(a.text("fb")).chain(given.into_iter().map(|d| d.doc)),
                    )
                } else {
                    curried(
                        a,
                        &callee(a, &f, n, types),
                        given,
                        params,
                        effects,
                        &mut || self.temp(),
                    )
                    .doc
                };
                let f = closure(
                    a,
                    "func(fb *flickrt.Fiber, ds ...flickrt.Value) flickrt.Value",
                    returning(a, inner),
                );
                call(a, "flickrt.Poly", [f]).into()
            }
        }
    }

    /// An application, or a constructor: a known function of all its
    /// arguments is called directly; an application of an effectful stage
    /// of any other value runs the task it gives.
    pub(super) fn app(&mut self, e: &Expr) -> Code<'a> {
        let a = self.a;
        let mut head = e;
        let mut first = None;
        if let ExprKind::App {
            func,
            arg,
            arg_first: true,
            stage,
        } = &e.kind
        {
            let v = self.expr(arg);
            first = Some((self.spill(v), *stage));
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
        let (make, given, arity, effects): (Call<'a>, Vec<Code<'a>>, usize, u64) = match &head.kind
        {
            ExprKind::Con(c) => (self.construct(c, Vec::new()), Vec::new(), c.arity, 0),
            // a constructor of a GADT with the dictionaries its value holds
            ExprKind::Dicts(inner, dicts) if matches!(inner.kind, ExprKind::Con(_)) => {
                let ExprKind::Con(c) = &inner.kind else {
                    unreachable!("a constructor, as the guard tested");
                };
                let carried = dicts.iter().map(|d| self.dict(d).doc).collect();
                (self.construct(c, carried), Vec::new(), c.arity, 0)
            }
            ExprKind::Global { module, name } => self.direct(module, name, Vec::new()),
            ExprKind::Dicts(inner, dicts) if matches!(inner.kind, ExprKind::Global { .. }) => {
                let ExprKind::Global { module, name } = &inner.kind else {
                    unreachable!("a global, as the guard tested");
                };
                let dicts: Vec<Code<'a>> = dicts.iter().map(|d| self.dict(d)).collect();
                self.direct(module, name, dicts)
            }
            _ => {
                let f = self.expr(head);
                let f = self.spill(f);
                (Box::new(move |_, _: &[Code<'a>]| f), Vec::new(), 0, 0)
            }
        };
        let typed = self.typed_head(head).filter(|(_, types, _)| types.any());
        let exprs: Vec<&Expr> = args.iter().map(|(arg, _)| *arg).collect();
        let mut values: Vec<(Code<'a>, Stage)> = self
            .ordered(&exprs)
            .into_iter()
            .zip(args.iter().map(|(_, s)| *s))
            .collect();
        // whether the value of each argument is ready with nothing to see:
        // a name, a literal or a function, and the one taken before the rest
        let mut quiet: Vec<bool> = exprs.iter().map(|e| quiet(e)).collect();
        quiet.extend(first.iter().map(|_| true));
        values.extend(first);
        if values.len() < arity {
            // a partial application keeps what it was given
            let taken = values.len();
            let given: Vec<Code<'a>> = given
                .into_iter()
                .chain(values.into_iter().map(|(v, _)| v))
                .map(|v| self.spill(v))
                .collect();
            return curried(
                a,
                &make,
                given,
                arity - taken,
                effects >> taken,
                &mut || self.temp(),
            );
        }
        let rest = values.split_off(arity);
        let quiet = quiet.split_off(arity.min(quiet.len()));
        let all: Vec<Code<'a>> = given
            .into_iter()
            .chain(values.into_iter().map(|(v, _)| v))
            .collect();
        let v = match typed {
            Some(typed) => self.typed_call(typed, &all, &exprs),
            None => make(self.context.unwrap_or("nil"), &all),
        };
        self.applied(v, rest, quiet)
    }

    /// The task `task` run in the fiber in scope.
    pub(super) fn run(&self, task: Code<'a>) -> Code<'a> {
        let fb = self.fiber.expect("effects run where a fiber is in scope");
        call(self.a, "flickrt.Run", [self.a.text(fb), task.doc]).into()
    }

    /// How a known function of `module` is called with `dicts` dictionaries
    /// and all its arguments, when that is a call of its Go function: how
    /// many parameters it takes, which run effects, and their Go types.
    pub(super) fn direct_shape(
        &self,
        module: &str,
        name: &str,
        dicts: usize,
    ) -> Option<(usize, u64, Types)> {
        let shape = self.shape_of(module, name);
        match shape {
            Shape::Func {
                dicts: n,
                params,
                effects,
                types,
            } if n == dicts && params > 0 && (self.fiber.is_some() || !shape.effectful()) => {
                Some((params, effects, types))
            }
            _ => None,
        }
    }

    /// How to call a top-level value of `module`: its Go function, how many
    /// parameters it takes after `dicts` and which run effects, or the
    /// value to apply.
    fn direct(
        &mut self,
        module: &str,
        name: &str,
        dicts: Vec<Code<'a>>,
    ) -> (Call<'a>, Vec<Code<'a>>, usize, u64) {
        self.reads(module);
        if let Some((params, effects, types)) = self.direct_shape(module, name, dicts.len()) {
            let f = names::func(module, name);
            let make = callee(self.a, &f, dicts.len(), types);
            (make, dicts, params, effects)
        } else {
            let f = self.global_value(module, name, dicts);
            (Box::new(move |_, _: &[Code<'a>]| f), Vec::new(), 0, 0)
        }
    }

    /// Binds the parameters of a function to Go parameter names; a pattern
    /// is destructured into the current statements.
    pub(super) fn bind_params(&mut self, params: &[Param]) -> Vec<&'a str> {
        let mut heads = Vec::new();
        for p in params {
            let name = match &p.pat {
                Pat::Bind(n) => self.a.alloc_str(&names::local(n)),
                Pat::Wild | Pat::Lit(Lit::Unit) => "_",
                pat => {
                    let t = self.temp();
                    self.destructure(pat, self.a.text(t));
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
    ) -> Code<'a> {
        let a = self.a;
        let effectful = |p: &Param| p.stage != Stage::Pure;
        let last_effectful = params.last().is_some_and(effectful);
        let saved = (
            mem::replace(&mut self.fiber, last_effectful.then_some("fb")),
            self.context.replace("fb"),
        );
        let (heads, stmts) = self.function_body(params, body, target, Types::default());
        (self.fiber, self.context) = saved;
        let body = if last_effectful {
            let task = docvec![a, "func(fb *flickrt.Fiber) flickrt.Value ", block(a, stmts)];
            single(a, docvec![a, "return ", call(a, "flickrt.Eff", [task])])
        } else {
            block(a, stmts)
        };
        // only the first three stages matter to a function of up to three
        let effects = params
            .iter()
            .take(3)
            .enumerate()
            .fold(0, |bits, (i, p)| bits | u64::from(effectful(p)) << i);
        if let Some(wrap) = at_once(params.len(), effects) {
            let all = signature(&heads);
            return call(
                a,
                wrap,
                [docvec![a, "func(", all, ") flickrt.Value ", body]],
            )
            .into();
        }
        let last = signature(&heads[heads.len() - 1..]);
        let mut text = docvec![a, "func(", last, ") flickrt.Value ", body];
        for (h, p) in heads[..heads.len() - 1].iter().zip(params).rev() {
            let mut next = call(a, "flickrt.Fn", [text]);
            if effectful(p) {
                // an effectful stage before the last is a task that gives the next function
                let task = single(a, docvec![a, "return ", next]);
                let task = docvec![a, "func(*flickrt.Fiber) flickrt.Value ", task];
                next = call(a, "flickrt.Eff", [task]);
            }
            let head = signature(slice::from_ref(h));
            text = docvec![
                a,
                "func(",
                head,
                ") flickrt.Value ",
                single(a, docvec![a, "return ", next])
            ];
        }
        call(a, "flickrt.Fn", [text]).into()
    }
}
