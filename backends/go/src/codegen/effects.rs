//! Effectful expressions: tasks, the scopes and regions they run in, typed
//! errors and the early return of `?`.

use std::mem;

use flick_pretty::{Document, Documentable, docvec};

use super::closures::Call;
use super::go_type::{GoType, Typed};
use super::layout::{Code, block, call, single, statement};
use super::{Gen, names};
use flick_ir::{Arm, Expr, Stage};

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
            self.returns.take(),
        );
        let (stmts, ()) = self.block(|g| g.tail_expr(body));
        (
            self.tail,
            self.generator,
            self.fiber,
            self.context,
            self.returns,
        ) = saved;
        stmts
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

    pub(super) fn read_owner_evidence(&self) -> Code<'a> {
        call(self.a, "flickrt.ReadOwnerEvidence", [self.context()]).into()
    }

    pub(super) fn with_owner_evidence(&mut self, owner: &Expr, inner: &Expr) -> Code<'a> {
        let a = self.a;
        let owner = self.expr(owner);
        let fb = self.fiber.expect("owner evidence brackets an execution");
        let region = call(a, "flickrt.WithOwnerEvidence", [a.text(fb), owner.doc]);
        let saved = (self.fiber.replace("fbe"), self.context.replace("fbe"));
        let (stmts, value) = self.block(|g| g.expr(inner));
        (self.fiber, self.context) = saved;
        let result = self.temp();
        self.emit(docvec![a, "var ", result, " flickrt.Value"]);
        let stmts = [statement(a, docvec![a, "fbe := ", region])]
            .into_iter()
            .chain(stmts)
            .chain([statement(a, docvec![a, result, " = ", value])]);
        self.emit(block(a, stmts));
        self.code(result)
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
            self.returns.take(),
        );
        let err = self.temp();
        let (mut stmts, ()) = self.block(|g| {
            for arm in arms {
                g.arm(
                    a.text(err),
                    None,
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
        (
            self.tail,
            self.generator,
            self.fiber,
            self.context,
            self.returns,
        ) = saved;
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
    pub(super) fn unwrap(&mut self, option: &Expr) -> Typed<'a> {
        let a = self.a;
        let o = self.expr_typed(option);
        let o = self.spill_typed(o);
        // in a lazy `for` a `None` ends the sequence, as a JS generator's return does
        let ret = if self.generator {
            a.text("return")
        } else {
            match &self.returns {
                Some(GoType::Opt(r)) => docvec![a, "return ", r.none_code(a)],
                None => a.text("return flickrt.None"),
                Some(GoType::Scalar(_) | GoType::Record(_)) => {
                    unreachable!("`?` only appears in Option-returning functions")
                }
            }
        };
        match o.ty {
            Some(GoType::Opt(repr)) => {
                let none = repr.is_none_doc(a, o.code.doc);
                self.emit(docvec![a, "if ", none, " ", single(a, ret)]);
                Typed {
                    code: repr.unwrap_doc(a, o.code.doc).into(),
                    ty: repr.payload_go_type(),
                }
            }
            None => {
                let none = call(a, "flickrt.IsNone", [o.code.doc]);
                self.emit(docvec![a, "if ", none, " ", single(a, ret)]);
                Typed {
                    code: call(a, "flickrt.UnwrapSome", [o.code.doc]).into(),
                    ty: None,
                }
            }
            Some(GoType::Scalar(_) | GoType::Record(_)) => unreachable!("`?` only unwraps Option"),
        }
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

    /// How `S.op` given all its arguments up to its first effectful stage
    /// is called, when that stage runs here: one `flickrt.Perform` of the
    /// operation's package variable, which looks the implementation up
    /// where it runs, as the task of `flickrt.Operation` would.
    pub(super) fn perform(&mut self, service: &str, name: &str) -> Call<'a> {
        let a = self.a;
        let made = format!(
            "flickrt.NewOp({}, {})",
            names::string(service),
            names::string(name)
        );
        let op = self.once(&made);
        Box::new(move |fb, args: &[Code<'a>]| {
            let args = [a.text(fb), a.text(op)]
                .into_iter()
                .chain(args.iter().map(|arg| arg.doc));
            call(a, "flickrt.Perform", args).into()
        })
    }
}
