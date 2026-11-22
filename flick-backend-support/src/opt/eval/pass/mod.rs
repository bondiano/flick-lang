//! Module-level release pass driving bounded concrete evaluation over portable IR.
//!
//! Walks each item body with an immutable lexical environment of known pure
//! bindings, skipping doomed calls over dynamic operands in O(1), bounding
//! failed retries across rounds, and keeping independently successful inner
//! folds when an outer call declines.

mod analysis;
mod block;
mod walk;

use std::collections::HashMap;
use std::rc::Rc;

use flick_ir::{Expr, ExprKind, FileId, Lit, Module, Pat, Stage};

use super::materialize::{
    MaterializeContext, MaterializeLimits, check_profitability, expr_cost, materialize,
};
use super::pattern::{PatMatch, match_pat};
use super::value::{CLOSURE_BYTES, Closure, DictEnv, Env, SLOT_BYTES, Value, bigint_value};
use super::{
    DefTable, EvalBudget, EvalConfig, EvalCounters, EvalOutcome, PrimitiveEval, StopReason,
    evaluate,
};
use crate::Known;
use crate::opt::{Generics, bodies, devirt};
use analysis::{collect_single_use_private_sizes, collect_visible_external_ctors};
use block::is_literal_data;

/// Maximum AST nodes hashed into a structural retry fingerprint.
const FINGERPRINT_NODE_LIMIT: usize = 32;

/// Tracks failed trial counts across simplification rounds without collecting input strings.
#[derive(Debug, Default)]
pub struct RetryTracker {
    /// Failed count per `(span, full_structural_fingerprint)`.
    unchanged_failures: HashMap<(FileId, u32, u32, u64), u32>,
    /// Total failed count per `(span, head_fingerprint)` across rounds.
    site_failures: HashMap<(FileId, u32, u32, u64), u32>,
}

impl RetryTracker {
    fn should_skip(&self, expr: &Expr, nodes: u32, config: &EvalConfig) -> bool {
        let span = expr.span;
        let site_key = (span.file, span.start, span.end, head_fingerprint(expr));
        if self.site_failures.get(&site_key).copied().unwrap_or(0) >= config.max_failed_retries {
            return true;
        }
        let fp_key = (span.file, span.start, span.end, fingerprint(expr, nodes));
        self.unchanged_failures.get(&fp_key).copied().unwrap_or(0) >= 1
    }

    fn record_failure(&mut self, expr: &Expr, nodes: u32) {
        let span = expr.span;
        let site_key = (span.file, span.start, span.end, head_fingerprint(expr));
        *self.site_failures.entry(site_key).or_default() += 1;
        let fp_key = (span.file, span.start, span.end, fingerprint(expr, nodes));
        *self.unchanged_failures.entry(fp_key).or_default() += 1;
    }
}

fn fnv_mix(h: &mut u64, v: u64) {
    *h ^= v;
    *h = h.wrapping_mul(0x0100_0000_01b3);
}

fn fnv_str(h: &mut u64, s: &str) {
    fnv_mix(h, u64::try_from(s.len()).unwrap_or(u64::MAX));
    for &b in s.as_bytes() {
        fnv_mix(h, u64::from(b));
    }
}

fn expr_tag(e: &Expr) -> u64 {
    match &e.kind {
        ExprKind::App {
            arg_first, stage, ..
        } => {
            1 + u64::from(*arg_first) * 2
                + match stage {
                    Stage::Pure => 0,
                    Stage::Sync => 4,
                    Stage::Runtime => 8,
                }
        }
        ExprKind::Lit(_) => 15,
        ExprKind::Local(_) => 16,
        ExprKind::Global { .. } => 17,
        ExprKind::Con(_) => 18,
        ExprKind::Method { .. } => 19,
        ExprKind::Match(_, arms) => 20 + u64::try_from(arms.len()).unwrap_or(0),
        ExprKind::Field(..) => 40,
        ExprKind::Update(_, fields) => 50 + u64::try_from(fields.len()).unwrap_or(0),
        ExprKind::Block(stmts, _) => 60 + u64::try_from(stmts.len()).unwrap_or(0),
        ExprKind::If(..) => 80,
        ExprKind::Binary(..) => 90,
        ExprKind::Neg(..) => 100,
        ExprKind::Not(_) => 101,
        ExprKind::Ignore(_) => 102,
        ExprKind::Dicts(_, dicts) => 110 + u64::try_from(dicts.len()).unwrap_or(0),
        _ => 200,
    }
}

/// Identifies a call site's callee head without storing any strings.
fn head_fingerprint(expr: &Expr) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut head = expr;
    let mut depth = 0_u64;
    while let ExprKind::App { func, .. } = &head.kind {
        head = func;
        depth += 1;
        if depth >= 32 {
            break;
        }
    }
    fnv_mix(&mut h, depth);
    fnv_mix(&mut h, expr_tag(head));
    match &head.kind {
        ExprKind::Global { module, name } => {
            fnv_str(&mut h, module);
            fnv_str(&mut h, name);
        }
        ExprKind::Local(name) | ExprKind::Method { name, .. } => fnv_str(&mut h, name),
        ExprKind::Con(c) => {
            fnv_str(&mut h, &c.module);
            fnv_str(&mut h, &c.name);
        }
        _ => {}
    }
    h
}

