//! Pattern matching: each pattern becomes a test of the subject and the
//! bindings it makes. Arms are tried top to bottom; a guard that fails goes
//! on to the next arm.

use flick_pretty::{Document, Documentable, docvec};

use super::arith::Native;
use super::expr::enum_literal;
use super::go_type::{GoType, OptCon, Typed};
use super::layout::{block, call, conjunction, inline, statement};
use super::{Gen, names, own};
use flick_ir::{Arm, Constructor, Expr, Lit, Pat, Rep};

#[derive(Default)]
struct Test<'a> {
    conds: Vec<Document<'a>>,
    /// `(raw_name, local_name, path, go_type)` for direct bindings.
    binds: Vec<(&'a str, &'a str, Document<'a>, Option<GoType>)>,
    /// Locals an or-pattern assigns inside its test, declared before it:
    /// `(raw_name, local_name, agreed_go_type)`.
    lets: Vec<(&'a str, &'a str, Option<GoType>)>,
    /// When inside an enclosing `Pat::Or`, the final merged `GoType` of every
    /// binder declared for that `Pat::Or` so nested `Pat::Or`s coerce directly
    /// into the outer variable's declared Go type.
    or_targets: Option<Vec<(&'a str, Option<GoType>)>>,
}

fn merge_let<'a>(
    lets: &mut Vec<(&'a str, &'a str, Option<GoType>)>,
    raw: &'a str,
    local: &'a str,
    go_ty: Option<GoType>,
) {
    if let Some((_, _, slot)) = lets.iter_mut().find(|(_, l, _)| *l == local) {
        if *slot != go_ty {
            *slot = None;
        }
    } else {
        lets.push((raw, local, go_ty));
    }
}

