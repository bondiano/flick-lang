//! Pattern matching: each pattern becomes a test of the subject and the
//! bindings it makes. Arms are tried top to bottom; a guard that fails goes
//! on to the next arm.

use flick_pretty::{Document, Documentable, docvec};

use super::arith::Native;
use super::expr::enum_literal;
use super::layout::{block, call, conjunction, inline, statement};
use super::{Gen, names, own};
use flick_ir::{Arm, Expr, Lit, Pat};
use flick_ir::{Constructor, Rep};

#[derive(Default)]
struct Test<'a> {
    conds: Vec<Document<'a>>,
    /// `(raw_name, local_name, path, native_type)` for direct bindings.
    binds: Vec<(&'a str, &'a str, Document<'a>, Option<Native>)>,
    /// Locals an or-pattern assigns inside its test, declared before it:
    /// `(raw_name, local_name, agreed_native_type)`.
    lets: Vec<(&'a str, &'a str, Option<Native>)>,
}

fn merge_let<'a>(
    lets: &mut Vec<(&'a str, &'a str, Option<Native>)>,
    raw: &'a str,
    local: &'a str,
    native: Option<Native>,
) {
    if let Some((_, _, slot)) = lets.iter_mut().find(|(_, l, _)| *l == local) {
        if *slot != native {
            *slot = None;
        }
    } else {
        lets.push((raw, local, native));
    }
}

