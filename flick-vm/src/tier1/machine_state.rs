//! Frame, register-value, global, loop, recovery, and continuation-end types for tier 1.

use crate::tier0::{CauseToken, Value};

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

pub(super) struct RecoveryState {
    pub(super) original: CauseToken,
    pub(super) dst: usize,
    pub(super) owner: usize,
}

pub(super) enum End {
    Top,
    Store(usize),
    Global {
        id: usize,
        dst: usize,
    },
    Loop(LoopState),
    FunctionBoundary {
        dst: usize,
    },
    Try {
        selector: RegVal,
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
    CloseScope {
        owner: usize,
        previous: usize,
        dst: usize,
    },
    ClosingScope {
        state: Box<ClosingScopeState>,
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
        next: Box<End>,
    },
    DiscardCleanupDone {
        saved_mask: usize,
        masked: bool,
        paused: Box<Discard>,
    },
    DiscardFinalizer,
}

pub(super) struct Frame {
    pub(super) code: usize,
    pub(super) pc: usize,
    pub(super) base: usize,
    pub(super) vars: Vec<Value>,
    pub(super) recovery_id: Option<usize>,
    pub(super) end: End,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum MainStage {
    Function,
    Done,
}
