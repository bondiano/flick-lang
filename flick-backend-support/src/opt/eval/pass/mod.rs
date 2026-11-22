//! Module-level release pass driving bounded concrete evaluation over portable IR.
//!
//! Walks each item body with an immutable lexical environment of known pure
//! bindings, skipping doomed calls over dynamic operands in O(1), bounding
//! failed retries across rounds, and keeping independently successful inner
//! folds when an outer call declines.

mod analysis;
mod block;
mod walk;

use std::cmp::Ordering;
use std::collections::HashMap;

use flick_ir::{Expr, ExprKind, Lit, Module, Pat, Span, Stage};

use super::materialize::{
    MaterializeContext, MaterializeLimits, check_profitability, expr_cost, materialize,
};
use super::pattern::{PatMatch, match_pat};
use super::value::{DictEnv, Env, Value};
use super::{
    DefTable, EvalBudget, EvalConfig, EvalCounters, EvalOutcome, PrimitiveEval, StopReason,
    evaluate,
};
use crate::Known;
use crate::opt::{Generics, bodies, devirt};
use analysis::analyze_module;

/// Maximum AST nodes hashed into a structural retry fingerprint.
const FINGERPRINT_NODE_LIMIT: usize = 32;

/// Tracks failed trial counts across simplification rounds without collecting
/// input strings, storing both site and unchanged-expression counts in a single
/// sorted `(hash, count)` table (distinguished by a 1-bit domain tag).
#[derive(Debug, Default)]
pub struct RetryTracker(Vec<(u64, u32)>);

impl RetryTracker {
    fn find(&self, key: u64) -> Result<usize, usize> {
        let (mut lo, mut hi) = (0, self.0.len());
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            match self.0[mid].0.cmp(&key) {
                Ordering::Less => lo = mid + 1,
                Ordering::Greater => hi = mid,
                Ordering::Equal => return Ok(mid),
            }
        }
        Err(lo)
    }

    fn get(&self, key: u64) -> u32 {
        self.find(key).map_or(0, |idx| self.0[idx].1)
    }

    fn inc(&mut self, key: u64) {
        match self.find(key) {
            Ok(idx) => self.0[idx].1 = self.0[idx].1.saturating_add(1),
            Err(idx) => {
                self.0.push((key, 1));
                let mut i = self.0.len() - 1;
                while i > idx {
                    self.0.swap(i - 1, i);
                    i -= 1;
                }
            }
        }
    }

    fn should_skip(&self, expr: &Expr, nodes: u32, config: &EvalConfig) -> bool {
        let span = expr.span;
        if self.get(retry_key(span, head_fingerprint(expr), 1)) >= config.max_failed_retries {
            return true;
        }
        self.get(retry_key(span, fingerprint(expr, nodes), 0)) >= 1
    }

    fn record_failure(&mut self, expr: &Expr, nodes: u32) {
        let span = expr.span;
        self.inc(retry_key(span, head_fingerprint(expr), 1));
        self.inc(retry_key(span, fingerprint(expr, nodes), 0));
    }
}

fn retry_key(span: Span, mut fp: u64, domain: u64) -> u64 {
    fnv_mix(&mut fp, domain);
    fnv_mix(&mut fp, u64::from(span.file.index()));
    fnv_mix(&mut fp, (u64::from(span.start) << 32) | u64::from(span.end));
    fp
}

#[inline(never)]
fn fnv_mix(h: &mut u64, v: u64) {
    *h ^= v;
    *h = h.wrapping_mul(0x0100_0000_01b3);
}

#[inline(never)]
fn fnv_str(h: &mut u64, s: &str) {
    fnv_mix(h, u64::try_from(s.len()).unwrap_or(u64::MAX));
    for &b in s.as_bytes() {
        fnv_mix(h, u64::from(b));
    }
}

