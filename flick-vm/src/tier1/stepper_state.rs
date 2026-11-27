//! Owned `'static` state container for [`super::stepper::SuspendedTier1`].

use std::collections::{BTreeMap, VecDeque};
use std::mem;

use flick_core::ir::memory::MemorySpan;

use crate::stdlib::primitives::IsaacRng;
use crate::stdlib::{DEFAULT_VIRTUAL_NOW_NS, FIBER_QUANTUM, HostServices};
use crate::tier0::{CauseToken, HostMode, ObjectId, Value};

use super::machine::{Frame, Machine, MainStage, OwnerEvidence, RecoveryState, RegVal};
use super::machine_coords::{CachedCellState, CoordState, DeferredState};
use super::machine_fibers::{FiberContext, FiberRecord, TestClockAdvance, TimerEntry};
use super::machine_generators::GeneratorSlot;
use super::machine_groups::FiberGroupState;
use super::machine_handlers::{
    Activation, CachedHandlerHit, CapturedSlot, InstalledOperation, Invocation, Segment,
};
use super::machine_layers::{PendingProvide, PendingProvideResult, Provider};
use super::machine_layers_build::MemoMapState;
use super::machine_scopes::{ClosingScopeState, OwnerState};
use super::machine_unwind::Discard;

pub(super) struct Tier1State {
    pub(super) frames: Vec<Frame>,
    pub(super) registers: Vec<Option<RegVal>>,
    pub(super) owners: Vec<OwnerState>,
    pub(super) current_owner: usize,
    pub(super) current_evidence: Option<OwnerEvidence>,
    pub(super) mask_depth: usize,
    pub(super) activations: BTreeMap<usize, Activation>,
    pub(super) providers: BTreeMap<usize, Provider>,
    pub(super) generators: BTreeMap<usize, GeneratorSlot>,
    pub(super) root: Option<usize>,
    pub(super) segments: BTreeMap<usize, Segment>,
    pub(super) continuations: BTreeMap<usize, CapturedSlot>,
    pub(super) invocations: BTreeMap<usize, Invocation>,
    pub(super) discard: Option<Discard>,
    pub(super) ready: Option<usize>,
    pub(super) next_handler: usize,
    pub(super) next_handler_epoch: u64,
    pub(super) handler_epoch: u64,
    pub(super) handler_cache_epoch: u64,
    pub(super) handler_cache: BTreeMap<usize, Option<CachedHandlerHit>>,
    pub(super) last_defect_span: Option<MemorySpan>,
    pub(super) scratch: Vec<Value>,
    pub(super) pending: Option<RegVal>,
    pub(super) failure: Option<CauseToken>,
    pub(super) failure_elected: bool,
    pub(super) recoveries: BTreeMap<usize, RecoveryState>,
    pub(super) next_recovery: usize,
    pub(super) pending_handler: Option<RegVal>,
    pub(super) arguments: VecDeque<Value>,
    pub(super) top: Option<RegVal>,
    pub(super) main_stage: MainStage,
    pub(super) stdout: String,
    pub(super) stderr: String,
    pub(super) program_args: Vec<String>,
    pub(super) host_services: HostServices,
    pub(super) fuel: u64,
    pub(super) host_mode: HostMode,
    pub(super) active_fiber: usize,
    pub(super) next_fiber: usize,
    pub(super) quantum: u32,
    pub(super) parked_fibers: BTreeMap<usize, FiberContext>,
    pub(super) fiber_table: BTreeMap<usize, FiberRecord>,
    pub(super) run_queue: VecDeque<usize>,
    pub(super) virtual_now_ns: u64,
    pub(super) test_clock_mode: bool,
    pub(super) test_clock_waiter: Option<TestClockAdvance>,
    pub(super) timers: BTreeMap<(u64, usize), TimerEntry>,
    pub(super) next_timer: usize,
    pub(super) deferreds: BTreeMap<usize, DeferredState>,
    pub(super) next_deferred: usize,
    pub(super) coords: BTreeMap<usize, CoordState>,
    pub(super) next_coord: usize,
    pub(super) fiber_groups: BTreeMap<usize, FiberGroupState>,
    pub(super) next_group: usize,
    pub(super) memo_maps: BTreeMap<usize, MemoMapState>,
    pub(super) next_memo: usize,
    pub(super) cached_cells: BTreeMap<usize, CachedCellState>,
    pub(super) next_cached: usize,
    pub(super) isaac_rngs: BTreeMap<usize, IsaacRng>,
    pub(super) next_rng: usize,
    pub(super) closing_scopes: BTreeMap<usize, Box<ClosingScopeState>>,
    pub(super) pending_provides: BTreeMap<usize, PendingProvide>,
    pub(super) next_provide: usize,
    pub(super) pending_resume_close: Option<usize>,
    pub(super) pending_provide_result: Option<PendingProvideResult>,
    pub(super) pending_layer_build: Option<(Value, usize, usize)>,
    pub(super) inherited_services: Vec<Value>,
    pub(super) inherited_tail_ops: Vec<InstalledOperation>,
    pub(super) lazy_waiters: BTreeMap<ObjectId, Vec<(usize, usize)>>,
    pub(super) fiber_refs: BTreeMap<String, Value>,
    pub(super) interrupt_requested: Option<u64>,
    pub(super) seeded_random: Option<usize>,
    pub(super) root_defects: Vec<CauseToken>,
    pub(super) runtime_fibers_started: u64,
}