impl<'a> Gen<'a> {
    /// Formats a literal for equality against `subject_native`, unboxing special
    /// runtime float constants (`NaN`, `Inf`, `NegInf`, `NegZero`) when needed.
    pub(super) fn pattern_lit(
        &mut self,
        lit: &Lit,
        subject_native: Option<Native>,
    ) -> Document<'a> {
        let a = self.a;
        let lit_doc = self.literal(lit).to_doc(a);
        match (lit, subject_native) {
            (Lit::Float(x), Some(Native::Float)) if !Native::is_native_float_lit(*x) => {
                Native::Float.unbox(a, lit_doc)
            }
            (Lit::Float32(x), Some(Native::Float32)) if !Native::is_native_float32_lit(*x) => {
                Native::Float32.unbox(a, lit_doc)
            }
            _ => lit_doc,
        }
    }

    fn test(
        &mut self,
        pat: &Pat,
        subject: Document<'a>,
        subject_ty: Option<GoType>,
        t: &mut Test<'a>,
    ) {
        let a = self.a;
        match pat {
            Pat::Wild | Pat::Lit(Lit::Unit) => {}
            Pat::Bind(name) => t.binds.push((
                a.alloc_str(name),
                a.alloc_str(&names::local(name)),
                subject,
                subject_ty,
            )),
            // a `BigInt` is equal by its number, any other literal as Go
            // compares a value with one: by its type, then by what it holds
            Pat::Lit(lit @ Lit::BigInt(_)) => {
                let lit = self.literal(lit).to_doc(a);
                t.conds.push(call(a, "flickrt.Same", [subject, lit]));
            }
            Pat::Lit(lit) => {
                let lit_doc =
                    self.pattern_lit(lit, subject_ty.as_ref().and_then(GoType::as_scalar));
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
            Pat::Con(c, args) => self.tagged(c, args, subject, subject_ty, t),
            Pat::Or(alternatives) => self.test_or(alternatives, subject, subject_ty.as_ref(), t),
        }
    }

    /// Pre-pass over an outermost `Pat::Or` (recursing into nested `Pat::Or`s
    /// and constructor subpatterns) to compute the final agreed `GoType` for
    /// every binder declared before the or-pattern runs.
    fn collect_or_lets(
        &self,
        pat: &Pat,
        subject_ty: Option<&GoType>,
        out: &mut Vec<(&'a str, &'a str, Option<GoType>)>,
    ) {
        let a = self.a;
        match pat {
            Pat::Wild | Pat::Lit(_) => {}
            Pat::Bind(name) => merge_let(
                out,
                a.alloc_str(name),
                a.alloc_str(&names::local(name)),
                subject_ty.cloned(),
            ),
            Pat::Tuple(items) => {
                for item in items {
                    self.collect_or_lets(item, None, out);
                }
            }
            Pat::List(items, rest) => {
                for item in items {
                    self.collect_or_lets(item, None, out);
                }
                if let Some(rest) = rest {
                    self.collect_or_lets(rest, None, out);
                }
            }
            Pat::Record(fields) => {
                for (_, p) in fields {
                    self.collect_or_lets(p, None, out);
                }
            }
            Pat::Con(c, args) if c.rep != Rep::Tagged => {
                for arg in args {
                    self.collect_or_lets(arg, None, out);
                }
            }
            Pat::Con(c, args) => {
                if let Some(GoType::Opt(repr)) = subject_ty {
                    if self.opt_con(c) == Some(OptCon::Some)
                        && let Some(arg) = args.first()
                    {
                        let inner = repr.payload_go_type();
                        self.collect_or_lets(arg, inner.as_ref(), out);
                    }
                    return;
                }
                if self.opt_con(c).is_some() {
                    if let Some(arg) = args.first() {
                        self.collect_or_lets(arg, None, out);
                    }
                    return;
                }
                let held = self.own_held(c).filter(own::Held::has_scalars);
                for (i, arg) in args.iter().enumerate() {
                    let slot = held
                        .as_ref()
                        .and_then(|h| h.field_native(i))
                        .map(GoType::Scalar);
                    self.collect_or_lets(arg, slot.as_ref(), out);
                }
            }
            Pat::Or(alts) => {
                for alt in alts {
                    self.collect_or_lets(alt, subject_ty, out);
                }
            }
        }
    }

    fn test_or(
        &mut self,
        alternatives: &[Pat],
        subject: Document<'a>,
        subject_ty: Option<&GoType>,
        t: &mut Test<'a>,
    ) {
        let a = self.a;
        // At the outermost `Pat::Or`, resolve the final agreed GoType across
        // every nested alternative before emitting any inner assignment so
        // inner `Pat::Or` closures coerce directly into the declared variable.
        let targets: Vec<(&'a str, Option<GoType>)> = if let Some(outer) = &t.or_targets {
            outer.clone()
        } else {
            let mut merged = Vec::new();
            for alt in alternatives {
                self.collect_or_lets(alt, subject_ty, &mut merged);
            }
            for (raw, local, go_ty) in &merged {
                merge_let(&mut t.lets, raw, local, go_ty.clone());
            }
            merged
                .into_iter()
                .map(|(_, local, go_ty)| (local, go_ty))
                .collect()
        };
        let mut tests = Vec::with_capacity(alternatives.len());
        for alt in alternatives {
            let mut inner = Test {
                or_targets: Some(targets.clone()),
                ..Test::default()
            };
            self.test(alt, subject, subject_ty.cloned(), &mut inner);
            let cond = if inner.conds.is_empty() {
                a.text("true")
            } else {
                conjunction(a, inner.conds)
            };
            for (_, l, _, _) in &inner.binds {
                // bound by one alternative or another: what it is bound to is not known
                self.held.remove(*l);
            }
            let assigns = inner.binds.into_iter().map(|(_, l, p, path_ty)| {
                let target_ty = targets
                    .iter()
                    .find(|(local, _)| *local == l)
                    .and_then(|(_, ty)| ty.as_ref());
                let value = self.coerce(
                    Typed {
                        code: p.into(),
                        ty: path_ty,
                    },
                    target_ty,
                );
                docvec![a, l, " = ", value]
            });
            let then = inline(a, assigns.chain([a.text("return true")]));
            tests.push(docvec![a, "if ", cond, " ", then]);
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

    /// The test of a tagged constructor and of its fields.
    fn tagged(
        &mut self,
        c: &Constructor,
        args: &[Pat],
        subject: Document<'a>,
        subject_ty: Option<GoType>,
        t: &mut Test<'a>,
    ) {
        let a = self.a;
        if let Some(GoType::Opt(repr)) = subject_ty {
            match self.opt_con(c) {
                Some(OptCon::None) => {
                    t.conds.push(repr.is_none_doc(a, subject));
                }
                Some(OptCon::Some) => {
                    t.conds.push(repr.is_some_doc(a, subject));
                    if let Some(arg) = args.first() {
                        self.test(arg, repr.unwrap_doc(a, subject), repr.payload_go_type(), t);
                    }
                }
                None => unreachable!("Option value matched against non-Option constructor"),
            }
            return;
        }
        if let Some(opt_con) = self.opt_con(c) {
            match opt_con {
                OptCon::None => t.conds.push(call(a, "flickrt.IsNone", [subject])),
                OptCon::Some => {
                    t.conds.push(call(a, "flickrt.IsSome", [subject]));
                    if let Some(arg) = args.first() {
                        self.test(arg, call(a, "flickrt.UnwrapSome", [subject]), None, t);
                    }
                }
            }
            return;
        }
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
                        self.test(arg, path, slots[i].map(GoType::Scalar), t);
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

    fn declare_lets(&mut self, lets: &[(&'a str, &'a str, Option<GoType>)]) {
        for (raw, local, go_ty) in lets {
            let ty_name = self.go_type_name(go_ty.as_ref());
            self.emit(docvec![self.a, "var ", *local, " ", ty_name]);
            self.emit(docvec![self.a, "_ = ", *local]);
            match go_ty {
                Some(gt) => {
                    self.go_types.insert((*raw).to_owned(), gt.clone());
                }
                None => {
                    self.go_types.remove(*raw);
                }
            }
        }
    }

    /// Binds the names of an irrefutable pattern.
    pub(super) fn destructure(
        &mut self,
        pat: &Pat,
        subject: Document<'a>,
        subject_ty: Option<GoType>,
    ) {
        let mut t = Test::default();
        self.test(pat, subject, subject_ty, &mut t);
        if !t.lets.is_empty() {
            self.declare_lets(&t.lets);
            self.emit(docvec![self.a, "_ = ", conjunction(self.a, t.conds)]);
        }
        for (raw, _, path, go_ty) in t.binds {
            self.bind_typed(raw, path.into(), go_ty);
        }
    }

    /// The arms of a `match` on `subject`: with `result`, the value goes
    /// there; otherwise each arm is in tail position and returns.
    pub(super) fn arms(
        &mut self,
        subject: Document<'a>,
        subject_ty: Option<&GoType>,
        arms: &[Arm],
        result: Option<&(&'a str, Option<GoType>)>,
    ) {
        if self.decide(subject, subject_ty, arms, result) {
            return;
        }
        let a = self.a;
        let mut out = Vec::new();
        for arm in arms {
            let (stmts, ()) = self.block(|g| {
                g.arm(
                    subject,
                    subject_ty.cloned(),
                    &arm.pat,
                    arm.guard.as_ref(),
                    &arm.body,
                    |g, body| g.arm_body(body, result.cloned()),
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
            self.emit(match &self.returns {
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
        subject_ty: Option<GoType>,
        pat: &Pat,
        guard: Option<&Expr>,
        value: &Expr,
        body: impl FnOnce(&mut Self, &Expr),
    ) {
        let a = self.a;
        let saved_types = self.go_types.clone();
        let mut t = Test::default();
        self.test(pat, subject, subject_ty, &mut t);
        let (lets, ()) = if t.lets.is_empty() {
            (Vec::new(), ())
        } else {
            self.block(|g| g.declare_lets(&t.lets))
        };
        let (inner, ()) = self.block(|g| {
            for (raw, _, path, go_ty) in &t.binds {
                g.bind_typed(raw, (*path).into(), go_ty.clone());
            }
            match guard {
                Some(guard) => {
                    let test = g.expr_bool(guard).doc;
                    let (b, ()) = g.block(|g| body(g, value));
                    g.emit(docvec![a, "if ", test, " ", block(a, b)]);
                }
                None => body(g, value),
            }
        });
        self.go_types = saved_types;
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
