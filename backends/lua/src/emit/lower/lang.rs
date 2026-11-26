//! The chunk `_flick/kernel/lang.lua`, which loads after the kernel's own
//! chunks: the built-in constructors the kernel builds and reads, in the
//! representation this program gives them. The kernel takes `Some`, `None`,
//! `Ok` and `Err` from their lang roles; the prelude's `Ordering` and the test
//! runner's `Test.Assertion` have none, so they are found by name, as the JS
//! and Go runtimes find them.

use flick_ir::{Constructor, Lang, Rep, Span};

use super::super::ast::{BinOp, Block, Expr, ExprKind, Function, LastKind, Name, Stmt, StmtKind};
use super::super::names::{KERNEL, Namer};
use super::Lowering;

impl<'p> Lowering<'p> {
    /// The constructor `name` of the type `ty` of `module`.
    fn named(&self, module: &str, ty: &str, name: &str) -> Option<&'p Constructor> {
        (self.constructors.get(&(module, name)).copied()).filter(|c| c.ty == ty)
    }

    /// The statements of `lang.lua`, after its prologue.
    pub(crate) fn lang(&mut self, span: Span) -> Vec<Stmt> {
        self.module = "";
        self.names = Namer::new();
        let mut stmts = Vec::new();
        let mut set = |field: &str, value: Expr| {
            let slot = Expr::name(&Name::new(KERNEL.to_owned()), span).field(field);
            stmts.push(Stmt::new(StmtKind::Assign(slot, value), span));
        };
        let langs = [
            (Lang::Some, "some"),
            (Lang::None, "none"),
            (Lang::Ok, "ok"),
            (Lang::Err, "err"),
        ];
        for (lang, field) in langs {
            let found = (self.known.get(lang)).and_then(|key| self.constructors.get(&key).copied());
            if let Some(c) = found {
                set(field, self.constructor(c, span));
            }
        }
        for name in ["LT", "EQ", "GT"] {
            if let Some(c) = self.named("Prelude", "Ordering", name) {
                set(name, self.constructor(c, span));
            }
        }
        if let Some(read) = self.assertion_message(span) {
            set("assertion_message", read);
        }
        stmts
    }

    /// A function of a `Test.Assertion`: the message of `Failed`, `nil`
    /// for `Pass`.
    fn assertion_message(&mut self, span: Span) -> Option<Expr> {
        let pass = self.named("Test", "Assertion", "Pass")?;
        let failed = self.named("Test", "Assertion", "Failed")?;
        let a = self.names.fresh("a");
        let value = Expr::name(&a, span);
        let (test, message) = match (&pass.rep, &failed.rep) {
            (Rep::Tagged, Rep::Tagged) => (
                Expr::binary(
                    BinOp::Eq,
                    value.clone().field("_tag"),
                    Expr::str(&self.tag_of(pass), span),
                    span,
                ),
                value.index(Expr::number(1, span)),
            ),
            (Rep::Literal(literal), Rep::Unboxed | Rep::Typeof(_)) => (
                Expr::binary(
                    BinOp::Eq,
                    value.clone(),
                    Self::enum_literal(literal, span),
                    span,
                ),
                value,
            ),
            _ => {
                let message = format!(
                    "the test runner of the {} target does not read this representation of \
                     `Test.Assertion`",
                    self.target
                );
                self.refuse(span, message);
                return None;
            }
        };
        let mut pass_block = Block::default();
        pass_block.end(LastKind::Return(Expr::new(ExprKind::Nil, span)), span);
        let mut body = Block::default();
        body.push(Stmt::new(
            StmtKind::If(vec![(test, pass_block)], None),
            span,
        ));
        body.end(LastKind::Return(message), span);
        let function = Function {
            params: vec![a],
            body,
            span,
        };
        Some(Expr::new(ExprKind::Function(Box::new(function)), span))
    }
}
