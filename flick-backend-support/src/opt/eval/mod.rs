//! Bounded concrete evaluator over portable Flick IR (FL-OPT-01, #803).
//!
//! Evaluates closed pure calls in release builds using an explicit-frame step
//! loop with deterministic budgets (steps, frame depth, cumulative module work,
//! constructed bytes, residual node growth, and bounded failed retries across
//! rounds). Evaluation (`Known(value)` vs `Stopped(reason)`), fallible
//! materialization into IR, and call-site profitability are kept distinct.

mod defs;
mod machine;
pub mod materialize;
mod pass;
mod pattern;
pub mod primitives;
pub mod value;

#[cfg(test)]
mod tests;

use flick_ir::Expr;

use crate::Known;
pub use defs::DefTable;
use machine::Machine;
pub use pass::{RetryTracker, run_module_pass};
pub use primitives::{NativePrimitive, ReviewedPrimitives, js_target_allows};
pub use value::{DictEnv, DictValue, Env, Value};

/// Deterministic resource budgets controlling bounded concrete evaluation.
///
/// Every counter uses fixed-width `u32` limits so native 64-bit and browser
/// `wasm32` builds hit the exact same limits without exhausting the host stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvalConfig {
    /// Maximum evaluation steps allowed for a single trial.
    pub max_trial_steps: u32,
    /// Maximum explicit continuation frame depth (enforced before pushing).
    pub max_frame_depth: u32,
    /// Maximum cumulative evaluation steps allowed across an entire module.
    pub max_module_steps: u32,
    /// Maximum constructed value bytes allocated during a single trial.
    pub max_constructed_bytes: u32,
    /// Maximum IR nodes allowed in a materialized value.
    pub max_materialized_nodes: u32,
    /// Maximum additional IR nodes beyond the original call (+ removed single-use private def).
    pub max_residual_node_growth: u32,
    /// Maximum additional payload bytes beyond the original call's literals.
    pub max_residual_byte_growth: u32,
    /// Maximum total payload bytes allowed in a materialized value.
    pub max_materialized_bytes: u32,
    /// Maximum failed trial attempts for the same call site across simplification rounds.
    pub max_failed_retries: u32,
}

impl Default for EvalConfig {
    fn default() -> Self {
        EvalConfig {
            max_trial_steps: 2_000,
            max_frame_depth: 64,
            max_module_steps: 20_000,
            max_constructed_bytes: 4_096,
            max_materialized_nodes: 32,
            max_residual_node_growth: 12,
            max_residual_byte_growth: 128,
            max_materialized_bytes: 512,
            max_failed_retries: 2,
        }
    }
}

/// Reason a bounded concrete evaluation trial stopped or declined replacement.
///
/// Crucially distinct from valid Flick values like `Known(None)` or `Known(Err e)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StopReason {
    /// Unbound/dynamic local variable or mutable `Var` read.
    DynamicRead,
    /// Unsupported operation, opaque foreign call, or unresolved dictionary/method.
    UnknownOp,
    /// Non-pure stage (`Sync`/`Runtime`), effect, handler, owner boundary, or unsafe construct.
    UnsafeContext,
    /// Computation that may trap/throw/fail at runtime (`2n ** -1n`, missed pattern match).
    PossibleTrap,
    /// Per-trial step limit reached.
    StepBudget,
    /// Explicit continuation frame depth limit reached.
    FrameDepth,
    /// Cumulative module work budget exhausted.
    ModuleBudget,
    /// Constructed value byte limit reached before allocation.
    ConstructedBytes,
    /// Valid internal value cannot be materialized into standalone portable IR.
    Unmaterializable,
    /// Materialized IR exceeds node or byte growth profitability limits.
    ResidualGrowth,
    /// Call site exceeded its failed retry cap across rounds.
    RetryCap,
}

/// Result of evaluating an expression inside the bounded interpreter.
#[derive(Debug, Clone)]
pub enum EvalOutcome<'a> {
    /// Exact Flick value (including valid `None`, `Err e`, closures, dictionaries).
    Known(Value<'a>),
    /// Evaluation stopped; the runtime computation must be preserved.
    Stopped(StopReason),
}

/// Stable internal outcome counters for diagnostics/profiling (#803, #809).
/// Stores only integer counts — never collects user input strings.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EvalCounters {
    pub unknown_op: u64,
    pub unsafe_context: u64,
    pub budget: u64,
    pub growth: u64,
    pub success: u64,
    pub dynamic_read: u64,
    pub possible_trap: u64,
    pub unmaterializable: u64,
    pub retries_skipped: u64,
}

impl EvalCounters {
    pub fn record_success(&mut self) {
        self.success += 1;
    }

