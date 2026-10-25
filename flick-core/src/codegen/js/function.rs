//! Functions: one arrow per stage, the body at the last; a runtime stage is
//! an `Effect.gen`. A pure function whose tail position calls itself with
//! all its arguments is a loop.

use std::mem;

use super::expr::{arrow_body, unmarked};
use super::{Gen, Tail, Target, indent, names};
use crate::ir::{Expr, ExprKind, Lit, Param, Pat};

impl Gen<'_> {
    /// A definition's value: a function of its dictionaries, if any, around
    /// its body; a self-recursive function may loop on `target`.
    pub(super) fn function(
        &mut self,
        dicts: &[String],
        value: &Expr,
        target: Option<Target>,
    ) -> String {
        let mark = self.givens.len();
        let params = self.bind_givens(dicts);
        let body = match &value.kind {
            ExprKind::Lambda { params, body } if dicts.is_empty() => {
                self.lambda(params, body, target, false)
            }
            // a value: its statements go before it, unless dictionaries defer it
            _ if dicts.is_empty() => self.expr(value),
            _ => {
                let saved = mem::replace(&mut self.generator, false);
                let (stmts, v) = self.block(|g| g.expr(value));
                self.generator = saved;
                arrow_body(&stmts, &v)
            }
        };
        self.givens.truncate(mark);
        if dicts.is_empty() {
            body
        } else {
            format!("{} {body}", self.arrow(&params))
        }
    }

    /// A curried function: one arrow per stage, the body at the last one;
    /// `uncurried`, one arrow taking every parameter (a worker).
    pub(super) fn lambda(
        &mut self,
        params: &[Param],
        body: &Expr,
        target: Option<Target>,
        uncurried: bool,
    ) -> String {
        let saved = (mem::take(&mut self.buf), self.generator, self.tail.take());
        let last_runtime = params.last().is_some_and(|p| p.runtime);
        self.generator = last_runtime;
        let mut heads = Vec::new();
        let mut destructure = Vec::new();
        for (i, p) in params.iter().enumerate() {
            let name = match &p.pat {
                Pat::Bind(n) => names::ident(n),
                // one arrow takes every parameter of a worker: names must differ
                Pat::Wild | Pat::Lit(Lit::Unit) if uncurried && heads.iter().any(|h| h == "$_") => {
                    format!("$_{i}")
                }
                // release keeps the generator's name for a parameter nobody reads
                Pat::Wild | Pat::Lit(Lit::Unit) if self.options.release => "$_".to_owned(),
                Pat::Wild | Pat::Lit(Lit::Unit) => "_".to_owned(),
                pat => {
                    let t = self.temp();
                    destructure.push((pat.clone(), t.clone()));
                    t
                }
            };
            heads.push(name);
        }
        let looping = target
            .filter(|t| params.iter().all(|p| !p.runtime) && has_tail_call(body, t, params.len()));
        let stmts = if let Some(target) = looping {
            let vars: Vec<String> = (0..heads.len()).map(|i| format!("$l{i}")).collect();
            let (inner, ()) = self.block(|g| {
                for (head, var) in heads.iter().zip(&vars) {
                    let c = g.constant(head, var);
                    g.buf.push(c);
                }
                for (pat, t) in &destructure {
                    g.destructure(pat, t);
                }
                g.tail = Some(Tail {
                    target,
                    vars: vars.clone(),
                    binds: binds(params),
                });
                g.tail_expr(body);
                g.tail = None;
            });
            let loop_heads: Vec<String> = heads.iter().map(|h| format!("{h}$")).collect();
            let init: Vec<String> = vars
                .iter()
                .zip(&loop_heads)
                .map(|(v, h)| format!("{v} = {h}"))
                .collect();
            heads = loop_heads;
            vec![format!(
                "let {};\nfor (;;) {{\n{}}}",
                init.join(", "),
                indent(&inner)
            )]
        } else {
            let (stmts, ()) = self.block(|g| {
                for (pat, t) in &destructure {
                    g.destructure(pat, t);
                }
                g.tail_expr(body);
            });
            stmts
        };
        let mut text = if last_runtime {
            self.generator_of(&stmts)
        } else {
            match &stmts[..] {
                [only] if only.starts_with("return ") && !only.contains('\n') => {
                    let v = &only["return ".len()..only.len() - 1];
                    if unmarked(v).starts_with('{') {
                        format!("({v})")
                    } else {
                        v.to_owned()
                    }
                }
                _ => format!("{{\n{}}}", indent(&stmts)),
            }
        };
        if uncurried {
            text = format!("{} {text}", self.arrow(&heads.join(", ")));
        }
        for (i, head) in heads.iter().enumerate().rev().filter(|_| !uncurried) {
            if i + 1 < params.len() && params[i].runtime {
                text = format!("{}({text})", self.effect("succeed"));
            }
            text = format!("{} {text}", self.arrow(head));
        }
        (self.buf, self.generator, self.tail) = saved;
        text
    }

    /// Emits `e` in tail position: its value is returned, a self tail call
    /// jumps to the next iteration.
    pub(super) fn tail_expr(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::Block(stmts, result) => {
                for stmt in stmts {
                    self.stmt(stmt);
                }
                self.tail_expr(result);
            }
            ExprKind::If(c, t, f) => {
                let c = self.expr(c);
                let (ts, ()) = self.block(|g| g.tail_expr(t));
                let (fs, ()) = self.block(|g| g.tail_expr(f));
                self.buf.push(format!(
                    "if ({c}) {{\n{}}} else {{\n{}}}",
                    indent(&ts),
                    indent(&fs)
                ));
            }
            ExprKind::Match(scrutinee, arms) => {
                let s = self.expr(scrutinee);
                let s = self.spill(s);
                self.arms(&s, arms, None);
            }
            _ => {
                if let Some(args) = self.tail.as_ref().and_then(|t| self_call(e, t)) {
                    let (vars, binds) = self
                        .tail
                        .as_ref()
                        .map(|t| (t.vars.clone(), t.binds.clone()))
                        .unwrap_or_default();
                    // a parameter passed on unchanged keeps its variable
                    let changed: Vec<usize> = (0..args.len())
                        .filter(|&i| !matches!(&args[i].kind, ExprKind::Local(n) if binds[i].as_ref() == Some(n)))
                        .collect();
                    let args: Vec<&Expr> = changed.iter().map(|&i| args[i]).collect();
                    // the arguments read the parameters, never the loop variables
                    let js = self.ordered(&args);
                    for (&i, value) in changed.iter().zip(js) {
                        self.buf.push(format!("{} = {value};", vars[i]));
                    }
                    self.buf.push("continue;".to_owned());
                    return;
                }
                let v = self.expr(e);
                let mark = self.mark(e.span);
                self.buf.push(format!("return {mark}{v};"));
            }
        }
    }

    /// The body of a runtime stage, `task` or handler: an `Effect.gen`.
    pub(super) fn gen_block(&mut self, body: &Expr) -> String {
        let saved = (self.generator, self.tail.take());
        self.generator = true;
        let (stmts, ()) = self.block(|g| g.tail_expr(body));
        (self.generator, self.tail) = saved;
        self.generator_of(&stmts)
    }

    /// An `Effect.gen` running `stmts`. In release, a lone `return (yield* x);`
    /// is `Effect.suspend(() => x)`: the same effect, built as late, without
    /// a generator.
    fn generator_of(&mut self, stmts: &[String]) -> String {
        if self.options.release
            && let [only] = stmts
            && let Some(effect) = lone_yield(only)
        {
            return format!("{}(() => {effect})", self.effect("suspend"));
        }
        format!(
            "{}(function* () {{\n{}}})",
            self.effect("gen"),
            indent(stmts)
        )
    }
}

