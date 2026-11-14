//! Expressions and statements. A JS expression is returned; statements it
//! needs go into the current statement list before it.

use std::mem;

use flick_pretty::docvec;

use super::doc::{After, Before, Form, Head, Js, Stmt, array, block_js, call, object};
use super::{Callee, Gen, Target, names};
use crate::ir::{Arm, Expr, ExprKind, Pat, Stmt as IrStmt};
use crate::numeric::Num;
use flick_ir::{Constructor, Rep};

impl<'d> Gen<'_, 'd> {
    fn stable(&self, js: Js<'d>) -> bool {
        js.atom()
            .is_some_and(|text| !self.vars.contains(text.split('.').next().unwrap_or(text)))
    }

    /// Compiles expressions that are evaluated in order: an operand whose
    /// code needs statements makes the operands before it constants first.
    pub(super) fn ordered(&mut self, exprs: &[&Expr]) -> Vec<Js<'d>> {
        let mut out: Vec<Js<'d>> = Vec::new();
        for e in exprs {
            let mark = self.buf.len();
            let js = self.expr(e);
            if self.buf.len() > mark {
                let mut spills = Vec::new();
                for prev in &mut out {
                    if !self.stable(*prev) {
                        let t = self.temp();
                        spills.push(self.constant(&t, *prev));
                        *prev = self.leaf(&t);
                    }
                }
                self.buf.splice(mark..mark, spills);
            }
            out.push(js);
        }
        out
    }

    /// `js` as a constant unless it is atomic.
    pub(super) fn spill(&mut self, js: Js<'d>) -> Js<'d> {
        if self.stable(js) {
            return js;
        }
        let t = self.temp();
        let c = self.constant(&t, js);
        self.buf.push(c);
        self.leaf(&t)
    }

    /// The text of `js` made atomic by [`spill`](Self::spill): a subject
    /// whose parts code names.
    pub(super) fn subject(&mut self, js: Js<'d>) -> String {
        let js = self.spill(js);
        js.atom().unwrap_or_default().to_owned()
    }

    /// `v;`.
    pub(super) fn expr_stmt(&self, v: Js<'d>) -> Stmt<'d> {
        let a = self.a;
        let doc = docvec![a, v.at(a, Before::Statement, After::Semicolon), ";"];
        Stmt::new(doc, v.flags)
    }

    /// `-inner`, kept in 32 bits on `Int`; a literal is in range already, as
    /// an `Int` literal is checked with its sign.
    fn neg(&mut self, num: Num, inner: &Expr) -> Js<'d> {
        let js = self.expr(inner);
        let a = self.a;
        let doc = if num == Num::Int && !matches!(inner.kind, ExprKind::Lit(_)) {
            docvec![a, "-", js.at(a, Before::Other, After::Other), " | 0"]
        } else {
            docvec![a, "-", js.at(a, Before::Other, After::Delimiter)]
        };
        Js::code(doc, Head::Other, &[js]).in_parens()
    }

    fn not(&mut self, inner: &Expr) -> Js<'d> {
        let v = self.expr(inner);
        let doc = docvec![self.a, "!", v.at(self.a, Before::Other, After::Delimiter)];
        Js::code(doc, Head::Other, &[v]).in_parens()
    }

    pub(super) fn expr(&mut self, e: &Expr) -> Js<'d> {
        match &e.kind {
            ExprKind::ControlLoop { .. } | ExprKind::LoopExit { .. } | ExprKind::Loop { .. } => {
                self.loop_expr(e)
            }
            ExprKind::Lit(lit) => self.literal(lit),
            ExprKind::Local(name) | ExprKind::Var(name) => self.leaf(&names::ident(name)),
            ExprKind::Global { module, name } => self.global(module, &names::ident(name)),
            ExprKind::Method { dict, name } => {
                if let Some(direct) = self.direct_method(dict, name) {
                    direct
                } else {
                    let dict = self.dict(dict);
                    self.member(dict, name)
                }
            }
            ExprKind::Con(c) => self.constructor(c),
            ExprKind::Perform {
                effect: service,
                op: name,
                stages,
            } => self.operation(service, name, stages, &[]),
            ExprKind::ServiceTag(key) => self.service_ref(key),
            ExprKind::Label(label) => self.string(label),
            ExprKind::Dicts(inner, dicts) => {
                let dicts: Vec<Js<'d>> = dicts.iter().map(|d| self.dict(d)).collect();
                if let ExprKind::Perform { effect, op, stages } = &inner.kind {
                    self.operation(effect, op, stages, &dicts)
                } else {
                    let f = self.expr(inner);
                    call(self.a, f, &dicts)
                }
            }
            ExprKind::Abstract(shown, inner) => self.abstraction(shown, inner),
            ExprKind::App {
                func,
                arg,
                stage,
                arg_first,
            } => self.app(e, func, arg, *stage, *arg_first),
            ExprKind::Lambda { params, body } => self.lambda(params, body, None, false),
            ExprKind::Block(stmts, result) => {
                for stmt in stmts {
                    self.stmt(stmt);
                }
                self.expr(result)
            }
            ExprKind::If(c, t, f) => self.if_expr(c, t, f),
            ExprKind::Match(scrutinee, arms) => {
                let s = self.expr(scrutinee);
                let s = self.subject(s);
                let r = self.temp();
                self.buf.push(self.declare(&r));
                self.arms(&s, arms, Some(&r));
                self.leaf(&r)
            }
            ExprKind::Handle(h) if h.is_try() => self.try_expr(&h.body, &h.errors),
            ExprKind::Tuple(items) | ExprKind::List(items) => {
                let items: Vec<&Expr> = items.iter().collect();
                let items = self.ordered(&items);
                array(self.a, &items)
            }
            ExprKind::Record(fields) | ExprKind::Dictionary(fields) => self.record(None, fields),
            ExprKind::Update(record, fields) => self.record(Some(record), fields),
            ExprKind::Field(record, label) => {
                let r = self.expr(record);
                self.field(r, label)
            }
            ExprKind::Binary(op, lhs, rhs) => self.binary(e.span, *op, lhs, rhs),
            ExprKind::Not(inner) => self.not(inner),
            ExprKind::Unwrap(inner) => self.unwrap(inner),
            ExprKind::Neg(num, inner) => self.neg(*num, inner),
            ExprKind::Ignore(inner) => {
                let v = self.expr(inner);
                if v.atom().is_none() {
                    self.buf.push(self.expr_stmt(v));
                }
                self.leaf("undefined")
            }
            ExprKind::Await(inner) => {
                let owner = self.owner.take();
                let v = self.expr(inner);
                if self.kernel {
                    self.await_task(owner, v)
                } else {
                    self.run(owner, v)
                }
            }
            ExprKind::Task(body) => self.gen_block(body),
            ExprKind::Generate { .. } | ExprKind::Each { .. } | ExprKind::Yield(_) => {
                self.generated(e)
            }
            ExprKind::Within(owner, call) => {
                self.owner = Some(names::ident(owner));
                self.expr(call)
            }
            ExprKind::Handle(h) => self.handle(h),
            ExprKind::Lift(inner, lift) => self.lifted(inner, lift),
        }
    }

    /// The field `label` of the record `r`.
    fn field(&mut self, r: Js<'d>, label: &str) -> Js<'d> {
        let a = self.a;
        if let Some(text) = r.atom() {
            if names::is_property(label) {
                return self.leaf(&format!("{text}.{label}"));
            }
            let doc = docvec![a, r.doc, "[", self.string(label).doc, "]"];
            return Js {
                form: Form::Member,
                ..r.starting(doc, &[r])
            };
        }
        let doc = docvec![a, "(", r.at(a, Before::Delimiter, After::Delimiter), ")"];
        let record = Js::code(doc, r.after_paren(), &[r]);
        let access = if names::is_property(label) {
            docvec![a, ".", a.alloc_str(label)]
        } else {
            docvec![a, "[", self.string(label).doc, "]"]
        };
        Js {
            form: Form::Member,
            ..Js::code(docvec![a, record.doc, access], record.head, &[record])
        }
    }

    /// A value that takes the dictionaries `shown` before it is computed.
    fn abstraction(&mut self, shown: &[String], inner: &Expr) -> Js<'d> {
        let mark = self.givens.len();
        let params = self.bind_givens(shown);
        let saved = mem::replace(&mut self.generator, false);
        let (stmts, value) = self.block(|g| g.expr(inner));
        self.generator = saved;
        self.givens.truncate(mark);
        let body = self.arrow_body(&stmts, value);
        self.lambda_of(&params, body)
    }

    fn if_expr(&mut self, c: &Expr, t: &Expr, f: &Expr) -> Js<'d> {
        let arena = self.a;
        let c = self.expr(c);
        let (mut ts, tv) = self.block(|g| g.expr(t));
        let (mut fs, fv) = self.block(|g| g.expr(f));
        if ts.is_empty() && fs.is_empty() {
            let doc = docvec![
                arena,
                c.at(arena, Before::Delimiter, After::Other),
                " ? ",
                tv.at(arena, Before::Other, After::Other),
                " : ",
                fv.at(arena, Before::Delimiter, After::Delimiter)
            ];
            return c.starting(doc, &[c, tv, fv]).in_parens();
        }
        let r = self.temp();
        let declare = self.declare(&r);
        ts.push(self.assign(&r, tv));
        fs.push(self.assign(&r, fv));
        let then = block_js(arena, &ts);
        let otherwise = block_js(arena, &fs);
        let doc = docvec![
            arena,
            declare.doc,
            arena.line(),
            "if (",
            c.at(arena, Before::Delimiter, After::Delimiter),
            ") ",
            then.doc,
            " else ",
            otherwise.doc
        ];
        let flags = declare.flags | c.flags | then.flags | otherwise.flags;
        self.buf.push(Stmt::new(doc, flags));
        self.leaf(&r)
    }

    /// `name = value;`.
    pub(super) fn assign(&self, name: &str, value: Js<'d>) -> Stmt<'d> {
        let a = self.a;
        let doc = docvec![
            a,
            a.alloc_str(name),
            " = ",
            value.at(a, Before::Assign, After::Semicolon),
            ";"
        ];
        Stmt::new(doc, value.flags)
    }

    fn try_expr(&mut self, body: &Expr, arms: &[Arm]) -> Js<'d> {
        if !self.generator && !self.sync {
            return self.expr(body);
        }
        if self.kernel && !self.generator {
            return self.try_at_once(body, arms);
        }
        let body = self.gen_block(body);
        let error = self.temp();
        let (stmts, ()) = self.block(|g| g.handlers(&error, arms));
        let try_with = self.rt("tryWith");
        let handler = self.lambda_of(&error, block_js(self.a, &stmts));
        let tried = call(self.a, try_with, &[body, handler]);
        self.run(None, tried)
    }

    /// `o?`: the value of the `Some`, after returning a `None` at once.
    fn unwrap(&mut self, option: &Expr) -> Js<'d> {
        let v = self.expr(option);
        let v = self.subject(v);
        let stmt = self.line(&format!("if ({v}._tag === \"None\") return {v};"));
        self.buf.push(stmt);
        self.leaf(&format!("{v}.value"))
    }

    /// A record literal, or `{ base with … }`.
    fn record(&mut self, base: Option<&Expr>, fields: &[(String, Expr)]) -> Js<'d> {
        let a = self.a;
        let mut all: Vec<&Expr> = base.into_iter().collect();
        all.extend(fields.iter().map(|(_, v)| v));
        let js = self.ordered(&all);
        let values = &js[usize::from(base.is_some())..];
        let mut entries = Vec::with_capacity(js.len());
        if base.is_some() {
            entries.push(docvec![
                a,
                "...",
                js[0].at(a, Before::Other, After::Delimiter)
            ]);
        }
        entries.extend(fields.iter().zip(values).map(|((label, _), v)| {
            docvec![
                a,
                key(label),
                ": ",
                v.at(a, Before::Delimiter, After::Delimiter)
            ]
        }));
        object(a, &entries, &js)
    }

    pub(super) fn stmt(&mut self, stmt: &IrStmt) {
        #[cfg(feature = "source-maps")]
        let span = match stmt {
            IrStmt::Let(_, e)
            | IrStmt::Var(_, e)
            | IrStmt::Assign(_, e)
            | IrStmt::Expr(e)
            | IrStmt::Fun { value: e, .. } => e.span,
        };
        #[cfg(feature = "source-maps")]
        let first = self.buf.len();
        self.stmt_kind(stmt);
        #[cfg(feature = "source-maps")]
        if first < self.buf.len() {
            let mark = self.mark(span);
            let first = &mut self.buf[first];
            // a cursor before a `return` keeps it from being taken apart
            *first = Stmt {
                doc: docvec![self.a, mark, first.doc],
                ret: None,
                ..*first
            };
        }
    }

    fn stmt_kind(&mut self, stmt: &IrStmt) {
        match stmt {
            IrStmt::Let(Pat::Bind(name), value) => {
                let v = self.expr(value);
                self.buf.push(self.constant(&names::ident(name), v));
            }
            IrStmt::Let(pat, value) => {
                let v = self.expr(value);
                let v = self.subject(v);
                self.destructure(pat, &v);
            }
            IrStmt::Var(name, value) => {
                let v = self.expr(value);
                let name = names::ident(name);
                self.buf.push(self.variable(&name, v));
                self.vars.insert(name);
            }
            IrStmt::Assign(name, value) => {
                let v = self.expr(value);
                self.buf.push(self.assign(&names::ident(name), v));
            }
            IrStmt::Fun {
                name,
                rec,
                dicts,
                value,
            } => {
                let target = rec.then(|| Target {
                    callee: Callee::Local(name.clone()),
                    dicts: dicts.clone(),
                });
                let f = self.function(dicts, value, target);
                self.buf.push(self.constant(&names::ident(name), f));
            }
            IrStmt::Expr(e) => {
                let v = self.expr(e);
                if v.atom().is_none() {
                    self.buf.push(self.expr_stmt(v));
                }
            }
        }
    }

    pub(super) fn constructor(&mut self, c: &Constructor) -> Js<'d> {
        if let Rep::Literal(literal) = &c.rep
            && !self.options.typescript()
        {
            return self.leaf(&names::enum_literal(literal));
        }
        match names::native(self.options.lang, c) {
            Some(names::Native::Some) => self.rt("some"),
            Some(names::Native::None) => self.rt("none"),
            Some(names::Native::Ok) => self.rt("ok"),
            Some(names::Native::Err) => self.rt("err"),
            None => self.global(&c.module, &names::ident(&c.name)),
        }
    }

    /// The body of an arrow returning `value` after `stmts`.
    pub(super) fn arrow_body(&self, stmts: &[Stmt<'d>], value: Js<'d>) -> Js<'d> {
        let a = self.a;
        if !stmts.is_empty() {
            let mut all = stmts.to_vec();
            all.push(Stmt::ret(a, None, value));
            return block_js(a, &all);
        }
        if value.lead() == Head::Brace {
            // an object, not a block
            let doc = docvec![
                a,
                "(",
                value.at(a, Before::Delimiter, After::Delimiter),
                ")"
            ];
            Js::code(doc, value.after_paren(), &[value])
        } else {
            value
        }
    }
}

/// A record key in an object literal.
fn key(label: &str) -> String {
    if names::is_property(label) && label != "__proto__" {
        label.to_owned()
    } else {
        format!("[{}]", names::string(label))
    }
}