impl<'a> Gen<'a> {
    fn test(
        &mut self,
        pat: &Pat,
        subject: Document<'a>,
        subject_native: Option<Native>,
        t: &mut Test<'a>,
    ) {
        let a = self.a;
        match pat {
            Pat::Wild | Pat::Lit(Lit::Unit) => {}
            Pat::Bind(name) => t.binds.push((
                a.alloc_str(name),
                a.alloc_str(&names::local(name)),
                subject,
                subject_native,
            )),
            // a `BigInt` is equal by its number, any other literal as Go
            // compares a value with one: by its type, then by what it holds
            Pat::Lit(lit @ Lit::BigInt(_)) => {
                let lit = self.literal(lit).to_doc(a);
                t.conds.push(call(a, "flickrt.Same", [subject, lit]));
            }
            Pat::Lit(lit) => {
                let mut lit_doc = self.literal(lit).to_doc(a);
                if let Lit::Float(x) = lit
                    && !Native::is_native_float_lit(*x)
                    && subject_native == Some(Native::Float)
                {
                    lit_doc = Native::Float.unbox(a, lit_doc);
                }
                t.conds.push(docvec![a, "(", subject, " == ", lit_doc, ")"]);
            }
            Pat::Tuple(items) => {
                for (i, item) in items.iter().enumerate() {
                    self.test(item, call(a, "flickrt.At", [subject, i.to_doc(a)]), None, t);
                }
            }
            Pat::List(items, rest) => {
                let op = if rest.is_some() { " >= " } else { " == " };
                let len = call(a, "flickrt.Len", [subject]);
                t.conds.push(docvec![a, len, op, items.len()]);
                for (i, item) in items.iter().enumerate() {
                    self.test(item, call(a, "flickrt.At", [subject, i.to_doc(a)]), None, t);
                }
                if let Some(rest) = rest {
                    let after = items.len().to_doc(a);
                    self.test(rest, call(a, "flickrt.Rest", [subject, after]), None, t);
                }
            }
            Pat::Record(fields) => {
                for (label, p) in fields {
                    let field = call(a, "flickrt.Field", [subject, self.string(label)]);
                    self.test(p, field, None, t);
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
                    self.test(arg, subject, None, t);
                }
            }
            Pat::Con(c, args) => self.tagged(c, args, subject, t),
            Pat::Or(alternatives) => {
                let mut tests = Vec::new();
                for alt in alternatives {
                    let mut inner = Test::default();
                    self.test(alt, subject, subject_native, &mut inner);
                    let cond = if inner.conds.is_empty() {
                        a.text("true")
                    } else {
                        conjunction(a, inner.conds)
                    };
                    let assigns = inner
                        .binds
                        .iter()
                        .map(|(_, l, p, _)| docvec![a, *l, " = ", *p]);
                    let then = inline(a, assigns.chain([a.text("return true")]));
                    tests.push(docvec![a, "if ", cond, " ", then]);
                    for &(raw, local, _, native) in &inner.binds {
                        // bound by one alternative or another: what it is bound to is not known
                        self.held.remove(local);
                        merge_let(&mut t.lets, raw, local, native);
                    }
                    for &(raw, local, native) in &inner.lets {
                        merge_let(&mut t.lets, raw, local, native);
                    }
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

    /// The test of a tagged constructor and of its fields.
    fn tagged(&mut self, c: &Constructor, args: &[Pat], subject: Document<'a>, t: &mut Test<'a>) {
        let a = self.a;
        if let Some((ty, held)) = self.own_type(c) {
            // a Go type of its own: Go tells the constructor and has the fields
            t.conds
                .push(call(a, a.alloc_str(&own::test(&ty)), [subject]));
            if matches!(held, own::Held::Zero) {
                return;
            }
            let ty = a.alloc_str(&ty);
            let field = |i: usize| docvec![a, subject, ".(*", ty, ").f[", i, "]"];
            let places = match (&held, args) {
                (own::Held::Record(labels), [Pat::Record(fields)]) => {
                    let place = |(label, _): &(String, Pat)| labels.iter().position(|l| l == label);
                    fields.iter().map(place).collect::<Option<Vec<usize>>>()
                }
                _ => None,
            };
            match (held, args, places) {
                // the record lies in the value: a label is a place in it
                (_, [Pat::Record(fields)], Some(places)) => {
                    for ((_, p), place) in fields.iter().zip(places) {
                        self.test(p, field(place), None, t);
                    }
                }
                (own::Held::Record(labels), [arg], _) => {
                    if let Pat::Bind(name) = arg {
                        // read by place where the name is read by label
                        self.held.insert(names::local(name), labels);
                    }
                    let record = docvec![a, "flickrt.Value(&", subject, ".(*", ty, ").rec)"];
                    self.test(arg, record, None, t);
                }
                (own::Held::Fields(slots), args, _) if slots.iter().any(Option::is_some) => {
                    for (i, arg) in args.iter().enumerate() {
                        let path = docvec![a, subject, ".(*", ty, ").f", i];
                        self.test(arg, path, slots[i], t);
                    }
                }
                (_, args, _) => {
                    for (i, arg) in args.iter().enumerate() {
                        self.test(arg, field(i), None, t);
                    }
                }
            }
            return;
        }
        let tag = self.string(&self.tag(c));
        // of a type whose other constructors may have Go types of their own
        let is = if self.input.own_module(&c.module).is_some() {
            "flickrt.IsData"
        } else {
            "flickrt.IsTag"
        };
        t.conds.push(call(a, is, [subject, tag]));
        for (i, arg) in args.iter().enumerate() {
            self.test(arg, call(a, "flickrt.Get", [subject, i.to_doc(a)]), None, t);
        }
    }

    /// Declares the local `raw` as the value at `path`, typed natively when known.
    fn declare(&mut self, raw: &str, path: Document<'a>, native: Option<Native>) {
        match native {
            Some(n) => self.bind_native(raw, path.into(), n),
            None => self.bind(raw, path.into()),
        }
    }

    fn declare_lets(&mut self, lets: &[(&'a str, &'a str, Option<Native>)]) {
        for &(raw, local, native) in lets {
            let go_ty = native.map_or("flickrt.Value", Native::go);
            self.emit(docvec![self.a, "var ", local, " ", go_ty]);
            self.emit(docvec![self.a, "_ = ", local]);
            match native {
                Some(n) => {
                    self.natives.insert(raw.to_owned(), n);
                }
                None => {
                    self.natives.remove(raw);
                }
            }
        }
    }

    /// Binds the names of an irrefutable pattern.
    pub(super) fn destructure(&mut self, pat: &Pat, subject: Document<'a>) {
        let mut t = Test::default();
        self.test(pat, subject, None, &mut t);
        if !t.lets.is_empty() {
            self.declare_lets(&t.lets);
            self.emit(docvec![self.a, "_ = ", conjunction(self.a, t.conds)]);
        }
        for (raw, _, path, native) in t.binds {
            self.declare(raw, path, native);
        }
    }

    /// The arms of a `match` on `subject`: with `result`, the value goes
    /// there; otherwise each arm is in tail position and returns.
    pub(super) fn arms(
        &mut self,
        subject: Document<'a>,
        arms: &[Arm],
        result: Option<(&'a str, Option<Native>)>,
    ) {
        if self.decide(subject, arms, result) {
            return;
        }
        let a = self.a;
        let mut out = Vec::new();
        for arm in arms {
            let (stmts, ()) = self.block(|g| {
                g.arm(
                    subject,
                    &arm.pat,
                    arm.guard.as_ref(),
                    &arm.body,
                    |g, body| g.arm_body(body, result),
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
            // it never returns; a function of a Go type has no value to return of it
            self.emit(match self.returns {
                Some(_) => "panic(flickrt.NoMatch())",
                None => "return flickrt.NoMatch()",
            });
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
        let saved_natives = self.natives.clone();
        let mut t = Test::default();
        self.test(pat, subject, None, &mut t);
        let (lets, ()) = if t.lets.is_empty() {
            (Vec::new(), ())
        } else {
            self.block(|g| g.declare_lets(&t.lets))
        };
        let (inner, ()) = self.block(|g| {
            for &(raw, _, path, native) in &t.binds {
                g.declare(raw, path, native);
            }
            match guard {
                Some(guard) => {
                    let c = g.expr(guard);
                    let test = g.unboxed(guard, c, Native::Bool).doc;
                    let (b, ()) = g.block(|g| body(g, value));
                    g.emit(docvec![a, "if ", test, " ", block(a, b)]);
                }
                None => body(g, value),
            }
        });
        self.natives = saved_natives;
        let arm = if t.conds.is_empty() {
            block(a, inner)
        } else {
            docvec![a, "if ", conjunction(a, t.conds), " ", block(a, inner)]
        };
        if lets.is_empty() {
            self.emit(arm);
        } else {
            self.emit(block(a, lets.into_iter().chain([statement(a, arm)])));
        }
    }
}