/// Identifies a call site's callee head without storing any strings.
fn head_fingerprint(expr: &Expr) -> u64 {
    let mut head = expr;
    let mut depth = 0_u64;
    while let ExprKind::App { func, .. } = &head.kind {
        head = func;
        depth += 1;
        if depth >= FINGERPRINT_NODE_LIMIT as u64 {
            break;
        }
    }
    hash_expr(head, depth, 1)
}

/// Bounded iterative structural fingerprint of `expr` (zero strings stored).
fn fingerprint(expr: &Expr, nodes: u32) -> u64 {
    hash_expr(expr, u64::from(nodes), FINGERPRINT_NODE_LIMIT)
}

fn hash_expr(root: &Expr, seed: u64, limit: usize) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325 ^ seed;
    let mut stack = vec![root];
    let mut visited = 0_usize;
    while let Some(e) = stack.pop() {
        visited += 1;
        if visited > limit {
            break;
        }
        fnv_mix(
            &mut h,
            (u64::from(e.span.start) << 32) | u64::from(e.span.end),
        );
        let (tag, s1, s2): (u64, &str, &str) = match &e.kind {
            ExprKind::App {
                arg_first, stage, ..
            } => {
                let st = match stage {
                    Stage::Pure => 0,
                    Stage::Sync => 4,
                    Stage::Runtime => 8,
                };
                (1 + u64::from(*arg_first) * 2 + st, "", "")
            }
            ExprKind::Lit(Lit::Unit) => (15, "", ""),
            ExprKind::Lit(Lit::Bool(b)) => (16 + u64::from(*b), "", ""),
            ExprKind::Lit(Lit::Int(n)) => {
                fnv_mix(&mut h, u64::from(n.cast_unsigned()));
                (18, "", "")
            }
            ExprKind::Lit(Lit::Float(x)) => {
                fnv_mix(&mut h, x.to_bits());
                (19, "", "")
            }
            ExprKind::Lit(Lit::BigInt(s) | Lit::String(s)) => (20, s, ""),
            ExprKind::Local(s) => (21, s, ""),
            ExprKind::Global { module, name } => (22, module, name),
            ExprKind::Con(c) => (23, &c.module, &c.name),
            ExprKind::Method { name, .. } => (24, name, ""),
            ExprKind::Field(_, s) => (25, s, ""),
            ExprKind::Match(_, arms) => (30 + u64::try_from(arms.len()).unwrap_or(0), "", ""),
            ExprKind::Update(_, fields) => (50 + u64::try_from(fields.len()).unwrap_or(0), "", ""),
            ExprKind::Block(stmts, _) => (70 + u64::try_from(stmts.len()).unwrap_or(0), "", ""),
            ExprKind::Dicts(_, dicts) => (90 + u64::try_from(dicts.len()).unwrap_or(0), "", ""),
            ExprKind::If(..) => (110, "", ""),
            ExprKind::Binary(..) => (111, "", ""),
            ExprKind::Neg(..) => (112, "", ""),
            ExprKind::Not(_) => (113, "", ""),
            ExprKind::Ignore(_) => (114, "", ""),
            _ => (200, "", ""),
        };
        fnv_mix(&mut h, tag);
        if !s1.is_empty() {
            fnv_str(&mut h, s1);
        }
        if !s2.is_empty() {
            fnv_str(&mut h, s2);
        }
        if limit > 1 {
            e.each_child(&mut |child| {
                if stack.len() < limit {
                    stack.push(child);
                }
            });
        }
    }
    h
}

/// Hint computed bottom-up while walking an expression: whether strict
/// evaluation of the expression could still produce a concrete [`Value`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Hint {
    /// Subexpressions have no known dynamic/failing strict dependency.
    Concrete,
    /// Expression contains a dynamic read, unsafe context, or failed strict subexpr.
    NonConcrete,
}

