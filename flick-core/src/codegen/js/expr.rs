//! Expressions and statements. A JS expression is returned; statements it
//! needs go into the current statement list before it.

use std::mem;

use super::{Gen, Target, indent, names};
use crate::ir::{Arm, BinOp, Expr, ExprKind, Lit, Pat, Stmt};
use crate::source::Span;
use crate::types::{Constructor, Dict, Lift, Rep};

/// Whether moving the evaluation of `js` later is unobservable: a name, a
/// property of a module namespace, or a literal.
pub fn atomic(js: &str) -> bool {
    let name = |s: &str| {
        s.chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$' || c == '.')
    };
    name(js)
        || js.parse::<f64>().is_ok()
        || (js.starts_with('"') && serde_json::from_str::<String>(js).is_ok())
}

impl Gen<'_> {
    /// Compiles expressions that are evaluated in order: an operand whose
    /// code needs statements makes the operands before it constants first.
    pub(super) fn ordered(&mut self, exprs: &[&Expr]) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for e in exprs {
            let mark = self.buf.len();
            let js = self.expr(e);
            if self.buf.len() > mark {
                let mut spills = Vec::new();
                for prev in &mut out {
                    if !atomic(prev) {
                        let t = self.temp();
                        spills.push(self.constant(&t, prev));
                        *prev = t;
                    }
                }
                self.buf.splice(mark..mark, spills);
            }
            out.push(js);
        }
        out
    }

    /// `js` as a constant unless it is atomic.
    pub(super) fn spill(&mut self, js: String) -> String {
        if atomic(&js) {
            return js;
        }
        let t = self.temp();
        let c = self.constant(&t, &js);
        self.buf.push(c);
        t
    }

    pub(super) fn expr(&mut self, e: &Expr) -> String {
        match &e.kind {
            ExprKind::Lit(lit) => literal(lit),
            ExprKind::Local(name) => names::ident(name),
            ExprKind::Global { module, name } => self.global(module, &names::ident(name)),
            ExprKind::Method { dict, name } => match self.direct_method(dict, name) {
                Some(direct) => direct,
                None => format!("{}.{name}", self.dict(dict)),
            },
            ExprKind::Con(c) => self.constructor(c),
            ExprKind::Operation {
                service,
                name,
                stages,
            } => {
                let arity = stages
                    .iter()
                    .position(|&r| r)
                    .map_or(stages.len(), |i| i + 1);
                let tag = self.service_ref(service);
                let op = self.rt("operation");
                format!("{op}({tag}, {}, {arity})", names::string(name))
            }
            ExprKind::ServiceTag(key) => self.service_ref(key),
            ExprKind::Label(label) => names::string(label),
            ExprKind::Dicts(inner, dicts) => {
                let f = self.expr(inner);
                let dicts: Vec<String> = dicts.iter().map(|d| self.dict(d)).collect();
                format!("{f}({})", dicts.join(", "))
            }
            ExprKind::Abstract(shown, inner) => self.abstraction(shown, inner),
            ExprKind::App {
                func,
                arg,
                runtime,
                arg_first,
            } => self.app(e, func, arg, *runtime, *arg_first),
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
                let s = self.spill(s);
                let r = self.temp();
                self.buf.push(self.declare(&r));
                self.arms(&s, arms, Some(&r));
                r
            }
            ExprKind::Try(body, arms) => self.try_expr(body, arms),
            ExprKind::Tuple(items) | ExprKind::List(items) => {
                let items: Vec<&Expr> = items.iter().collect();
                format!("[{}]", self.ordered(&items).join(", "))
            }
            ExprKind::Record(fields) => self.record(None, fields),
            ExprKind::Update(record, fields) => self.record(Some(record), fields),
            ExprKind::Field(record, label) => {
                let r = self.expr(record);
                let r = if atomic(&r) { r } else { format!("({r})") };
                if names::is_property(label) {
                    format!("{r}.{label}")
                } else {
                    format!("{r}[{}]", names::string(label))
                }
            }
            ExprKind::Binary(op, lhs, rhs) => self.binary(e.span, *op, lhs, rhs),
            ExprKind::Not(inner) => format!("(!{})", self.expr(inner)),
            ExprKind::Neg(inner) => format!("(-{})", self.expr(inner)),
            ExprKind::Ignore(inner) => {
                let v = self.expr(inner);
                if !atomic(&v) {
                    self.buf.push(format!("{v};"));
                }
                "undefined".to_owned()
            }
            ExprKind::Await(inner) => {
                let owner = self.owner.take();
                let v = self.expr(inner);
                self.run(owner, &v)
            }
            ExprKind::Task(body) => self.gen_block(body),
            ExprKind::Scope { owner, outer, body } => {
                self.scope(owner.as_deref(), outer.as_deref(), body)
            }
            ExprKind::Within(owner, call) => {
                self.owner = Some(names::ident(owner));
                self.expr(call)
            }
            ExprKind::Provide(layer, body) => self.provide(layer, body),
            ExprKind::CodecBlock(name, fields) => self.codec_block(name, fields),
            ExprKind::Lift(inner, lift) => {
                let v = self.expr(inner);
                let f = self.lift(lift);
                format!("({f})({v})")
            }
        }
    }

    fn app(&mut self, e: &Expr, func: &Expr, arg: &Expr, runtime: bool, arg_first: bool) -> String {
        if let Some(value) = self.constructed(e) {
            return value;
        }
        if let Some((worker, args)) = self.worker_call(e) {
            let owner = self.owner.take();
            let js = self.ordered(&args);
            let call = format!("{}{worker}({})", self.mark(e.span), js.join(", "));
            return if runtime {
                self.run(owner, &call)
            } else {
                call
            };
        }
        let owner = self.owner.take();
        let (f, a) = if arg_first {
            let a = self.expr(arg);
            let a = self.spill(a);
            (self.expr(func), a)
        } else {
            let js = self.ordered(&[func, arg]);
            (js[0].clone(), js[1].clone())
        };
        // `(x) => (y)` ends with `)` too, yet needs parentheses to be called
        let call_like = f.ends_with(')') && !f.contains("=>");
        let f = if atomic(&f) || call_like {
            f
        } else {
            format!("({f})")
        };
        let call = format!("{}{f}({a})", self.mark(e.span));
        if runtime {
            assert!(self.generator, "a runtime call runs inside a runtime stage");
            self.run(owner, &call)
        } else {
            call
        }
    }

    /// The constant of `method` of `dict`, a canonical instance of this
    /// module whose method is emitted on its own.
    fn direct_method(&mut self, dict: &Dict, method: &str) -> Option<String> {
        let Dict::Instance { module, name, args } = dict else {
            return None;
        };
        if !args.is_empty() {
            return None;
        }
        if module != self.module {
            let constant = format!("{}${method}", names::instance(name));
            return (self.options.direct)(module, name, method)
                .then(|| self.global(module, &constant));
        }
        self.direct.get(&(name.clone(), method.to_owned())).cloned()
    }

    /// A known full application of a function of the module with a worker,
    /// its stages before the last pure and none evaluating its argument first.
    fn worker_call<'e>(&mut self, e: &'e Expr) -> Option<(String, Vec<&'e Expr>)> {
        let mut args = Vec::new();
        let mut head = e;
        let mut last = true;
        while let ExprKind::App {
            func,
            arg,
            runtime,
            arg_first: false,
        } = &head.kind
        {
            if *runtime && !last {
                return None;
            }
            last = false;
            args.push(&**arg);
            head = func;
        }
        let ExprKind::Global { module, name } = &head.kind else {
            return None;
        };
        args.reverse();
        if module != self.module {
            let arity = (self.options.workers)(module, name).filter(|&n| n == args.len())?;
            let worker = self.global(module, &format!("{}${arity}", names::ident(name)));
            return Some((worker, args));
        }
        if let Some((arity, alias)) = self.direct_foreigns.get(name) {
            return (args.len() == *arity).then(|| (alias.clone(), args));
        }
        let (arity, _) = *self.workers.get(name)?;
        (args.len() == arity).then(|| (format!("{}${arity}", names::ident(name)), args))
    }

    /// Release: a tagged constructor applied to all its fields, as the
    /// object it builds, `{ _tag: "Rect#…", _0: 3, _1: 4 }`.
    fn constructed(&mut self, e: &Expr) -> Option<String> {
        if !self.options.release {
            return None;
        }
        let mut args = Vec::new();
        let mut head = e;
        while let ExprKind::App {
            func,
            arg,
            runtime: false,
            arg_first: false,
        } = &head.kind
        {
            args.push(&**arg);
            head = func;
        }
        let ExprKind::Con(c) = &head.kind else {
            return None;
        };
        if c.rep != Rep::Tagged || names::native(c).is_some() || args.len() != c.arity {
            return None;
        }
        args.reverse();
        let js = self.ordered(&args);
        let package = (self.options.package)(&c.module);
        let tag = names::string(&names::tag(c, package.as_deref()));
        let fields: Vec<String> = js
            .iter()
            .enumerate()
            .map(|(i, v)| format!("_{i}: {v}"))
            .collect();
        Some(format!(
            "{}{{ _tag: {tag}, {} }}",
            self.mark(e.span),
            fields.join(", ")
        ))
    }

    /// `yield*` of the effect `js`, in `owner` when a `Within` gave one.
    fn run(&mut self, owner: Option<String>, js: &str) -> String {
        match owner {
            Some(owner) => format!("(yield* {}({owner}, {js}))", self.rt("within")),
            None => format!("(yield* {js})"),
        }
    }

    fn provide(&mut self, layer: &Expr, body: &Expr) -> String {
        let layer = self.expr(layer);
        let layer = self.spill(layer);
        let body = self.gen_block(body);
        let provide = self.rt("provide");
        format!("(yield* {provide}({layer}, {body}))")
    }

    fn scope(&mut self, owner: Option<&str>, outer: Option<&str>, body: &Expr) -> String {
        let owner = owner.map_or("$_".to_owned(), names::ident);
        let params = match outer {
            Some(outer) => format!("{owner}, {}", names::ident(outer)),
            None => owner,
        };
        let body = self.gen_block(body);
        let scope = self.rt("scope");
        format!("(yield* {scope}({} {body}))", self.arrow(&params))
    }

    /// A value that takes the dictionaries `shown` before it is computed.
    fn abstraction(&mut self, shown: &[String], inner: &Expr) -> String {
        let mark = self.givens.len();
        let params = self.bind_givens(shown);
        let saved = mem::replace(&mut self.generator, false);
        let (stmts, value) = self.block(|g| g.expr(inner));
        self.generator = saved;
        self.givens.truncate(mark);
        format!("{} {}", self.arrow(&params), arrow_body(&stmts, &value))
    }

    fn if_expr(&mut self, c: &Expr, t: &Expr, f: &Expr) -> String {
        let c = self.expr(c);
        let (ts, tv) = self.block(|g| g.expr(t));
        let (fs, fv) = self.block(|g| g.expr(f));
        if ts.is_empty() && fs.is_empty() {
            return format!("({c} ? {tv} : {fv})");
        }
        let r = self.temp();
        let declare = self.declare(&r);
        self.buf.push(format!(
            "{declare}\nif ({c}) {{\n{}  {r} = {tv};\n}} else {{\n{}  {r} = {fv};\n}}",
            indent(&ts),
            indent(&fs)
        ));
        r
    }

    fn try_expr(&mut self, body: &Expr, arms: &[Arm]) -> String {
        if !self.generator {
            return self.expr(body);
        }
        let body = self.gen_block(body);
        let error = self.temp();
        let (stmts, ()) = self.block(|g| g.handlers(&error, arms));
        let try_with = self.rt("tryWith");
        format!(
            "(yield* {try_with}({body}, {} {{\n{}}}))",
            self.arrow(&error),
            indent(&stmts)
        )
    }

    /// A record literal, or `{ base with … }`.
    fn record(&mut self, base: Option<&Expr>, fields: &[(String, Expr)]) -> String {
        let mut all: Vec<&Expr> = base.into_iter().collect();
        all.extend(fields.iter().map(|(_, v)| v));
        let mut js = self.ordered(&all);
        let base = base.map(|_| js.remove(0));
        let mut entries: Vec<String> = base.map(|b| format!("...{b}")).into_iter().collect();
        entries.extend(
            fields
                .iter()
                .zip(js)
                .map(|((label, _), v)| format!("{}: {v}", key(label))),
        );
        format!("{{ {} }}", entries.join(", "))
    }

    fn binary(&mut self, span: Span, op: BinOp, lhs: &Expr, rhs: &Expr) -> String {
        if matches!(op, BinOp::And | BinOp::Or) {
            let l = self.expr(lhs);
            let (rs, rv) = self.block(|g| g.expr(rhs));
            let js = if op == BinOp::And { "&&" } else { "||" };
            if rs.is_empty() {
                return format!("({l} {js} {rv})");
            }
            let t = self.temp();
            let test = if op == BinOp::And {
                t.clone()
            } else {
                format!("!{t}")
            };
            self.buf.push(format!(
                "let {t} = {l};\nif ({test}) {{\n{}  {t} = {rv};\n}}",
                indent(&rs)
            ));
            return t;
        }
        let js = self.ordered(&[lhs, rhs]);
        let (l, r) = (&js[0], &js[1]);
        let l = &format!("{}{l}", self.mark(span));
        match op {
            BinOp::Eq => format!("{}({l}, {r})", self.rt("equals")),
            BinOp::NotEq => format!("!{}({l}, {r})", self.rt("equals")),
            BinOp::ListConcat => format!("({l}).concat({r})"),
            BinOp::Concat => format!("({l} + {r})"),
            _ => format!("({l} {} {r})", op.symbol()),
        }
    }

    pub(super) fn stmt(&mut self, stmt: &Stmt) {
        let span = match stmt {
            Stmt::Let(_, e) | Stmt::Expr(e) | Stmt::Fun { value: e, .. } => e.span,
        };
        let first = self.buf.len();
        self.stmt_kind(stmt);
        if first < self.buf.len() {
            let mark = self.mark(span);
            self.buf[first].insert_str(0, &mark);
        }
    }

    fn stmt_kind(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let(Pat::Bind(name), value) => {
                let v = self.expr(value);
                self.buf.push(self.constant(&names::ident(name), &v));
            }
            Stmt::Let(pat, value) => {
                let v = self.expr(value);
                let v = self.spill(v);
                self.destructure(pat, &v);
            }
            Stmt::Fun {
                name,
                rec,
                dicts,
                value,
            } => {
                let target = rec.then(|| Target::Local(name.clone()));
                let f = self.function(dicts, value, target);
                self.buf.push(self.constant(&names::ident(name), &f));
            }
            Stmt::Expr(e) => {
                let v = self.expr(e);
                if !atomic(&v) {
                    self.buf.push(format!("{v};"));
                }
            }
        }
    }

    pub(super) fn constructor(&mut self, c: &Constructor) -> String {
        match names::native(c) {
            Some(names::Native::Some) => self.rt("some"),
            Some(names::Native::None) => self.rt("none"),
            Some(names::Native::Ok) => self.rt("ok"),
            Some(names::Native::Err) => self.rt("err"),
            None => self.global(&c.module, &names::ident(&c.name)),
        }
    }

    fn service_ref(&mut self, key: &str) -> String {
        let (module, name) = key.rsplit_once('.').unwrap_or(("", key));
        self.global(module, &names::service(name))
    }

    /// A JS function that turns a pure value into the runtime one `lift` describes.
    fn lift(&mut self, lift: &Lift) -> String {
        match lift {
            Lift::Stage { lift, result } => {
                let inner = result.as_deref().map(|r| self.lift(r));
                let call = "$f($x)";
                let body = match (lift, inner) {
                    (true, None) => format!("{}(() => {call})", self.effect("sync")),
                    (true, Some(r)) => format!("{}(() => ({r})({call}))", self.effect("sync")),
                    (false, Some(r)) => format!("{}({call}, {r})", self.effect("map")),
                    (false, None) => call.to_owned(),
                };
                format!("{} {} {body}", self.arrow("$f"), self.arrow("$x"))
            }
            Lift::Eff(inner) => {
                let inner = self.lift(inner);
                let map = self.effect("map");
                format!("{} {map}($e, {inner})", self.arrow("$e"))
            }
        }
    }
}

/// `js` without its leading source markers.
pub(super) fn unmarked(js: &str) -> &str {
    let mut rest = js;
    while let Some(after) = rest.strip_prefix(super::MARK_START) {
        rest = after.split_once(super::MARK_END).map_or("", |(_, r)| r);
    }
    rest
}

/// The body of an arrow returning `value` after `stmts`.
pub fn arrow_body(stmts: &[String], value: &str) -> String {
    if stmts.is_empty() {
        if unmarked(value).starts_with('{') {
            format!("({value})")
        } else {
            value.to_owned()
        }
    } else {
        format!("{{\n{}  return {value};\n}}", indent(stmts))
    }
}

pub fn literal(lit: &Lit) -> String {
    match lit {
        Lit::Number(n) | Lit::BigInt(n) => n.clone(),
        Lit::String(s) => names::string(s),
        Lit::Bool(b) => b.to_string(),
        Lit::Unit => "undefined".to_owned(),
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