/// `x` of `return (yield* x);` when `x` yields nothing itself, with the
/// markers before it.
fn lone_yield(stmt: &str) -> Option<String> {
    let value = stmt.strip_prefix("return ")?.strip_suffix(';')?;
    let code = unmarked(value);
    let marks = &value[..value.len() - code.len()];
    let inner = code.strip_prefix("(yield* ")?.strip_suffix(')')?;
    (!stmt.contains('\n') && balanced_without_yield(inner)).then(|| format!("{marks}{inner}"))
}

/// Whether the parentheses of `js` balance and it has no `yield` outside a
/// nested `function*`, skipping string literals.
fn balanced_without_yield(js: &str) -> bool {
    let bytes = js.as_bytes();
    // braces open, `true` for the body of a nested generator
    let mut braces: Vec<bool> = Vec::new();
    let mut parens = 0usize;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            quote @ (b'"' | b'\'' | b'`') => {
                i += 1;
                while i < bytes.len() && bytes[i] != quote {
                    i += if bytes[i] == b'\\' { 2 } else { 1 };
                }
            }
            b'(' => parens += 1,
            b')' => match parens.checked_sub(1) {
                Some(p) => parens = p,
                None => return false,
            },
            b'{' => braces.push(js[..i].trim_end().ends_with("function* ()")),
            b'}' => {
                braces.pop();
            }
            b'y' if js[i..].starts_with("yield") && !braces.contains(&true) => return false,
            _ => {}
        }
        i += 1;
    }
    parens == 0
}

/// The local each parameter binds, if it is a name.
fn binds(params: &[Param]) -> Vec<Option<String>> {
    params
        .iter()
        .map(|p| match &p.pat {
            Pat::Bind(n) => Some(n.clone()),
            _ => None,
        })
        .collect()
}

/// The arguments of a full pure call of the loop's function.
fn self_call<'e>(e: &'e Expr, tail: &Tail) -> Option<Vec<&'e Expr>> {
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
    args.reverse();
    let target = match &head.kind {
        ExprKind::Local(name) => Target::Local(name.clone()),
        ExprKind::Global { module, name } => Target::Global(format!("{module}.{name}")),
        _ => return None,
    };
    (target == tail.target && args.len() == tail.vars.len()).then_some(args)
}

/// Whether a tail position of `body` calls `target` with `arity` arguments.
fn has_tail_call(body: &Expr, target: &Target, arity: usize) -> bool {
    let tail = Tail {
        target: target.clone(),
        vars: vec![String::new(); arity],
        binds: Vec::new(),
    };
    let mut pending = vec![body];
    while let Some(e) = pending.pop() {
        match &e.kind {
            ExprKind::Block(_, result) => pending.push(result),
            ExprKind::If(_, t, f) => pending.extend([&**t, &**f]),
            ExprKind::Match(_, arms) => pending.extend(arms.iter().map(|a| &a.body)),
            _ if self_call(e, &tail).is_some() => return true,
            _ => {}
        }
    }
    false
}