/// Outcome of attempting to evaluate and replace a compound expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EvalAttempt {
    /// Replaced the expression with a smaller/equivalent materialized IR tree.
    Replaced,
    /// Evaluated to a valid `Known` value that stayed unmaterialized (e.g. closure,
    /// large data, or already identical IR), so enclosing calls may still consume it.
    KnownKept,
    /// Evaluation stopped (dynamic read, trap, unknown op, unsafe context, budget, retry cap).
    Stopped,
}

/// Runs one round of bounded concrete evaluation across `module`.
/// Returns whether any expression in `module` was rewritten.
#[allow(clippy::too_many_arguments, reason = "pass state across rounds")]
pub fn run_module_pass(
    module: &mut Module,
    exported: &dyn Fn(&str) -> bool,
    lookup: &devirt::Lookup<'_>,
    generics: Generics<'_>,
    lang: &Known<'_>,
    primitives: &dyn PrimitiveEval,
    config: &EvalConfig,
    module_steps_remaining: &mut u32,
    retries: &mut RetryTracker,
    counters: &mut EvalCounters,
) -> bool {
    if *module_steps_remaining == 0 {
        return false;
    }
    let defs = DefTable::build(module, lookup, generics, lang);
    let analysis = analyze_module(module, exported);
    let current_module = module.name.clone();
    let mat_ctx = MaterializeContext {
        current_module: &current_module,
        lang,
        visible_external_ctors: &analysis.visible_external_ctors,
    };

    let mut ctx = PassCtx {
        defs: &defs,
        lang,
        primitives,
        config,
        mat_ctx: &mat_ctx,
        single_use_private: &analysis.single_use_private,
        module_steps_remaining,
        retries,
        counters,
        changed: false,
    };

    for item in &mut module.items {
        for body in bodies(item) {
            ctx.visit_expr(body, &Env::default(), false, 0);
        }
    }
    ctx.changed
}

pub(super) struct PassCtx<'a, 'b> {
    pub(super) defs: &'a DefTable,
    pub(super) lang: &'a Known<'a>,
    pub(super) primitives: &'b dyn PrimitiveEval,
    pub(super) config: &'b EvalConfig,
    pub(super) mat_ctx: &'a MaterializeContext<'a>,
    pub(super) single_use_private: &'a HashMap<String, usize>,
    pub(super) module_steps_remaining: &'b mut u32,
    pub(super) retries: &'b mut RetryTracker,
    pub(super) counters: &'b mut EvalCounters,
    pub(super) changed: bool,
}

