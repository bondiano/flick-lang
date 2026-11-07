//! Effectful expressions: tasks, the scopes and regions they run in, typed
//! errors and the early return of `?`.

use std::mem;

use flick_pretty::{Document, Documentable, docvec};

use super::layout::{Code, block, call, single, statement};
use super::{Gen, names};
use crate::ir::{Arm, Expr, Stage};

impl<'a> Gen<'a> {
    /// A task: a Go function of the fiber that runs `body` in it.
    pub(super) fn task(&mut self, body: &Expr) -> Code<'a> {
        let a = self.a;
        let body = block(a, self.effectful_body(body));
        let run = docvec![a, "func(fb *flickrt.Fiber) flickrt.Value ", body];
        call(a, "flickrt.Eff", [run]).into()
    }

    /// The statements of a Go function of a fiber `fb` that returns `body`.
    fn effectful_body(&mut self, body: &Expr) -> Vec<Document<'a>> {
        let saved = (
            self.tail.take(),
            mem::replace(&mut self.generator, false),
            self.fiber.replace("fb"),
            self.context.replace("fb"),
        );
        let (stmts, ()) = self.block(|g| g.tail_expr(body));
        (self.tail, self.generator, self.fiber, self.context) = saved;
        stmts
    }

    /// `scope [owner] { body }`: the body runs as the owner of a fresh
    /// region, given it and the owner around it.
    pub(super) fn scope(
        &mut self,
        owner: Option<&str>,
        outer: Option<&str>,
        body: &Expr,
    ) -> Code<'a> {
        let a = self.a;
        let fb = self
            .fiber
            .expect("a `scope` runs where a fiber is in scope");
        let name = |n: Option<&str>| n.map_or_else(|| "_".to_owned(), names::local);
        let head = format!(
            "func(fb *flickrt.Fiber, {}, {} flickrt.Value) flickrt.Value ",
            name(owner),
            name(outer)
        );
        let body = block(a, self.effectful_body(body));
        call(a, "flickrt.Scope", [a.text(fb), docvec![a, head, body]]).into()
    }

    /// A call that acquires into the region of `owner`: run in a fiber
    /// whose owner it is.
    pub(super) fn within(&mut self, owner: &str, inner: &Expr) -> Code<'a> {
        let a = self.a;
        let fb = self.fiber.expect("`Within` runs where a fiber is in scope");
        let saved = (self.fiber.replace("fbw"), self.context.replace("fbw"));
        let (stmts, v) = self.block(|g| g.expr(inner));
        (self.fiber, self.context) = saved;
        let r = self.temp();
        let region = call(
            a,
            "flickrt.Within",
            [a.text(fb), names::local(owner).to_doc(a)],
        );
        let stmts = [statement(a, docvec![a, "fbw := ", region])]
            .into_iter()
            .chain(stmts)
            .chain([statement(a, docvec![a, r, " = ", v])]);
        self.emit(docvec![a, "var ", r, " flickrt.Value"]);
        self.emit(block(a, stmts));
        self.code(r)
    }

    /// `try body with arms`: the body as a task, and a handler of its typed
    /// error that gives `flickrt.Unmatched` when no arm matches.
    pub(super) fn try_expr(&mut self, body: &Expr, arms: &[Arm]) -> Code<'a> {
        let a = self.a;
        let body = self.task(body);
        let handler = self.error_handler(arms);
        let fb = self.fiber.expect("a `try` with arms runs in a fiber");
        call(a, "flickrt.Try", [a.text(fb), body.doc, handler]).into()
    }

    /// The handler of the typed errors of `arms`: a Go function of the fiber
    /// and the error that gives `flickrt.Unmatched` when no arm matches.
    pub(super) fn error_handler(&mut self, arms: &[Arm]) -> Document<'a> {
        let a = self.a;
        let saved = (
            self.tail.take(),
            mem::replace(&mut self.generator, false),
            self.fiber.replace("fb"),
            self.context.replace("fb"),
        );
        let err = self.temp();
        let (mut stmts, ()) = self.block(|g| {
            for arm in arms {
                g.arm(
                    a.text(err),
                    &arm.pat,
                    arm.guard.as_ref(),
                    &arm.body,
                    |g, value| {
                        let v = g.expr(value);
                        g.emit(docvec![a, "return ", v]);
                    },
                );
            }
        });
        (self.tail, self.generator, self.fiber, self.context) = saved;
        stmts.push(statement(a, "return flickrt.Unmatched"));
        docvec![
            a,
            "func(fb *flickrt.Fiber, ",
            err,
            " flickrt.Value) flickrt.Value ",
            block(a, stmts)
        ]
    }

    /// `o?`: the value of the `Some`, after returning a `None` at once.
    pub(super) fn unwrap(&mut self, option: &Expr) -> Code<'a> {
        let a = self.a;
        let v = self.expr(option);
        let v = self.spill(v);
        // in a lazy `for` a `None` ends the sequence, as a JS generator's return does
        let ret = if self.generator {
            a.text("return")
        } else {
            docvec![a, "return ", v]
        };
        let none = call(a, "flickrt.IsTag", [v.doc, a.text("\"None\"")]);
        self.emit(docvec![a, "if ", none, " ", single(a, ret)]);
        call(a, "flickrt.Get", [v.doc, a.text("0")]).into()
    }

    /// `S.op`: a function of its arguments up to its first effectful stage.
    pub(super) fn operation(&self, service: &str, name: &str, stages: &[Stage]) -> Code<'a> {
        let arity = stages
            .iter()
            .position(|&s| s != Stage::Pure)
            .map_or(stages.len(), |i| i + 1);
        let args = [
            self.string(service),
            self.string(name),
            arity.to_doc(self.a),
        ];
        call(self.a, "flickrt.Operation", args).into()
    }
}