impl Tier1State {
    pub(super) fn from_machine(m: &mut Machine<'_, '_>) -> Self {
        let mut state = Self {
            frames: Vec::new(),
            registers: Vec::new(),
            owners: Vec::new(),
            current_owner: 0,
            current_evidence: None,
            mask_depth: 0,
            activations: BTreeMap::new(),
            providers: BTreeMap::new(),
            generators: BTreeMap::new(),
            root: None,
            segments: BTreeMap::new(),
            continuations: BTreeMap::new(),
            invocations: BTreeMap::new(),
            discard: None,
            ready: None,
            next_handler: 0,
            next_handler_epoch: 1,
            handler_epoch: 0,
            handler_cache_epoch: 0,
            handler_cache: BTreeMap::new(),
            last_defect_span: None,
            scratch: Vec::new(),
            pending: None,
            failure: None,
            failure_elected: false,
            recoveries: BTreeMap::new(),
            next_recovery: 0,
            pending_handler: None,
            arguments: VecDeque::new(),
            top: None,
            main_stage: MainStage::Done,
            stdout: String::new(),
            stderr: String::new(),
            program_args: Vec::new(),
            host_services: HostServices::new(),
            fuel: 0,
            host_mode: HostMode::Deterministic,
            active_fiber: 1,
            next_fiber: 2,
            quantum: FIBER_QUANTUM,
            parked_fibers: BTreeMap::new(),
            fiber_table: BTreeMap::new(),
            run_queue: VecDeque::new(),
            virtual_now_ns: DEFAULT_VIRTUAL_NOW_NS,
            test_clock_mode: false,
            test_clock_waiter: None,
            timers: BTreeMap::new(),
            next_timer: 1,
            deferreds: BTreeMap::new(),
            next_deferred: 1,
            coords: BTreeMap::new(),
            next_coord: 1,
            fiber_groups: BTreeMap::new(),
            next_group: 1,
            memo_maps: BTreeMap::new(),
            next_memo: 1,
            cached_cells: BTreeMap::new(),
            next_cached: 1,
            isaac_rngs: BTreeMap::new(),
            next_rng: 1,
            closing_scopes: BTreeMap::new(),
            pending_provides: BTreeMap::new(),
            next_provide: 1,
            pending_resume_close: None,
            pending_provide_result: None,
            pending_layer_build: None,
            inherited_services: Vec::new(),
            inherited_tail_ops: Vec::new(),
            lazy_waiters: BTreeMap::new(),
            fiber_refs: BTreeMap::new(),
            interrupt_requested: None,
            seeded_random: None,
            root_defects: Vec::new(),
            runtime_fibers_started: 0,
        };
        state.capture(m);
        state
    }

