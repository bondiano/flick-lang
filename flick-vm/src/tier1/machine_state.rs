//! Frame, register-value, global, loop, recovery, owner-evidence, and end types for tier 1.

use std::collections::BTreeMap;

use crate::tier0::{CauseToken, CellHandle, LazyHandle, ObjectId, OwnerHandle, Value};

use super::machine_layers::LayerBuildTarget;
use super::machine_scopes::{BracketOutcome, ClosingScopeState};
use super::machine_unwind::Discard;

/// Compact register value: captureless closures stay unallocated in registers
/// until they escape to a heap container, primitive, or top-level return.
#[derive(Debug)]
pub(crate) enum RegVal {
    Value(Value),
    StaticClosure(usize),
}

pub(super) enum GlobalState {
    Empty,
    Initializing,
    Ready(RegVal),
}

pub(super) struct LoopState {
    pub(super) id: usize,
    pub(super) code: usize,
    pub(super) args: Vec<RegVal>,
    pub(super) dst: usize,
}

/// Copyable lexical owner evidence metadata, never a copyable ownership token.
#[derive(Clone, Copy)]
pub(super) struct OwnerEvidence {
    pub(super) heap: usize,
    pub(super) execution: usize,
    pub(super) index: usize,
}

impl OwnerEvidence {
    pub(super) fn from_owner(owner: &OwnerHandle) -> Self {
        Self {
            heap: owner.heap,
            execution: owner.execution,
            index: owner.index,
        }
    }

    pub(super) fn value(self) -> Value {
        Value::Owner(OwnerHandle {
            heap: self.heap,
            execution: self.execution,
            index: self.index,
        })
    }
}

pub(super) struct RecoveryState {
    pub(super) original: CauseToken,
    pub(super) dst: usize,
    pub(super) owner: usize,
    pub(super) evidence: Option<OwnerEvidence>,
}

pub(super) enum End {
    Top,
    Store(usize),
    Global {
        id: usize,
        dst: usize,
    },
    Modify {
        cell: CellHandle,
        dst: usize,
    },
    Loop(LoopState),
    FunctionBoundary {
        dst: usize,
    },
    GenerateStrict {
        items: Vec<Value>,
        remaining: Option<usize>,
        dst: usize,
    },
    GeneratorStep {
        gen_id: usize,
        dst: usize,
    },
    LazyComplete {
        lazy: LazyHandle,
        dst: usize,
    },
    Try {
        selector: RegVal,
        dst: usize,
    },
    CaptureExit {
        dst: usize,
    },
    CatchDefect {
        handler: RegVal,
        dst: usize,
    },
    RecoverySelection {
        id: usize,
    },
    RecoveryHelper {
        id: usize,
        dst: usize,
    },
    RecoveryBody {
        dst: usize,
        owner: usize,
        evidence: Option<OwnerEvidence>,
    },
    Handle {
        activation: usize,
        dst: usize,
    },
    Discontinue {
        cont_id: usize,
        dst: usize,
    },
    Invocation(usize),
    LayerRun {
        provider: usize,
        dst: usize,
    },
    CloseScope {
        owner: usize,
        previous: usize,
        dst: Option<usize>,
    },
    ClosingScope {
        state: Box<ClosingScopeState>,
    },
    RestoreOwner {
        previous: usize,
        dst: usize,
    },
    AcquireRegister {
        owner: usize,
        release: RegVal,
        dst: usize,
    },
    BracketUse {
        use_fn: RegVal,
        release_fn: RegVal,
        dst: usize,
    },
    BracketRelease {
        resource: Value,
        release_fn: RegVal,
        dst: usize,
    },
    BracketFinish {
        outcome: BracketOutcome,
        saved_mask: usize,
        dst: usize,
    },
    RestoreMask {
        saved_mask: usize,
        check_on_exit: bool,
        next: Box<End>,
    },
    DiscardCleanupDone {
        saved_mask: usize,
        masked: bool,
        paused: Box<Discard>,
    },
    OnExitDiscardRunFin {
        saved_mask: usize,
        masked: bool,
        paused: Box<Discard>,
    },
    EnsuringFinish {
        finalizer: RegVal,
        dst: usize,
    },
    EnsuringDone {
        outcome: BracketOutcome,
        saved_mask: usize,
        dst: usize,
    },
    OnExitDecide {
        interruptible: bool,
        decide: RegVal,
        dst: usize,
    },
    OnExitRunFin {
        outcome: BracketOutcome,
        saved_mask: usize,
        dst: usize,
    },
    OnExitFinish {
        outcome: BracketOutcome,
        saved_mask: usize,
        dst: usize,
    },
    CatchCause {
        handler: RegVal,
        dst: usize,
    },
    OrDie {
        dst: usize,
    },
    TimedFinish {
        start_ns: u64,
        dst: usize,
    },
    CachedComplete {
        cell_id: usize,
        next: Box<End>,
    },
    ProvideContext {
        context: Value,
        dst: usize,
    },
    WithServices {
        services: Vec<Value>,
        next: Box<End>,
    },
    RestoreFiberRef {
        key: String,
        previous: Option<Value>,
        dst: usize,
    },
    RestoreOwnerEvidence {
        previous: usize,
        evidence: Option<OwnerEvidence>,
        dst: usize,
    },
    RestoreEnvRefs {
        previous: BTreeMap<String, Value>,
        dst: usize,
    },
    RestoreRandom {
        previous: Option<usize>,
        rng_id: usize,
        dst: usize,
    },
    LayerBuildFinish {
        inner_owner: usize,
        parent_scope: usize,
        prev_owner: usize,
        prev_ev: Option<OwnerEvidence>,
        prev_memo: Option<Value>,
        memo_entry: Option<(usize, ObjectId)>,
        or_die: bool,
        target: LayerBuildTarget,
    },
    DiscardFinalizer,
}

pub(super) struct Frame {
    pub(super) code: usize,
    pub(super) pc: usize,
    pub(super) base: usize,
    pub(super) vars: Vec<Value>,
    pub(super) iterators: Vec<Value>,
    pub(super) recovery_id: Option<usize>,
    pub(super) evidence: Option<OwnerEvidence>,
    pub(super) end: End,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum MainStage {
    Root { main: usize },
    Function,
    Task,
    Done,
}