/// Bounded iterative structural fingerprint of `expr` (zero strings stored).
fn fingerprint(expr: &Expr, nodes: u32) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325 ^ u64::from(nodes);
    let mut stack = vec![expr];
    let mut visited = 0_usize;
    while let Some(e) = stack.pop() {
        visited += 1;
        if visited > FINGERPRINT_NODE_LIMIT {
            break;
        }
        fnv_mix(&mut h, expr_tag(e));
        fnv_mix(&mut h, u64::from(e.span.start));
        fnv_mix(&mut h, u64::from(e.span.end));
        match &e.kind {
            ExprKind::Lit(Lit::Unit) => fnv_mix(&mut h, 1),
            ExprKind::Lit(Lit::Bool(b)) => fnv_mix(&mut h, 2 + u64::from(*b)),
            ExprKind::Lit(Lit::Int(n)) => fnv_mix(&mut h, u64::from(n.cast_unsigned())),
            ExprKind::Lit(Lit::Float(x)) => fnv_mix(&mut h, x.to_bits()),
            ExprKind::Lit(Lit::BigInt(s) | Lit::String(s)) => fnv_str(&mut h, s),
            ExprKind::Local(name) => fnv_str(&mut h, name),
            ExprKind::Global { module, name } => {
                fnv_str(&mut h, module);
                fnv_str(&mut h, name);
            }
            ExprKind::Con(c) => {
                fnv_str(&mut h, &c.module);
                fnv_str(&mut h, &c.name);
            }
            ExprKind::Field(_, label) | ExprKind::Method { name: label, .. } => {
                fnv_str(&mut h, label);
            }
            _ => {}
        }
        e.each_child(&mut |child| {
            if stack.len() < FINGERPRINT_NODE_LIMIT {
                stack.push(child);
            }
        });
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
    let visible_external_ctors = collect_visible_external_ctors(module);
    let single_use_private = collect_single_use_private_sizes(module, exported);
    let current_module = module.name.clone();
    let mat_ctx = MaterializeContext {
        current_module: &current_module,
        lang,
        visible_external_ctors: &visible_external_ctors,
    };

    let mut ctx = PassCtx {
        defs: &defs,
        lang,
        primitives,
        config,
        mat_ctx: &mat_ctx,
        single_use_private: &single_use_private,
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
    pub(super) single_use_private: &'a HashMap<String, u32>,
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
        let PatMatch::Matched(binds) = matched.ok()? else {
            return None;
        };
        let mut next = env.clone();
        for (name, v) in binds {
            next = next.with(name, v);
        }
        Some(next)
    }

    pub(super) fn eval_for_env<'e>(&mut self, expr: &'e Expr, env: &Env<'e>) -> Option<Value<'e>>
    where
        'a: 'e,
    {
        let mut budget = self.new_trial_budget()?;
        // Fast path for already-evaluated literals, locals, constructors, and lambdas.
        // Charge steps and constructed bytes before allocating any value into `Env`.
        let fast = match &expr.kind {
            ExprKind::Lit(Lit::Unit) => Some(budget.charge_steps(1).map(|()| Value::Unit)),
            ExprKind::Lit(Lit::Bool(b)) => Some(budget.charge_steps(1).map(|()| Value::Bool(*b))),
            ExprKind::Lit(Lit::Int(n)) => Some(budget.charge_steps(1).map(|()| Value::Int(*n))),
            ExprKind::Lit(Lit::Float(x)) => Some(budget.charge_steps(1).map(|()| Value::Float(*x))),
            ExprKind::Lit(Lit::BigInt(d)) => Some(
                budget
                    .charge_steps(1)
                    .and_then(|()| budget.charge_bytes(16))
                    .and_then(|()| bigint_value(d).ok_or(StopReason::UnknownOp))
                    .map(Value::BigInt),
            ),
            ExprKind::Lit(Lit::String(s)) => {
                let len = u32::try_from(s.len()).unwrap_or(u32::MAX);
                Some(
                    budget
                        .charge_steps(1)
                        .and_then(|()| budget.charge_bytes(len))
                        .map(|()| Value::String(Rc::from(s.as_str()))),
                )
            }
            ExprKind::Local(name) => Some(
                budget
                    .charge_steps(1)
                    .and_then(|()| env.get(name, &mut budget))
                    .and_then(|v| v.cloned().ok_or(StopReason::DynamicRead)),
            ),
            ExprKind::Con(ctor) => Some(
                budget
                    .charge_steps(1)
                    .and_then(|()| budget.charge_bytes(SLOT_BYTES))
                    .map(|()| Value::Con {
                        ctor: Rc::new(ctor.clone()),
                        args: Rc::from([]),
                        dicts: Rc::from([]),
                    }),
            ),
            ExprKind::Lambda { params, body } => Some(
                budget
                    .charge_steps(1)
                    .and_then(|()| budget.charge_bytes(CLOSURE_BYTES))
                    .map(|()| {
                        Value::Closure(Rc::new(Closure {
                            env: env.clone(),
                            dict_env: DictEnv::default(),
                            dict_params: &[],
                            params: params.as_slice(),
                            body: body.as_ref(),
                            rec_group: None,
                        }))
                    }),
            ),
            _ => None,
        };
        if let Some(res) = fast {
            *self.module_steps_remaining = budget.module_steps_remaining;
            return res.ok();
        }
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
        if is_literal_data(e) {
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
        if is_literal_data(e) {
            return EvalAttempt::KnownKept;
        }
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
                if new_expr == *e {
                    return EvalAttempt::KnownKept;
                }
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
            return self.single_use_private.get(name).copied().unwrap_or(0);
        }
        0
    }
}