    pub(super) fn restore(&mut self, m: &mut Machine<'_, '_>) {
        m.frames = mem::take(&mut self.frames);
        m.registers = mem::take(&mut self.registers);
        m.owners = mem::take(&mut self.owners);
        m.current_owner = self.current_owner;
        m.current_evidence = self.current_evidence;
        m.mask_depth = self.mask_depth;
        m.activations = mem::take(&mut self.activations);
        m.providers = mem::take(&mut self.providers);
        m.generators = mem::take(&mut self.generators);
        m.root = self.root;
        m.segments = mem::take(&mut self.segments);
        m.continuations = mem::take(&mut self.continuations);
        m.invocations = mem::take(&mut self.invocations);
        m.discard = self.discard.take();
        m.ready = self.ready;
        m.next_handler = self.next_handler;
        m.next_handler_epoch = self.next_handler_epoch;
        m.handler_epoch = self.handler_epoch;
        m.handler_cache_epoch = self.handler_cache_epoch;
        m.handler_cache = mem::take(&mut self.handler_cache);
        m.last_defect_span = self.last_defect_span;
        m.scratch = mem::take(&mut self.scratch);
        m.pending = self.pending.take();
        m.failure = self.failure.take();
        m.failure_elected = self.failure_elected;
        m.recoveries = mem::take(&mut self.recoveries);
        m.next_recovery = self.next_recovery;
        m.pending_handler = self.pending_handler.take();
        m.arguments = mem::take(&mut self.arguments);
        m.top = self.top.take();
        m.main_stage = self.main_stage;
        m.stdout = mem::take(&mut self.stdout);
        m.stderr = mem::take(&mut self.stderr);
        m.program_args = mem::take(&mut self.program_args);
        m.host_services = mem::take(&mut self.host_services);
        m.fuel = self.fuel;
        m.host_mode = self.host_mode;
        m.active_fiber = self.active_fiber;
        m.next_fiber = self.next_fiber;
        m.quantum = self.quantum;
        m.parked_fibers = mem::take(&mut self.parked_fibers);
        m.fiber_table = mem::take(&mut self.fiber_table);
        m.run_queue = mem::take(&mut self.run_queue);
        m.virtual_now_ns = self.virtual_now_ns;
        m.test_clock_mode = self.test_clock_mode;
        m.test_clock_waiter = self.test_clock_waiter.take();
        m.timers = mem::take(&mut self.timers);
        m.next_timer = self.next_timer;
        m.deferreds = mem::take(&mut self.deferreds);
        m.next_deferred = self.next_deferred;
        m.coords = mem::take(&mut self.coords);
        m.next_coord = self.next_coord;
        m.fiber_groups = mem::take(&mut self.fiber_groups);
        m.next_group = self.next_group;
        m.memo_maps = mem::take(&mut self.memo_maps);
        m.next_memo = self.next_memo;
        m.cached_cells = mem::take(&mut self.cached_cells);
        m.next_cached = self.next_cached;
        m.isaac_rngs = mem::take(&mut self.isaac_rngs);
        m.next_rng = self.next_rng;
        m.closing_scopes = mem::take(&mut self.closing_scopes);
        m.pending_provides = mem::take(&mut self.pending_provides);
        m.next_provide = self.next_provide;
        m.pending_resume_close = self.pending_resume_close;
        m.pending_provide_result = self.pending_provide_result.take();
        m.pending_layer_build = self.pending_layer_build.take();
        m.inherited_services = mem::take(&mut self.inherited_services);
        m.inherited_tail_ops = mem::take(&mut self.inherited_tail_ops);
        m.lazy_waiters = mem::take(&mut self.lazy_waiters);
        m.fiber_refs = mem::take(&mut self.fiber_refs);
        m.interrupt_requested = self.interrupt_requested;
        m.seeded_random = self.seeded_random;
        m.root_defects = mem::take(&mut self.root_defects);
        m.runtime_fibers_started = self.runtime_fibers_started;
    }

