//! Functions: one arrow per stage, the body at the last; a runtime stage is
//! an `Effect.gen`. A pure function whose tail position calls itself with
//! all its arguments, and its own dictionaries, is a loop.

use std::mem;

use super::expr::{arrow_body, unmarked};
use super::{Gen, Tail, indent, names, view};
use crate::codegen::tail::{Calls, Target, self_call, tail_callees};
use crate::ir::{BinOp, Expr, ExprKind, Lit, Param, Pat, Stage};

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
            ExprKind::Lambda { params, body } => self.lambda(params, body, target, false),
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
        let saved = (
            mem::take(&mut self.buf),
            self.generator,
            self.sync,
            self.tail.take(),
        );
        let last = params.last().map_or(Stage::Pure, |p| p.stage);
        let last_runtime = last == Stage::Runtime;
        self.generator = last_runtime;
        self.sync = last == Stage::Sync;
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
        let vars: Vec<String> = (0..heads.len()).map(|i| format!("$l{i}")).collect();
        let grouped = target.as_ref().and_then(|t| self.group_call(t, &heads));
        let tail = target
            .filter(|_| grouped.is_none() && params.iter().all(|p| p.stage == Stage::Pure))
            .map(|target| Tail {
                members: vec![(target, params.len())],
                current: 0,
                vars: vars.clone(),
                binds: binds(params),
                state: None,
                offsets: Vec::new(),
            })
            .filter(|t| has_tail_call(body, t));
        let stmts = if let Some(call) = grouped {
            vec![format!("return {call};")]
        } else if let Some(mut tail) = tail {
            tail.offsets = offsets(&view::positions(&[(params, body)], &tail.members));
            self.looping(&mut heads, &destructure, body, tail)
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
            if i + 1 < params.len() && params[i].stage == Stage::Runtime {
                text = format!("{}({text})", self.effect("succeed"));
            }
            text = format!("{} {text}", self.arrow(head));
        }
        (self.buf, self.generator, self.sync, self.tail) = saved;
        text
    }

    /// The loop of a function jumping to `tail`: the heads become the
    /// initial values of its variables.
    fn looping(
        &mut self,
        heads: &mut Vec<String>,
        destructure: &[(Pat, String)],
        body: &Expr,
        tail: Tail,
    ) -> Vec<String> {
        let (vars, offsets) = (tail.vars.clone(), tail.offsets.clone());
        let (inner, ()) = self.block(|g| {
            for (head, var) in heads.iter().zip(&vars) {
                let c = g.constant(head, var);
                g.buf.push(c);
            }
            for (pat, t) in destructure {
                g.destructure(pat, t);
            }
            g.walk(heads, &offsets, |g| {
                g.tail = Some(tail);
                g.tail_expr(body);
                g.tail = None;
            });
        });
        let loop_heads: Vec<String> = heads.iter().map(|h| format!("{h}$")).collect();
        let mut init: Vec<String> = vars
            .iter()
            .zip(&loop_heads)
            .map(|(v, h)| format!("{v} = {h}"))
            .collect();
        init.extend(offsets.iter().flatten().map(|o| format!("{o} = 0")));
        *heads = loop_heads;
        vec![format!(
            "let {};\nfor (;;) {{\n{}}}",
            init.join(", "),
            indent(&inner)
        )]
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
                let s = match &scrutinee.kind {
                    // a walked list is tested by its name, which has its offset
                    ExprKind::Local(n) if self.views.contains_key(&names::ident(n)) => {
                        names::ident(n)
                    }
                    _ => {
                        let s = self.expr(scrutinee);
                        self.spill(s)
                    }
                };
                self.arms(&s, arms, None);
            }
            // the right operand is in tail position: a jump there needs a statement
            ExprKind::Binary(op @ (BinOp::And | BinOp::Or), lhs, rhs)
                if self.tail.as_ref().is_some_and(|t| has_tail_call(rhs, t)) =>
            {
                let l = self.expr(lhs);
                let (test, value) = if *op == BinOp::And {
                    (format!("!{l}"), "false")
                } else {
                    (l, "true")
                };
                self.buf
                    .push(format!("if ({test}) {{\n  return {value};\n}}"));
                self.tail_expr(rhs);
            }
            _ => {
                if let Some((j, args)) = self
                    .tail
                    .as_ref()
                    .and_then(|t| self_call(e, &t.members, Calls::Pure))
                {
                    let (vars, binds, state, offsets) = self
                        .tail
                        .as_ref()
                        .map(|t| {
                            let state = t.state.clone().filter(|_| j != t.current);
                            (t.vars.clone(), t.binds.clone(), state, t.offsets.clone())
                        })
                        .unwrap_or_default();
                    let offset = |i: usize| offsets.get(i).cloned().flatten();
                    // the rest of a walked list passed on at its own position moves its offset
                    let moves: Vec<(usize, String)> = (0..args.len())
                        .filter(|&i| offset(i).is_some())
                        .filter_map(|i| match &args[i].kind {
                            ExprKind::Local(r) => match self.rests.get(r) {
                                Some((pos, start)) if *pos == i => Some((i, start.clone())),
                                _ => None,
                            },
                            _ => None,
                        })
                        .collect();
                    // a parameter passed on unchanged keeps its variable
                    let changed: Vec<usize> = (0..args.len())
                        .filter(|&i| !moves.iter().any(|(m, _)| *m == i))
                        .filter(|&i| !matches!(&args[i].kind, ExprKind::Local(n) if binds.get(i).is_some_and(|b| b.as_ref() == Some(n))))
                        .collect();
                    let args: Vec<&Expr> = changed.iter().map(|&i| args[i]).collect();
                    // the arguments read the parameters, never the loop variables
                    let js = self.ordered(&args);
                    for (&i, value) in changed.iter().zip(js) {
                        self.buf.push(format!("{} = {value};", vars[i]));
                        if let Some(o) = offset(i) {
                            self.buf.push(format!("{o} = 0;"));
                        }
                    }
                    for (i, start) in moves {
                        if let Some(o) = offset(i) {
                            self.buf.push(format!("{o} = {start};"));
                        }
                    }
                    if let Some(state) = state {
                        self.buf.push(format!("{state} = {j};"));
                    }
                    self.buf.push("continue;".to_owned());
                    return;
                }
                let v = self.expr(e);
                let mark = mark!(self, e.span);
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

/// The offset variable of each walked position.
pub(super) fn offsets(walked: &[bool]) -> Vec<Option<String>> {
    walked
        .iter()
        .enumerate()
        .map(|(i, &w)| w.then(|| format!("$o{i}")))
        .collect()
}

/// The local each parameter binds, if it is a name.
pub(super) fn binds(params: &[Param]) -> Vec<Option<String>> {
    params
        .iter()
        .map(|p| match &p.pat {
            Pat::Bind(n) => Some(n.clone()),
            _ => None,
        })
        .collect()
}

/// Whether a tail position of `body` jumps to a member of `tail`.
fn has_tail_call(body: &Expr, tail: &Tail) -> bool {
    !tail_callees(body, &tail.members, Calls::Pure).is_empty()
}
