//! Bounded concrete evaluator over portable Flick IR (FL-OPT-01, #803).
//!
//! Evaluates closed pure calls in release builds using an explicit-frame step
//! loop with deterministic budgets (steps, frame depth, cumulative module work,
//! constructed bytes, residual node growth, and bounded failed retries across
//! rounds). Evaluation (`Known(value)` vs `Stopped(reason)`), fallible
//! materialization into IR, and call-site profitability are kept distinct.

mod machine;
pub mod materialize;
mod pass;
mod pattern;
pub mod value;

#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet, VecDeque};

use flick_ir::{Dict, Expr, ExprKind, ItemKind, Module};

use super::{Generics, devirt};
use crate::Known;
use machine::Machine;
pub use pass::{RetryTracker, run_module_pass};
pub use value::{DictEnv, DictValue, Env, Value};

/// Maximum number of external definitions pre-loaded into a [`DefTable`] per round.
const MAX_EXTERNAL_DEFS: usize = 256;

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
    pub fn check_depth(&self, current_depth: usize) -> Result<(), StopReason> {
        let depth = u32::try_from(current_depth).unwrap_or(u32::MAX);
        if depth >= self.max_frame_depth {
            return Err(StopReason::FrameDepth);
        }
        Ok(())
    }

    /// Charges constructed payload bytes *before* allocating a value.
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

/// Immutable snapshot of pure definitions and instance methods available to
/// the evaluator during an optimization round.
#[derive(Debug, Default)]
pub struct DefTable {
    values: HashMap<(String, String), (Vec<String>, Expr)>,
    methods: HashMap<(String, String, String), (Vec<String>, Expr)>,
}

struct RefQueues {
    values: VecDeque<(String, String)>,
    methods: VecDeque<(String, String, String)>,
    ext_instances: HashSet<(String, String)>,
    polymorphic_methods: HashSet<String>,
}

impl DefTable {
    /// Builds a [`DefTable`] for `module` by snapshotting own items and
    /// transitively pre-loading reachable external pure definitions via `generics`
    /// and `lookup` (bounded by [`MAX_EXTERNAL_DEFS`]).
    pub fn build(
        module: &Module,
        lookup: &devirt::Lookup<'_>,
        generics: Generics<'_>,
        _lang: &Known<'_>,
    ) -> Self {
        let mut table = DefTable::default();
        let mut queues = RefQueues {
            values: VecDeque::new(),
            methods: VecDeque::new(),
            ext_instances: HashSet::new(),
            polymorphic_methods: HashSet::new(),
        };
        let mut seen_values: HashSet<(String, String)> = HashSet::new();
        let mut seen_methods: HashSet<(String, String, String)> = HashSet::new();

        for item in &module.items {
            match &item.kind {
                ItemKind::Value { name, dicts, body } => {
                    let key = (module.name.clone(), name.clone());
                    seen_values.insert(key.clone());
                    enqueue_refs(body, &module.name, &mut queues);
                    table.values.insert(key, (dicts.clone(), body.clone()));
                }
                ItemKind::Destructure { body, .. } => {
                    enqueue_refs(body, &module.name, &mut queues);
                }
                ItemKind::Instance { info, methods } => {
                    for (m, code) in methods {
                        let key = (module.name.clone(), info.name.clone(), m.clone());
                        seen_methods.insert(key.clone());
                        enqueue_refs(code, &module.name, &mut queues);
                        table
                            .methods
                            .insert(key, (info.context.clone(), code.clone()));
                    }
                    for (m, n, _) in &info.defaults {
                        if *n == 0 {
                            let key = (module.name.clone(), info.name.clone(), m.clone());
                            if seen_methods.insert(key.clone())
                                && let Some(found) = lookup.raw_method(&module.name, &info.name, m)
                            {
                                enqueue_refs(&found.1, &module.name, &mut queues);
                                table.methods.insert(key, found);
                            }
                        }
                    }
                }
                ItemKind::Foreign(_) | ItemKind::Constructor(_) | ItemKind::Service { .. } => {}
            }
        }

        let mut external_loaded = 0;
        let mut expanded_poly = false;
        while external_loaded < MAX_EXTERNAL_DEFS {
            if let Some((owner, name)) = queues.values.pop_front() {
                let key = (owner, name);
                if !seen_values.insert(key.clone()) {
                    continue;
                }
                if let Some((dicts, body)) = generics(&key.0, &key.1) {
                    enqueue_refs(&body, &module.name, &mut queues);
                    table.values.insert(key, (dicts, body));
                    external_loaded += 1;
                }
                continue;
            }
            if let Some((owner, instance, method)) = queues.methods.pop_front() {
                let key = (owner, instance, method);
                if !seen_methods.insert(key.clone()) {
                    continue;
                }
                if let Some((context, body)) = lookup.raw_method(&key.0, &key.1, &key.2) {
                    enqueue_refs(&body, &module.name, &mut queues);
                    table.methods.insert(key, (context, body));
                    external_loaded += 1;
                }
                continue;
            }
            if !expanded_poly {
                expanded_poly = true;
                for (owner, instance) in &queues.ext_instances {
                    for method in &queues.polymorphic_methods {
                        queues
                            .methods
                            .push_back((owner.clone(), instance.clone(), method.clone()));
                    }
                }
                if !queues.methods.is_empty() {
                    continue;
                }
            }
            break;
        }
        table
    }