    pub(super) fn capture(&mut self, m: &mut Machine<'_, '_>) {
        self.frames = mem::take(&mut m.frames);
        self.registers = mem::take(&mut m.registers);
        self.owners = mem::take(&mut m.owners);
        self.current_owner = m.current_owner;
        self.current_evidence = m.current_evidence;
        self.mask_depth = m.mask_depth;
        self.activations = mem::take(&mut m.activations);
        self.providers = mem::take(&mut m.providers);
        self.generators = mem::take(&mut m.generators);
        self.root = m.root;
        self.segments = mem::take(&mut m.segments);
        self.continuations = mem::take(&mut m.continuations);
        self.invocations = mem::take(&mut m.invocations);
        self.discard = m.discard.take();
        self.ready = m.ready;
        self.next_handler = m.next_handler;
        self.next_handler_epoch = m.next_handler_epoch;
        self.handler_epoch = m.handler_epoch;
        self.handler_cache_epoch = m.handler_cache_epoch;
        self.handler_cache = mem::take(&mut m.handler_cache);
        self.last_defect_span = m.last_defect_span;
        self.scratch = mem::take(&mut m.scratch);
        self.pending = m.pending.take();
        self.failure = m.failure.take();
        self.failure_elected = m.failure_elected;
        self.recoveries = mem::take(&mut m.recoveries);
        self.next_recovery = m.next_recovery;
        self.pending_handler = m.pending_handler.take();
        self.arguments = mem::take(&mut m.arguments);
        self.top = m.top.take();
        self.main_stage = m.main_stage;
        self.stdout = mem::take(&mut m.stdout);
        self.stderr = mem::take(&mut m.stderr);
        self.program_args = mem::take(&mut m.program_args);
        self.host_services = mem::take(&mut m.host_services);
        self.fuel = m.fuel;
        self.host_mode = m.host_mode;
        self.active_fiber = m.active_fiber;
        self.next_fiber = m.next_fiber;
        self.quantum = m.quantum;
        self.parked_fibers = mem::take(&mut m.parked_fibers);
        self.fiber_table = mem::take(&mut m.fiber_table);
        self.run_queue = mem::take(&mut m.run_queue);
        self.virtual_now_ns = m.virtual_now_ns;
        self.test_clock_mode = m.test_clock_mode;
        self.test_clock_waiter = m.test_clock_waiter.take();
        self.timers = mem::take(&mut m.timers);
        self.next_timer = m.next_timer;
        self.deferreds = mem::take(&mut m.deferreds);
        self.next_deferred = m.next_deferred;
        self.coords = mem::take(&mut m.coords);
        self.next_coord = m.next_coord;
        self.fiber_groups = mem::take(&mut m.fiber_groups);
        self.next_group = m.next_group;
        self.memo_maps = mem::take(&mut m.memo_maps);
        self.next_memo = m.next_memo;
        self.cached_cells = mem::take(&mut m.cached_cells);
        self.next_cached = m.next_cached;
        self.isaac_rngs = mem::take(&mut m.isaac_rngs);
        self.next_rng = m.next_rng;
        self.closing_scopes = mem::take(&mut m.closing_scopes);
        self.pending_provides = mem::take(&mut m.pending_provides);
        self.next_provide = m.next_provide;
        self.pending_resume_close = m.pending_resume_close;
        self.pending_provide_result = m.pending_provide_result.take();
        self.pending_layer_build = m.pending_layer_build.take();
        self.inherited_services = mem::take(&mut m.inherited_services);
        self.inherited_tail_ops = mem::take(&mut m.inherited_tail_ops);
        self.lazy_waiters = mem::take(&mut m.lazy_waiters);
        self.fiber_refs = mem::take(&mut m.fiber_refs);
        self.interrupt_requested = m.interrupt_requested;
        self.seeded_random = m.seeded_random;
        self.root_defects = mem::take(&mut m.root_defects);
        self.runtime_fibers_started = m.runtime_fibers_started;
    }
}