    pub fn record_stop(&mut self, reason: StopReason) {
        match reason {
            StopReason::UnknownOp => self.unknown_op += 1,
            StopReason::UnsafeContext => self.unsafe_context += 1,
            StopReason::StepBudget
            | StopReason::FrameDepth
            | StopReason::ModuleBudget
            | StopReason::ConstructedBytes => self.budget += 1,
            StopReason::ResidualGrowth => self.growth += 1,
            StopReason::DynamicRead => self.dynamic_read += 1,
            StopReason::PossibleTrap => self.possible_trap += 1,
            StopReason::Unmaterializable => self.unmaterializable += 1,
            StopReason::RetryCap => self.retries_skipped += 1,
        }
    }
}

/// Active work budget for a single trial, charging against both trial limits
/// and the module's remaining cumulative step allowance before allocations/scans.
#[derive(Debug, Clone)]
pub struct EvalBudget {
    pub trial_steps_remaining: u32,
    pub module_steps_remaining: u32,
    pub max_frame_depth: u32,
    pub bytes_remaining: u32,
}

impl EvalBudget {
    pub fn new(config: &EvalConfig, module_steps_remaining: u32) -> Self {
        EvalBudget {
            trial_steps_remaining: config.max_trial_steps,
            module_steps_remaining,
            max_frame_depth: config.max_frame_depth,
            bytes_remaining: config.max_constructed_bytes,
        }
    }

    /// Charges `steps` before performing work or scanning.
    #[inline(never)]
    pub fn charge_steps(&mut self, steps: u32) -> Result<(), StopReason> {
        if steps == 0 {
            return Ok(());
        }
        if steps > self.module_steps_remaining {
            self.module_steps_remaining = 0;
            return Err(StopReason::ModuleBudget);
        }
        if steps > self.trial_steps_remaining {
            self.trial_steps_remaining = 0;
            return Err(StopReason::StepBudget);
        }
        self.module_steps_remaining -= steps;
        self.trial_steps_remaining -= steps;
        Ok(())
    }

    /// Enforces frame depth *before* pushing a new continuation frame.
    #[inline(never)]
    pub fn check_depth(&self, current_depth: usize) -> Result<(), StopReason> {
        let depth = u32::try_from(current_depth).unwrap_or(u32::MAX);
        if depth >= self.max_frame_depth {
            return Err(StopReason::FrameDepth);
        }
        Ok(())
    }

    /// Charges constructed payload bytes *before* allocating a value.
    #[inline(never)]
    pub fn charge_bytes(&mut self, bytes: u32) -> Result<(), StopReason> {
        if bytes > self.bytes_remaining {
            return Err(StopReason::ConstructedBytes);
        }
        self.bytes_remaining -= bytes;
        Ok(())
    }
}

/// Outcome of asking a target primitive evaluator to fold a reviewed builtin call.
#[derive(Debug, Clone)]
pub enum PrimitiveOutcome<'a> {
    Known(Value<'a>),
    Stop(StopReason),
}

/// Target-specific hook for evaluating reviewed native/foreign primitives
/// (plugged in by #805 for JS/Go shared primitives and #831/#836 for Lua).
pub trait PrimitiveEval {
    /// Returns the expected pure argument arity if `(module, name)` is a
    /// reviewed pure primitive supported by this target.
    fn arity(&self, module: &str, name: &str, lang: &Known<'_>) -> Option<usize>;

    /// Evaluates a saturated primitive call, charging any scans/allocations
    /// against `budget` before performing them.
    fn eval<'a>(
        &self,
        module: &str,
        name: &str,
        dicts: &[DictValue<'a>],
        args: &[Value<'a>],
        lang: &Known<'_>,
        budget: &mut EvalBudget,
    ) -> PrimitiveOutcome<'a>;
}

/// Default primitive hook that declines all foreign primitives, preserving
/// opaque FFI boundaries.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoPrimitives;

impl PrimitiveEval for NoPrimitives {
    fn arity(&self, _module: &str, _name: &str, _lang: &Known<'_>) -> Option<usize> {
        None
    }

    fn eval<'a>(
        &self,
        _module: &str,
        _name: &str,
        _dicts: &[DictValue<'a>],
        _args: &[Value<'a>],
        _lang: &Known<'_>,
        _budget: &mut EvalBudget,
    ) -> PrimitiveOutcome<'a> {
        PrimitiveOutcome::Stop(StopReason::UnknownOp)
    }
}

/// Evaluates `expr` in `(env, dict_env)` under `budget` using the explicit-frame machine.
pub fn evaluate<'a>(
    expr: &'a Expr,
    defs: &'a DefTable,
    env: Env<'a>,
    dict_env: DictEnv<'a>,
    lang: &'a Known<'a>,
    primitives: &dyn PrimitiveEval,
    budget: &mut EvalBudget,
) -> EvalOutcome<'a> {
    Machine::new(defs, lang, primitives, budget).run(expr, env, dict_env)
}
