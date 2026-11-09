//! Expressions and statements. A JS expression is returned; statements it
//! needs go into the current statement list before it.

use std::mem;

use super::{Callee, Gen, Target, indent, literal, names, tidy};
use crate::ir::{Arm, Expr, ExprKind, Pat, Stage, Stmt};
use crate::numeric::Num;
use crate::types::{Constructor, Dict, Rep};

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
                    if !atomic(prev) || self.vars.contains(prev.as_str()) {
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
        if atomic(&js) && !self.vars.contains(&js) {
            return js;
        }
        let t = self.temp();
        let c = self.constant(&t, &js);
        self.buf.push(c);
        t
    }

    /// `-inner`, kept in 32 bits on `Int`; a literal is in range already, as
    /// an `Int` literal is checked with its sign.
    fn neg(&mut self, num: Num, inner: &Expr) -> String {
        let js = self.expr(inner);
        if num == Num::Int && !matches!(inner.kind, ExprKind::Lit(_)) {
            format!("(-{js} | 0)")
        } else {
            format!("(-{js})")
        }
    }

    pub(super) fn expr(&mut self, e: &Expr) -> String {
        match &e.kind {
            ExprKind::Lit(lit) => literal::literal(lit),
            ExprKind::Local(name) | ExprKind::Var(name) => names::ident(name),
            ExprKind::Global { module, name } => self.global(module, &names::ident(name)),
            ExprKind::Method { dict, name } => match self.direct_method(dict, name) {
                Some(direct) => direct,
                None => format!("{}.{name}", self.dict(dict)),
            },
            ExprKind::Con(c) => self.constructor(c),
            ExprKind::Perform {
                effect: service,
                op: name,
                stages,
            } => self.operation(service, name, stages, &[]),
            ExprKind::ServiceTag(key) => self.service_ref(key),
            ExprKind::Label(label) => names::string(label),
            ExprKind::Dicts(inner, dicts) => {
                let dicts: Vec<String> = dicts.iter().map(|d| self.dict(d)).collect();
                // the implementation takes them, once the operation looked it up
                if let ExprKind::Perform {
                    effect: service,
                    op: name,
                    stages,
                } = &inner.kind
                {
                    self.operation(service, name, stages, &dicts)
                } else {
                    let f = self.expr(inner);
                    format!("{f}({})", dicts.join(", "))
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
                let s = self.spill(s);
                let r = self.temp();
                self.buf.push(self.declare(&r));
                self.arms(&s, arms, Some(&r));
                r
            }
            ExprKind::Handle(h) if h.is_try() => self.try_expr(&h.body, &h.errors),
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
            ExprKind::Unwrap(inner) => self.unwrap(inner),
            ExprKind::Neg(num, inner) => self.neg(*num, inner),
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
                if self.kernel {
                    return self.await_task(owner, v);
                }
                self.run(owner, &v)
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

    fn app(&mut self, e: &Expr, func: &Expr, arg: &Expr, stage: Stage, arg_first: bool) -> String {
        if let Some(value) = self.constructed(e) {
            return value;
        }
        if let Some((worker, args)) = self.worker_call(e) {
            let owner = self.owner.take();
            let js = self.ordered(&args);
            let js = self.before_sync(stage, owner.is_some(), js);
            let call = format!("{}{worker}({})", mark!(self, e.span), js.join(", "));
            return self.call(stage, owner, call);
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
        let [f, a] = <[String; 2]>::try_from(self.before_sync(stage, owner.is_some(), vec![f, a]))
            .expect("two operands");
        // `(x) => (y)` ends with `)` too, yet needs parentheses to be called
        let call_like = tidy::is_call(&f);
        let f = if atomic(&f) || call_like {
            f
        } else {
            format!("({f})")
        };
        let call = format!("{}{f}({a})", mark!(self, e.span));
        self.call(stage, owner, call)
    }

    /// The operands of a Sync call in a generator as constants when one
    /// yields: `$rt.sync` wraps the call in an arrow, where `yield*` is
    /// not allowed.
    fn before_sync(&mut self, stage: Stage, within: bool, js: Vec<String>) -> Vec<String> {
        // on the kernel only `within` wraps a call in an arrow
        let wrapped = if self.kernel {
            within
        } else {
            stage == Stage::Sync
        };
        if !wrapped || !self.generator || !js.iter().any(|j| j.contains("yield")) {
            return js;
        }
        js.into_iter().map(|j| self.spill(j)).collect()
    }

    /// The JS call `call` of `stage` made at the current stage: a Sync one
    /// in a generator runs with the fiber's services.
    fn call(&mut self, stage: Stage, owner: Option<String>, call: String) -> String {
        if self.kernel {
            return if stage == Stage::Pure {
                call
            } else {
                self.run(owner, &call)
            };
        }
        match stage {
            Stage::Sync if self.generator => format!("(yield* {}(() => {call}))", self.rt("sync")),
            Stage::Pure | Stage::Sync => call,
            Stage::Runtime => self.run(owner, &call),
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
            stage,
            arg_first: false,
        } = &head.kind
        {
            if *stage == Stage::Runtime && !last {
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
            stage: Stage::Pure,
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
            mark!(self, e.span),
            fields.join(", ")
        ))
    }

    /// `yield*` of the effect `js`, in `owner` when a `Within` gave one.
    /// Runs the effect `js`: in a generator with `yield*`, in a Sync stage
    /// at once with the services of the context.
    pub(super) fn run(&mut self, owner: Option<String>, js: &str) -> String {
        if self.kernel {
            return self.kernel_run(owner, js);
        }
        let js = match owner {
            Some(owner) => format!("{}({owner}, {js})", self.rt("within")),
            None => js.to_owned(),
        };
        if self.generator {
            format!("(yield* {js})")
        } else {
            assert!(
                self.sync,
                "a runtime call runs inside a runtime or Sync stage"
            );
            format!("{}({js})", self.rt("runIn"))
        }
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
        let tried = format!(
            "{try_with}({body}, {} {{\n{}}})",
            self.arrow(&error),
            indent(&stmts)
        );
        self.run(None, &tried)
    }

    /// `o?`: the value of the `Some`, after returning a `None` at once.
    fn unwrap(&mut self, option: &Expr) -> String {
        let v = self.expr(option);
        let v = self.spill(v);
        self.buf
            .push(format!("if ({v}._tag === \"None\") return {v};"));
        format!("{v}.value")
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

    pub(super) fn stmt(&mut self, stmt: &Stmt) {
        #[cfg(feature = "source-maps")]
        let span = match stmt {
            Stmt::Let(_, e)
            | Stmt::Var(_, e)
            | Stmt::Assign(_, e)
            | Stmt::Expr(e)
            | Stmt::Fun { value: e, .. } => e.span,
        };
        #[cfg(feature = "source-maps")]
        let first = self.buf.len();
        self.stmt_kind(stmt);
        #[cfg(feature = "source-maps")]
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
            Stmt::Var(name, value) => {
                let v = self.expr(value);
                let name = names::ident(name);
                self.buf.push(self.variable(&name, &v));
                self.vars.insert(name);
            }
            Stmt::Assign(name, value) => {
                let v = self.expr(value);
                self.buf.push(format!("{} = {v};", names::ident(name)));
            }
            Stmt::Fun {
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
        // TypeScript types the literal as itself, so `Get == Post` would not
        // check: it names the constructor, typed as the program's values are
        if let Rep::Literal(literal) = &c.rep
            && !self.options.typescript
        {
            return names::enum_literal(literal);
        }
        match names::native(c) {
            Some(names::Native::Some) => self.rt("some"),
            Some(names::Native::None) => self.rt("none"),
            Some(names::Native::Ok) => self.rt("ok"),
            Some(names::Native::Err) => self.rt("err"),
            None => self.global(&c.module, &names::ident(&c.name)),
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

/// A record key in an object literal.
fn key(label: &str) -> String {
    if names::is_property(label) && label != "__proto__" {
        label.to_owned()
    } else {
        format!("[{}]", names::string(label))
    }
}
