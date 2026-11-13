//! Calls of functions that take or return Go's own types. A function of
//! the top level whose parameters or result are `Int`, `Float`, `Bool` or
//! `String` is a Go function of `int32`, `float64`, `bool` or `string`
//! there: a number passed to it or returned by it is no value made for the
//! call. Where such a function is a value, a function of values around it
//! converts them (`closures.rs`).

use super::arith::Native;
use super::layout::{Code, call};
use super::shape::Types;
use super::{Gen, names};
use crate::ir::{Expr, ExprKind, Stage};

impl<'a> Gen<'a> {
    /// The Go function the head of an application names, its Go types and
    /// how many parameters it takes, when a call of it with all its
    /// arguments is a call of that function.
    pub(super) fn typed_head(&self, head: &Expr) -> Option<(&'a str, Types, usize)> {
        let (module, name, dicts) = match &head.kind {
            ExprKind::Global { module, name } => (module, name, 0),
            ExprKind::Dicts(inner, dicts) => match &inner.kind {
                ExprKind::Global { module, name } => (module, name, dicts.len()),
                _ => return None,
            },
            _ => return None,
        };
        let (params, _, types) = self.direct_shape(module, name, dicts)?;
        Some((self.a.alloc_str(&names::func(module, name)), types, params))
    }

    /// The Go type of the application `e`, when `app` compiles it to a call
    /// of a known function, of all its arguments, that returns one.
    pub(super) fn typed_result(&self, e: &Expr) -> Option<Native> {
        let mut head = e;
        let mut given = 0;
        if let ExprKind::App {
            func,
            arg_first: true,
            ..
        } = &e.kind
        {
            (head, given) = (func, 1);
        }
        while let ExprKind::App {
            func,
            arg_first: false,
            ..
        } = &head.kind
        {
            (head, given) = (func, given + 1);
        }
        let (_, types, params) = self.typed_head(head)?;
        types.result().filter(|_| given == params)
    }

    /// The call `e` of the function `typed` with all its arguments, `all`:
    /// the dictionaries, then the values of `exprs`, then one taken before
    /// them. Each is in the Go type of its parameter, with no value made of
    /// one that is of it already.
    pub(super) fn typed_call(
        &mut self,
        (f, types, params): (&'a str, Types, usize),
        all: &[Code<'a>],
        exprs: &[&Expr],
        e: &Expr,
    ) -> Code<'a> {
        let a = self.a;
        let dicts = all.len() - params;
        // the Go types of a binding are written in a table: they must be
        // those of the types the checker gave the call
        let differs = |go: Option<Native>, of: &Expr| {
            go.zip(Native::of(self.input.lang, of.ty.as_deref()))
                .is_some_and(|(go, flick)| go != flick)
        };
        let mut wrong = differs(types.result(), e);
        let mut args = vec![self.context()];
        for (i, arg) in all.iter().enumerate() {
            let native = i.checked_sub(dicts).and_then(|p| types.param(p));
            args.push(match (native, exprs.get(i.wrapping_sub(dicts))) {
                (Some(native), Some(of)) => {
                    wrong |= differs(Some(native), of);
                    self.unboxed(of, *arg, native).doc
                }
                // the argument taken before the others is a value by now
                (Some(native), None) => native.unbox(a, arg.doc),
                (None, _) => arg.doc,
            });
        }
        if wrong {
            let message = format!("the Go types of `{f}` are not those of its Flick type");
            self.errors.push((message, Some(e.span)));
        }
        call(a, f, args).into()
    }

    /// `v` applied to the arguments `rest` that its head did not take, each
    /// with the stage of its application and whether it is quiet.
    pub(super) fn applied(
        &mut self,
        mut v: Code<'a>,
        rest: Vec<(Code<'a>, Stage)>,
        quiet: Vec<bool>,
    ) -> Code<'a> {
        let a = self.a;
        let mut rest = rest.into_iter().zip(quiet).peekable();
        while let Some(((arg, mut stage), _)) = rest.next() {
            // an argument of a pure stage goes with the next, up to three
            // at once: a function that takes them so is called once. The
            // next one must be quiet: a function may fail, or not return,
            // once it has an argument, and that comes before the next
            // argument is evaluated
            let mut group = vec![self.context(), v.doc, arg.doc];
            while group.len() < 5 && stage == Stage::Pure {
                let Some(((next, after), _)) = rest.next_if(|(_, quiet)| *quiet) else {
                    break;
                };
                group.push(next.doc);
                stage = after;
            }
            let apply = ["flickrt.Apply", "flickrt.Apply2", "flickrt.Apply3"][group.len() - 3];
            v = call(a, apply, group).into();
            if stage != Stage::Pure {
                v = self.run(v);
            }
        }
        v
    }
}
