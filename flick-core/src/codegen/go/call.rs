//! Functions and calls: a known top-level function applied to all its
//! arguments is a direct Go call; any other application goes through the
//! curried value.

use std::{iter, mem, slice};

use flick_pretty::{Document, DocumentArena, docvec};

use super::layout::{Code, block, call, closure, signature, single, statement};
use super::{Gen, Shape, names};
use crate::codegen::tail::Target;
use crate::ir::{Expr, ExprKind, Lit, Param, Pat, Stage, operation_arity};
use crate::types::{Dict, Rep};

/// How to build a call from the fiber in scope and its Go arguments.
type Call<'a> = Box<dyn Fn(&'static str, &[Code<'a>]) -> Code<'a> + 'a>;

/// The call of the Go function `f`, with the hidden execution context.
fn callee<'a>(a: &'a DocumentArena<'a>, f: &str) -> Call<'a> {
    let f = a.alloc_str(f);
    Box::new(move |fb, args: &[Code<'a>]| {
        call(
            a,
            f,
            iter::once(a.text(fb)).chain(args.iter().map(|arg| arg.doc)),
        )
        .into()
    })
}

/// `return value`, the body of a closure.
fn returning<'a>(a: &'a DocumentArena<'a>, value: Document<'a>) -> [Document<'a>; 1] {
    [docvec![a, "return ", value]]
}

/// The curried function that makes `make` of `given`, then of `n` more
/// arguments taken one at a time; an arrow whose bit in `effects` is set
/// gives a task, the last one running the call in the task's fiber.
fn curried<'a>(
    a: &'a DocumentArena<'a>,
    make: &dyn Fn(&'static str, &[Code<'a>]) -> Code<'a>,
    given: Vec<Code<'a>>,
    n: usize,
    effects: u64,
    temp: &mut dyn FnMut() -> &'a str,
) -> Code<'a> {
    let heads: Vec<&'a str> = (0..n).map(|_| temp()).collect();
    let args: Vec<Code<'a>> = given
        .into_iter()
        .chain(heads.iter().map(|h| Code::leaf(a, h)))
        .collect();
    let last = n - 1;
    let mut body = if effects >> last & 1 == 1 {
        let run = make("fb", &args).doc;
        let task = closure(
            a,
            "func(fb *flickrt.Fiber) flickrt.Value",
            returning(a, run),
        );
        call(a, "flickrt.Eff", [task])
    } else {
        make("fb", &args).doc
    };
    for (i, h) in heads.iter().enumerate().rev() {
        if i < last && effects >> i & 1 == 1 {
            // an effectful stage before the last gives the next function, as Effect's `succeed`
            let task = closure(a, "func(*flickrt.Fiber) flickrt.Value", returning(a, body));
            body = call(a, "flickrt.Eff", [task]);
        }
        let head = docvec![
            a,
            "func(fb *flickrt.Fiber, ",
            *h,
            " flickrt.Value) flickrt.Value"
        ];
        body = call(a, "flickrt.Fn", [closure(a, head, returning(a, body))]);
    }
    body.into()
}

/// `flickrt.Poly` of a function of the dictionaries, which binds them to
/// `ds`, runs `stmts` and returns `value`.
pub(super) fn poly<'a>(
    a: &'a DocumentArena<'a>,
    ds: &[&'a str],
    stmts: Vec<Document<'a>>,
    value: Code<'a>,
) -> Document<'a> {
    let takes = ds.iter().enumerate().flat_map(|(i, d)| {
        [
            statement(a, docvec![a, *d, " := ds[", i, "]"]),
            statement(a, docvec![a, "_ = ", *d]),
        ]
    });
    let body = takes
        .chain(stmts)
        .chain([statement(a, docvec![a, "return ", value])]);
    let f = docvec![
        a,
        "func(fb *flickrt.Fiber, ds ...flickrt.Value) flickrt.Value ",
        block(a, body)
    ];
    call(a, "flickrt.Poly", [f])
}