impl<'a> PassCtx<'a, '_> {
    pub(super) fn bind_let_in_env<'e>(
        &mut self,
        pat: &'e Pat,
        rhs: &'e Expr,
        env: &Env<'e>,
    ) -> Option<Env<'e>>
    where
        'a: 'e,
    {
        let val = self.eval_for_env(rhs, env)?;
        let mut budget = self.new_trial_budget()?;
        let matched = match_pat(pat, val, &mut budget);
        *self.module_steps_remaining = budget.module_steps_remaining;
        let PatMatch::Matched(mut binds) = matched.ok()? else {
            return None;
        };
        let mut next = env.clone();
        while let Some((name, v)) = binds.pop() {
            next = next.with(name, v);
        }
        Some(next)
    }

    pub(super) fn eval_for_env<'e>(&mut self, expr: &'e Expr, env: &Env<'e>) -> Option<Value<'e>>
    where
        'a: 'e,
    {
        let mut budget = self.new_trial_budget()?;
        let outcome = evaluate(
            expr,
            self.defs,
            env.clone(),
            DictEnv::default(),
            self.lang,
            self.primitives,
            &mut budget,
        );
        *self.module_steps_remaining = budget.module_steps_remaining;
        match outcome {
            EvalOutcome::Known(v) => Some(v),
            EvalOutcome::Stopped(_) => None,
        }
    }

    pub(super) fn new_trial_budget(&self) -> Option<EvalBudget> {
        if *self.module_steps_remaining == 0 {
            return None;
        }
        Some(EvalBudget::new(self.config, *self.module_steps_remaining))
    }

    /// Tries to evaluate a call site `e` and replace it if materialization and
    /// profitability succeed. Returns `Hint::Concrete` when replaced OR when
    /// evaluation produced a valid `Known` value (even if not materialized).
    pub(super) fn try_eval_call_site<'e>(&mut self, e: &'e mut Expr, env: &Env<'e>) -> Hint
    where
        'a: 'e,
    {
        if matches!(e.pure_spine().0.kind, ExprKind::Con(_)) {
            return Hint::Concrete;
        }
        match self.try_eval_expr(e, env, true) {
            EvalAttempt::Replaced | EvalAttempt::KnownKept => Hint::Concrete,
            EvalAttempt::Stopped => Hint::NonConcrete,
        }
    }

    /// Tries to evaluate a non-call compound expression (`If`, `Match`, `Field`,
    /// `Update`, `Block`, `Binary`, `Neg`, `Not`, `Ignore`).
    pub(super) fn try_eval_and_replace<'e>(&mut self, e: &'e mut Expr, env: &Env<'e>) -> EvalAttempt
    where
        'a: 'e,
    {
        self.try_eval_expr(e, env, false)
    }

    fn try_eval_expr<'e>(
        &mut self,
        e: &'e mut Expr,
        env: &Env<'e>,
        credit_removed_def: bool,
    ) -> EvalAttempt
    where
        'a: 'e,
    {
        if *self.module_steps_remaining == 0 {
            self.counters.record_stop(StopReason::ModuleBudget);
            return EvalAttempt::Stopped;
        }
        let orig = expr_cost(e);
        if self.retries.should_skip(e, orig.nodes, self.config) {
            self.counters.record_stop(StopReason::RetryCap);
            return EvalAttempt::Stopped;
        }
        let removed_credit = if credit_removed_def {
            self.removed_def_credit(e)
        } else {
            0
        };
        let mut budget = EvalBudget::new(self.config, *self.module_steps_remaining);
        let replacement = {
            let outcome = evaluate(
                e,
                self.defs,
                env.clone(),
                DictEnv::default(),
                self.lang,
                self.primitives,
                &mut budget,
            );
            match outcome {
                EvalOutcome::Stopped(reason) => Err((reason, false)),
                EvalOutcome::Known(val) => {
                    let limits = MaterializeLimits::from_config(self.config);
                    match materialize(&val, e.span, self.mat_ctx, limits) {
                        Err(reason) => Err((reason, true)),
                        Ok(mat) => {
                            match check_profitability(orig, &mat, removed_credit, self.config) {
                                Ok(()) => Ok(mat.expr),
                                Err(reason) => Err((reason, true)),
                            }
                        }
                    }
                }
            }
        };
        *self.module_steps_remaining = budget.module_steps_remaining;
        match replacement {
            Ok(new_expr) => {
                *e = new_expr;
                self.changed = true;
                self.counters.record_success();
                EvalAttempt::Replaced
            }
            Err((reason, evaluated_known)) => {
                self.counters.record_stop(reason);
                if !evaluated_known || reason == StopReason::ResidualGrowth {
                    self.retries.record_failure(e, orig.nodes);
                }
                if evaluated_known {
                    EvalAttempt::KnownKept
                } else {
                    EvalAttempt::Stopped
                }
            }
        }
    }

    fn removed_def_credit(&self, e: &Expr) -> u32 {
        let mut head = e;
        while let ExprKind::App { func, .. } = &head.kind {
            head = func;
        }
        if let ExprKind::Dicts(inner, _) = &head.kind {
            head = inner;
        }
        if let ExprKind::Global { module, name } = &head.kind
            && module == self.mat_ctx.current_module
        {
            return self
                .single_use_private
                .get(name.as_str())
                .copied()
                .map_or(0, |n| u32::try_from(n).unwrap_or(u32::MAX));
        }
        0
    }
}
