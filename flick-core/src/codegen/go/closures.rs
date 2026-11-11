//! The Go of functions as values and of calls: how a known function is
//! called, and the closures that take the arguments of one a stage or a few
//! at a time.

use std::iter;

use flick_pretty::{Document, DocumentArena, docvec};

use super::layout::{Code, block, call, closure, signature, statement};
use super::shape::Types;
use crate::ir::{Expr, ExprKind};

/// How to build a call from the fiber in scope and its Go arguments.
pub(super) type Call<'a> = Box<dyn Fn(&'static str, &[Code<'a>]) -> Code<'a> + 'a>;

/// The call of the Go function `f` with arguments that are values, after
/// the hidden execution context: `dicts` dictionaries, then the arguments
/// of its parameters, each made the Go type `types` gives its parameter.
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
                Some(native) => native.unbox(a, arg.doc),
                None => arg.doc,
            }
        });
        call(a, f, iter::once(a.text(fb)).chain(args)).into()
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