impl<'a> Gen<'a> {
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
            ExprKind::Operation {
                service,
                name,
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
            } if dicts.len() == n => {
                let f = names::func(module, name);
                match params {
                    0 => call(
                        a,
                        f,
                        iter::once(self.context()).chain(dicts.into_iter().map(|d| d.doc)),
                    )
                    .into(),
                    1 if n == 0 && effects == 0 => self.code(&f),
                    _ => curried(a, &callee(a, &f), dicts, params, effects, &mut || {
                        self.temp()
                    }),
                }
            }
            // a constrained value passed on before its dictionaries
            Shape::Func {
                dicts: n,
                params,
                effects,
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
                    curried(a, &callee(a, &f), given, params, effects, &mut || {
                        self.temp()
                    })
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
            ExprKind::Con(c) => {
                let tag = a.alloc_str(&names::string(&self.tag(c)));
                let make: Call<'a> = match c.rep {
                    Rep::Tagged if c.arity == 0 => Box::new(move |_, _: &[Code<'a>]| {
                        call(a, "flickrt.Nullary", [a.text(tag)]).into()
                    }),
                    Rep::Tagged => Box::new(move |_, args: &[Code<'a>]| {
                        let args = args.iter().map(|arg| arg.doc);
                        call(a, "flickrt.NewData", iter::once(a.text(tag)).chain(args)).into()
                    }),
                    Rep::Unboxed => Box::new(|_, args: &[Code<'a>]| args[0]),
                    Rep::Typeof(_) => {
                        self.unsupported(head.span, "an `untagged` type");
                        Box::new(|_, args: &[Code<'a>]| args[0])
                    }
                };
                (make, Vec::new(), c.arity, 0)
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
        let exprs: Vec<&Expr> = args.iter().map(|(arg, _)| *arg).collect();
        let mut values: Vec<(Code<'a>, Stage)> = self
            .ordered(&exprs)
            .into_iter()
            .zip(args.iter().map(|(_, s)| *s))
            .collect();
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
        let all: Vec<Code<'a>> = given
            .into_iter()
            .chain(values.into_iter().map(|(v, _)| v))
            .collect();
        let mut v = make(self.context.unwrap_or("nil"), &all);
        for (arg, stage) in rest {
            v = call(a, "flickrt.Apply", [self.context(), v.doc, arg.doc]).into();
            if stage != Stage::Pure {
                v = self.run(v);
            }
        }
        v
    }

    /// The task `task` run in the fiber in scope.
    pub(super) fn run(&self, task: Code<'a>) -> Code<'a> {
        let fb = self.fiber.expect("effects run where a fiber is in scope");
        call(self.a, "flickrt.Run", [self.a.text(fb), task.doc]).into()
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
        let shape = self.shape_of(module, name);
        match shape {
            Shape::Func {
                dicts: n,
                params,
                effects,
            } if n == dicts.len() && params > 0 && (self.fiber.is_some() || !shape.effectful()) => {
                let f = names::func(module, name);
                (callee(self.a, &f), dicts, params, effects)
            }
            _ => {
                let f = self.global_value(module, name, dicts);
                (Box::new(move |_, _: &[Code<'a>]| f), Vec::new(), 0, 0)
            }
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
        let (heads, stmts) = self.function_body(params, body, target);
        (self.fiber, self.context) = saved;
        let body = if last_effectful {
            let task = docvec![a, "func(fb *flickrt.Fiber) flickrt.Value ", block(a, stmts)];
            single(a, docvec![a, "return ", call(a, "flickrt.Eff", [task])])
        } else {
            block(a, stmts)
        };
        let last = signature(&heads[heads.len() - 1..]);
        let mut text = docvec![a, "func(", last, ") flickrt.Value ", body];
        for (h, p) in heads[..heads.len() - 1].iter().zip(params).rev() {
            let mut next = call(a, "flickrt.Fn", [text]);
            if effectful(p) {
                // an effectful stage before the last gives the next function, as Effect's `succeed`
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
