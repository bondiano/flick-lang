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
                self.lambda(params, body, target)
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
            format!("({params}) => {body}")
        }
    }

    /// A curried function: one arrow per stage, the body at the last one.
    pub(super) fn lambda(
        &mut self,
        params: &[Param],
        body: &Expr,
        target: Option<Target>,
    ) -> String {
        let saved = (mem::take(&mut self.buf), self.generator, self.tail.take());
        let last_runtime = params.last().is_some_and(|p| p.runtime);
        self.generator = last_runtime;
        let mut heads = Vec::new();
        let mut destructure = Vec::new();
        for p in params {
            let name = match &p.pat {
                Pat::Bind(n) => names::ident(n),
                Pat::Wild | Pat::Lit(Lit::Unit) => "$_".to_owned(),
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
                    g.buf.push(format!("const {head} = {var};"));
                }
                for (pat, t) in &destructure {
                    g.destructure(pat, t);
                }
                g.tail = Some(Tail {
                    target,
                    vars: vars.clone(),
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
            format!(
                "{}(function* () {{\n{}}})",
                self.effect("gen"),
                indent(&stmts)
            )
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
        for (i, head) in heads.iter().enumerate().rev() {
            if i + 1 < params.len() && params[i].runtime {
                text = format!("{}({text})", self.effect("succeed"));
            }
            text = format!("({head}) => {text}");
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
                    let js = self.ordered(&args);
                    let temps: Vec<String> = js.into_iter().map(|j| self.spill(j)).collect();
                    let vars = self
                        .tail
                        .as_ref()
                        .map(|t| t.vars.clone())
                        .unwrap_or_default();
                    for (var, value) in vars.iter().zip(temps) {
                        self.buf.push(format!("{var} = {value};"));
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
        format!(
            "{}(function* () {{\n{}}})",
            self.effect("gen"),
            indent(&stmts)
        )
    }
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
