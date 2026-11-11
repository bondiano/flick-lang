//! Pattern matching: each pattern becomes a test of the subject and the
//! bindings it makes. Arms are tried top to bottom; a guard that fails goes
//! on to the next arm.

use flick_pretty::{Document, Documentable, docvec};

use super::expr::enum_literal;
use super::layout::{block, call, conjunction, inline, statement};
use super::{Gen, names};
use crate::ir::{Arm, Expr, Lit, Pat};
use crate::types::Rep;

#[derive(Default)]
struct Test<'a> {
    conds: Vec<Document<'a>>,
    binds: Vec<(&'a str, Document<'a>)>,
    /// Locals an or-pattern assigns inside its test, declared before it.
    lets: Vec<&'a str>,
}

impl<'a> Gen<'a> {
    fn test(&mut self, pat: &Pat, subject: Document<'a>, t: &mut Test<'a>) {
        let a = self.a;
        match pat {
            Pat::Wild | Pat::Lit(Lit::Unit) => {}
            Pat::Bind(name) => t.binds.push((a.alloc_str(&names::local(name)), subject)),
            Pat::Lit(lit) => {
                let lit = self.literal(lit).to_doc(a);
                t.conds.push(call(a, "flickrt.Same", [subject, lit]));
            }
            Pat::Tuple(items) => {
                for (i, item) in items.iter().enumerate() {
                    self.test(item, call(a, "flickrt.At", [subject, i.to_doc(a)]), t);
                }
            }
            Pat::List(items, rest) => {
                let op = if rest.is_some() { " >= " } else { " == " };
                let len = call(a, "flickrt.Len", [subject]);
                t.conds.push(docvec![a, len, op, items.len()]);
                for (i, item) in items.iter().enumerate() {
                    self.test(item, call(a, "flickrt.At", [subject, i.to_doc(a)]), t);
                }
                if let Some(rest) = rest {
                    let after = items.len().to_doc(a);
                    self.test(rest, call(a, "flickrt.Rest", [subject, after]), t);
                }
            }
            Pat::Record(fields) => {
                for (label, p) in fields {
                    let field = call(a, "flickrt.Field", [subject, self.string(label)]);
                    self.test(p, field, t);
                }
            }
            Pat::Con(c, args) if c.rep != Rep::Tagged => {
                match &c.rep {
                    Rep::Typeof(class) => {
                        let class = self.string(class.name());
                        t.conds.push(call(a, "flickrt.IsClass", [subject, class]));
                    }
                    Rep::Literal(l) => {
                        let lit = a.text(a.alloc_str(&enum_literal(l)));
                        t.conds.push(call(a, "flickrt.Same", [subject, lit]));
                    }
                    Rep::Tagged | Rep::Unboxed => {}
                }
                for arg in args {
                    self.test(arg, subject, t);
                }
            }
            Pat::Con(c, args) => {
                let tag = self.string(&self.tag(c));
                t.conds.push(call(a, "flickrt.IsTag", [subject, tag]));
                for (i, arg) in args.iter().enumerate() {
                    self.test(arg, call(a, "flickrt.Get", [subject, i.to_doc(a)]), t);
                }
            }
            Pat::Or(alternatives) => {
                let mut tests = Vec::new();
                for alt in alternatives {
                    let mut inner = Test::default();
                    self.test(alt, subject, &mut inner);
                    let cond = if inner.conds.is_empty() {
                        a.text("true")
                    } else {
                        conjunction(a, inner.conds)
                    };
                    let assigns = inner.binds.iter().map(|(n, p)| docvec![a, *n, " = ", *p]);
                    let then = inline(a, assigns.chain([a.text("return true")]));
                    tests.push(docvec![a, "if ", cond, " ", then]);
                    for (name, _) in &inner.binds {
                        if !t.lets.contains(name) {
                            t.lets.push(name);
                        }
                    }
                    t.lets.extend(inner.lets);
                }
                let tests = inline(a, tests.into_iter().chain([a.text("return false")]));
                t.conds.push(docvec![
                    a,
                    "func(_ *flickrt.Fiber) bool ",
                    tests,
                    "(",
                    self.context(),
                    ")"
                ]);
            }
        }
    }

    /// Declares the local `name` as the value at `path`.
    fn declare(&mut self, name: &'a str, path: Document<'a>) {
        self.emit(docvec![self.a, "var ", name, " flickrt.Value = ", path]);
        self.emit(docvec![self.a, "_ = ", name]);
    }

    fn declare_lets(&mut self, lets: &[&'a str]) {
        for &l in lets {
            self.emit(docvec![self.a, "var ", l, " flickrt.Value"]);
            self.emit(docvec![self.a, "_ = ", l]);
        }
    }

    /// Binds the names of an irrefutable pattern.
    pub(super) fn destructure(&mut self, pat: &Pat, subject: Document<'a>) {
        let mut t = Test::default();
        self.test(pat, subject, &mut t);
        if !t.lets.is_empty() {
            self.declare_lets(&t.lets);
            self.emit(docvec![self.a, "_ = ", conjunction(self.a, t.conds)]);
        }
        for (name, path) in t.binds {
            self.declare(name, path);
        }
    }

    /// The arms of a `match` on `subject`: with `result`, the value goes
    /// there; otherwise each arm is in tail position and returns.
    pub(super) fn arms(&mut self, subject: Document<'a>, arms: &[Arm], result: Option<&'a str>) {
        let a = self.a;
        let mut out = Vec::new();
        for arm in arms {
            let (stmts, ()) = self.block(|g| {
                g.arm(
                    subject,
                    &arm.pat,
                    arm.guard.as_ref(),
                    &arm.body,
                    |g, body| match result {
                        Some(r) => {
                            let v = g.expr(body);
                            g.emit(docvec![a, r, " = ", v]);
                            g.emit("break");
                        }
                        None => g.tail_expr(body),
                    },
                );
            });
            out.extend(stmts);
        }
        if result.is_some() {
            // a loop run once, so that an arm can leave it with `break`
            out.push(statement(a, "flickrt.NoMatch()"));
            self.emit(docvec![a, "for ", block(a, out)]);
        } else {
            self.buf.extend(out);
            self.emit("return flickrt.NoMatch()");
        }
    }

    /// Runs `body` on `value` when `pat` matches `subject` and `guard`
    /// holds, with the names the pattern binds.
    pub(super) fn arm(
        &mut self,
        subject: Document<'a>,
        pat: &Pat,
        guard: Option<&Expr>,
        value: &Expr,
        body: impl FnOnce(&mut Self, &Expr),
    ) {
        let a = self.a;
        let mut t = Test::default();
        self.test(pat, subject, &mut t);
        let (inner, ()) = self.block(|g| {
            for &(name, path) in &t.binds {
                g.declare(name, path);
            }
            match guard {
                Some(guard) => {
                    let c = g.expr(guard);
                    let (b, ()) = g.block(|g| body(g, value));
                    let test = call(a, "flickrt.Bool", [c.doc]);
                    g.emit(docvec![a, "if ", test, " ", block(a, b)]);
                }
                None => body(g, value),
            }
        });
        let arm = if t.conds.is_empty() {
            block(a, inner)
        } else {
            docvec![a, "if ", conjunction(a, t.conds), " ", block(a, inner)]
        };
        if t.lets.is_empty() {
            self.emit(arm);
        } else {
            let (lets, ()) = self.block(|g| g.declare_lets(&t.lets));
            self.emit(block(a, lets.into_iter().chain([statement(a, arm)])));
        }
    }
}