    /// Inserts a single value definition (useful for unit tests).
    pub fn insert_value(&mut self, module: String, name: String, dicts: Vec<String>, body: Expr) {
        self.values.insert((module, name), (dicts, body));
    }

    /// Inserts a single instance method definition (useful for unit tests).
    pub fn insert_method(
        &mut self,
        module: String,
        instance: String,
        method: String,
        context: Vec<String>,
        body: Expr,
    ) {
        self.methods
            .insert((module, instance, method), (context, body));
    }

    pub(super) fn value(&self, module: &str, name: &str) -> Option<(&[String], &Expr)> {
        self.values
            .get(&(module.to_owned(), name.to_owned()))
            .map(|(d, b)| (d.as_slice(), b))
    }

    pub(super) fn method(
        &self,
        module: &str,
        instance: &str,
        method: &str,
    ) -> Option<(&[String], &Expr)> {
        self.methods
            .get(&(module.to_owned(), instance.to_owned(), method.to_owned()))
            .map(|(c, b)| (c.as_slice(), b))
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

fn enqueue_refs(root: &Expr, current_module: &str, queues: &mut RefQueues) {
    let mut stack = vec![root];
    while let Some(e) = stack.pop() {
        match &e.kind {
            ExprKind::Global { module, name } => {
                queues.values.push_back((module.clone(), name.clone()));
            }
            ExprKind::Method { dict, name } => {
                enqueue_dict_methods(dict, name, current_module, queues);
            }
            ExprKind::Dicts(_, dicts) => {
                for dict in dicts {
                    collect_external_instances(dict, current_module, &mut queues.ext_instances);
                }
            }
            _ => {}
        }
        e.each_child(&mut |child| stack.push(child));
    }
}

fn enqueue_dict_methods(dict: &Dict, method: &str, current_module: &str, queues: &mut RefQueues) {
    let mut stack = vec![dict];
    while let Some(d) = stack.pop() {
        match d {
            Dict::Instance { module, name, args } => {
                queues
                    .methods
                    .push_back((module.clone(), name.clone(), method.to_owned()));
                for arg in args {
                    collect_external_instances(arg, current_module, &mut queues.ext_instances);
                    stack.push(arg);
                }
            }
            Dict::Super { dict, .. } => stack.push(dict),
            Dict::Structural { fields, .. } => {
                for field in fields {
                    stack.push(field);
                }
            }
            Dict::Given(_) | Dict::Field(_) => {
                queues.polymorphic_methods.insert(method.to_owned());
            }
            Dict::Value { .. }
            | Dict::Opened { .. }
            | Dict::Primitive { .. }
            | Dict::Site { .. }
            | Dict::Error => {}
        }
    }
}

fn collect_external_instances(
    dict: &Dict,
    current_module: &str,
    out: &mut HashSet<(String, String)>,
) {
    let mut stack = vec![dict];
    while let Some(d) = stack.pop() {
        match d {
            Dict::Instance { module, name, args } => {
                if module != current_module {
                    out.insert((module.clone(), name.clone()));
                }
                for arg in args {
                    stack.push(arg);
                }
            }
            Dict::Super { dict, .. } => stack.push(dict),
            Dict::Structural { fields, .. } => {
                for field in fields {
                    stack.push(field);
                }
            }
            Dict::Given(_)
            | Dict::Value { .. }
            | Dict::Opened { .. }
            | Dict::Field(_)
            | Dict::Primitive { .. }
            | Dict::Site { .. }
            | Dict::Error => {}
        }
    }
}
