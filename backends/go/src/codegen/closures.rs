//! The Go of functions as values and of calls: how a known function is
//! called, and the closures that take the arguments of one a stage or a few
//! at a time.

use std::{iter, mem, slice};

use flick_backend_support::tail::Target;
use flick_pretty::{Document, DocumentArena, docvec};

use super::layout::{Code, block, call, closure, signature, single, statement};
use super::shape::Types;
use super::{Gen, names};
use flick_ir::{Dict, Expr, ExprKind, Lit, Param, Pat, Stage, operation_arity};

/// How to build a call from the fiber in scope and its Go arguments.
pub(super) type Call<'a> = Box<dyn Fn(&'static str, &[Code<'a>]) -> Code<'a> + 'a>;

/// The call of the Go function `f` with arguments that are values, after
/// the hidden execution context: `dicts` dictionaries, then the arguments
/// of its parameters, each made the Go type `types` gives its parameter,
/// and converting a typed `Option` return value back to erased `flickrt.Value`.
pub(super) fn callee<'a>(
    a: &'a DocumentArena<'a>,
    f: &str,
    dicts: usize,
    types: Types,
) -> Call<'a> {
    let f = a.alloc_str(f);
    Box::new(move |fb, args: &[Code<'a>]| {
        let args = args.iter().enumerate().map(|(i, arg)| {
            match i.checked_sub(dicts).and_then(|p| types.param(p)) {
                Some(gt) => gt.unbox_doc(a, arg.doc),
                None => arg.doc,
            }
        });
        let invoked = call(a, f, iter::once(a.text(fb)).chain(args));
        match types.result() {
            Some(gt) => gt.box_if_needed(a, invoked).into(),
            None => invoked.into(),
        }
    })
}

/// The runtime's function of `n` arguments taken at once, where it has one:
/// two or three whose stages before the last, by the bits of `effects`, are
/// pure. Given them all it is one call, with no function made per argument.
pub(super) fn at_once(n: usize, effects: u64) -> Option<&'static str> {
    let wrap = match n {
        2 => "flickrt.Fn2",
        3 => "flickrt.Fn3",
        _ => return None,
    };
    (effects & ((1 << (n - 1)) - 1) == 0).then_some(wrap)
}

/// Whether evaluating `e` is nothing anyone can see: it cannot fail, print
/// or run for ever, so it may be evaluated before a call written before it.
pub(super) fn quiet(e: &Expr) -> bool {
    matches!(
        e.kind,
        ExprKind::Local(_) | ExprKind::Lit(_) | ExprKind::Lambda { .. }
    )
}

/// `return value`, the body of a closure.
pub(super) fn returning<'a>(a: &'a DocumentArena<'a>, value: Document<'a>) -> [Document<'a>; 1] {
    [docvec![a, "return ", value]]
}

/// The curried function that makes `make` of `given`, then of `n` more
/// arguments taken one at a time; an arrow whose bit in `effects` is set
/// gives a task, the last one running the call in the task's fiber.
pub(super) fn curried<'a>(
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
    if let Some(wrap) = at_once(n, effects) {
        let head = format!("func({}) flickrt.Value", signature(&heads));
        return call(a, wrap, [closure(a, head, returning(a, body))]).into();
    }
    for (i, h) in heads.iter().enumerate().rev() {
        if i < last && effects >> i & 1 == 1 {
            // an effectful stage before the last is a task that gives the next function
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
            ExprKind::Global { module, name, .. } => self.global_value(module, name, dicts),
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
            self.returns.take(),
        );
        let (stmts, v) = self.block(|g| g.expr(inner));
        (
            self.tail,
            self.generator,
            self.fiber,
            self.context,
            self.returns,
        ) = saved;
        self.givens.truncate(mark);
        poly(self.a, &dicts, stmts, v).into()
    }

    /// Binds the parameters of a function to Go parameter names; a pattern
    /// is destructured into the current statements.
    pub(super) fn bind_params(&mut self, params: &[Param], types: &Types) -> Vec<&'a str> {
        let mut heads = Vec::new();
        for (i, p) in params.iter().enumerate() {
            let name = match &p.pat {
                Pat::Bind(n) => self.a.alloc_str(&names::local(n)),
                Pat::Wild | Pat::Lit(Lit::Unit) => "_",
                pat => {
                    let t = self.temp();
                    self.destructure(pat, self.a.text(t), types.param(i).cloned());
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
        let (heads, stmts) = self.function_body(params, body, target, &Types::default());
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
