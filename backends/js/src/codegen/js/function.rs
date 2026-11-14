//! Functions: one arrow per stage, the body at the last; a runtime stage is
//! an `Effect.gen`. A pure function whose tail position calls itself with
//! all its arguments, and its own dictionaries, is a loop.

use std::mem;

use flick_pretty::docvec;

use super::doc::{
    After, Before, Head, Js, MULTILINE, Stmt, YIELDS, block, block_js, call, stmt_flags, text_flags,
};
use super::{Gen, Tail, names, view};
use crate::codegen::tail::{Target, binds, has_tail_call, jumpable, passed_on, self_call};
use crate::ir::{BinOp, Expr, ExprKind, Lit, Param, Pat, Stage};

impl<'d> Gen<'_, 'd> {
    /// A definition's value: a function of its dictionaries, if any, around
    /// its body; a self-recursive function may loop on `target`.
    pub(super) fn function(
        &mut self,
        dicts: &[String],
        value: &Expr,
        target: Option<Target>,
    ) -> Js<'d> {
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
                self.arrow_body(&stmts, v)
            }
        };
        self.givens.truncate(mark);
        if dicts.is_empty() {
            body
        } else {
            self.lambda_of(&params, body)
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
    ) -> Js<'d> {
        let a = self.a;
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
        let calls = self.calls();
        let tail = target
            .filter(|_| grouped.is_none() && jumpable(params, calls))
            .map(|target| Tail {
                members: vec![(target, params.len())],
                current: 0,
                vars: vars.clone(),
                binds: binds(params),
                state: None,
                offsets: Vec::new(),
            })
            .filter(|t| has_tail_call(body, &t.members, calls));
        let stmts = if let Some(call) = grouped {
            vec![Stmt::ret(a, None, call)]
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
        if self.kernel && last_runtime {
            let text = self.stage_function(&heads, &stmts, uncurried);
            (self.buf, self.generator, self.sync, self.tail) = saved;
            return text;
        }
        let mut text = if last_runtime {
            self.generator_of(&stmts)
        } else {
            match &stmts[..] {
                [only] if !only.multiline() && only.ret.is_some() => {
                    let (marks, value) = only.ret.expect("a return");
                    let value = value.marked_by(a, marks);
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
                _ => block_js(a, &stmts),
            }
        };
        if uncurried {
            text = self.lambda_of(&heads.join(", "), text);
        }
        for (i, head) in heads.iter().enumerate().rev().filter(|_| !uncurried) {
            if i + 1 < params.len() && params[i].stage == Stage::Runtime && !self.kernel {
                let succeed = self.effect("succeed");
                text = call(a, succeed, &[text]);
            }
            text = self.lambda_of(head, text);
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
    ) -> Vec<Stmt<'d>> {
        let (vars, offsets) = (tail.vars.clone(), tail.offsets.clone());
        let (inner, ()) = self.block(|g| {
            for (head, var) in heads.iter().zip(&vars) {
                let c = g.constant(head, g.leaf(var));
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
        let init = init.join(", ");
        let flags = text_flags(&init) | stmt_flags(&inner) | MULTILINE;
        let a = self.a;
        let doc = docvec![
            a,
            "let ",
            init,
            ";",
            a.line(),
            "for (;;) ",
            block(a, &inner)
        ];
        vec![Stmt::new(doc, flags)]
    }

    /// Emits `e` in tail position: its value is returned, a self tail call
    /// jumps to the next iteration.
    pub(super) fn tail_expr(&mut self, e: &Expr) {
        let arena = self.a;
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
                let stmt = self.if_else(c, &ts, &fs);
                self.buf.push(stmt);
            }
            ExprKind::Match(scrutinee, arms) => {
                let s = match &scrutinee.kind {
                    // a walked list is tested by its name, which has its offset
                    ExprKind::Local(n) if self.views.contains_key(&names::ident(n)) => {
                        names::ident(n)
                    }
                    _ => {
                        let s = self.expr(scrutinee);
                        self.subject(s)
                    }
                };
                self.arms(&s, arms, None);
            }
            // the right operand is in tail position: a jump there needs a statement
            ExprKind::Binary(op @ (BinOp::And | BinOp::Or), lhs, rhs)
                if self
                    .tail
                    .as_ref()
                    .is_some_and(|t| has_tail_call(rhs, &t.members, self.calls())) =>
            {
                let l = self.expr(lhs);
                let (test, value) = if *op == BinOp::And {
                    let test = docvec![arena, "!", l.at(arena, Before::Other, After::Delimiter)];
                    (test, "false")
                } else {
                    (l.at(arena, Before::Delimiter, After::Delimiter), "true")
                };
                let returned = Stmt::new(docvec![arena, "return ", value, ";"], 0);
                let doc = docvec![arena, "if (", test, ") ", block(arena, &[returned])];
                self.buf.push(Stmt::new(doc, l.flags | MULTILINE));
                self.tail_expr(rhs);
            }
            _ => {
                if let Some((j, args)) = self
                    .tail
                    .as_ref()
                    .and_then(|t| self_call(e, &t.members, self.calls()))
                {
                    self.jump(j, &args);
                    return;
                }
                let v = self.expr(e);
                let v = self.tail_call(v);
                let mark = mark!(self, e.span);
                self.buf.push(Stmt::ret(arena, mark, v));
            }
        }
    }

    /// `if (c) { ts } else { fs }`.
    pub(super) fn if_else(&self, c: Js<'d>, ts: &[Stmt<'d>], fs: &[Stmt<'d>]) -> Stmt<'d> {
        let a = self.a;
        let doc = docvec![
            a,
            "if (",
            c.at(a, Before::Delimiter, After::Delimiter),
            ") ",
            block(a, ts),
            " else ",
            block(a, fs)
        ];
        Stmt::new(doc, c.flags | stmt_flags(ts) | stmt_flags(fs) | MULTILINE)
    }

    /// A self tail call to the member `j` with `args`: the loop variables
    /// take the arguments, and the loop goes on.
    fn jump(&mut self, j: usize, args: &[&Expr]) {
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
            .filter(|&i| !passed_on(args, &binds, i))
            .collect();
        let args: Vec<&Expr> = changed.iter().map(|&i| args[i]).collect();
        // the arguments read the parameters, never the loop variables
        let js = self.ordered(&args);
        for (&i, value) in changed.iter().zip(js) {
            self.buf.push(self.assign(&vars[i], value));
            if let Some(o) = offset(i) {
                self.buf.push(self.line(&format!("{o} = 0;")));
            }
        }
        for (i, start) in moves {
            if let Some(o) = offset(i) {
                self.buf.push(self.line(&format!("{o} = {start};")));
            }
        }
        if let Some(state) = state {
            self.buf.push(self.line(&format!("{state} = {j};")));
        }
        if self.kernel && self.generator {
            self.buf.push(Stmt::new(
                self.a.text("yield;"),
                YIELDS | super::doc::YIELD_TEXT,
            ));
        }
        self.buf.push(self.line("continue;"));
    }

    /// The body of a runtime stage, `task` or handler: an `Effect.gen`.
    pub(super) fn gen_block(&mut self, body: &Expr) -> Js<'d> {
        let saved = (self.generator, self.tail.take());
        self.generator = true;
        let (stmts, ()) = self.block(|g| g.tail_expr(body));
        (self.generator, self.tail) = saved;
        if self.kernel {
            self.stage_function(&[], &stmts, true)
        } else {
            self.generator_of(&stmts)
        }
    }

    /// An `Effect.gen` running `stmts`. In release, a lone `return (yield* x);`
    /// is `Effect.suspend(() => x)`: the same effect, built as late, without
    /// a generator.
    fn generator_of(&mut self, stmts: &[Stmt<'d>]) -> Js<'d> {
        let a = self.a;
        if self.options.release
            && let [only] = stmts
            && let Some((marks, value)) = only.ret
            && let Some((effect, flags)) = value.effect
        {
            let suspend = self.effect("suspend");
            let marks = marks.unwrap_or_else(|| a.empty());
            let effect = Js::code(docvec![a, marks, effect], Head::Ident, &[]).with(flags);
            return call(a, suspend, &[self.thunk(effect)]);
        }
        let effect_gen = self.effect("gen");
        call(a, effect_gen, &[self.generator_function("", stmts)])
    }
}

/// The offset variable of each walked position.
pub(super) fn offsets(walked: &[bool]) -> Vec<Option<String>> {
    walked
        .iter()
        .enumerate()
        .map(|(i, &w)| w.then(|| format!("$o{i}")))
        .collect()
}
