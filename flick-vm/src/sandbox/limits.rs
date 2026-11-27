//! Host-supplied execution resource limits and pluggable wall-clock abstraction.
//!
//! Follows the budget / reservation / cancellation design from
//! `flick-backend/src/host/{memory,component}.rs`: limits are owned by the host
//! (never by guest code), checked before over-committing allocations or frames,
//! and reported as typed [`LimitKind`] outcomes after LIFO scope finalization.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

use super::capabilities::CapabilitySet;

/// Typed resource limit exhausted by a sandboxed VM execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LimitKind {
    /// Execution step / primitive fuel budget exhausted.
    Fuel,
    /// Counted heap live-byte cap exceeded.
    HeapMemory,
    /// Wall-clock deadline elapsed.
    WallTime,
    /// Maximum call-frame stack depth exceeded.
    StackDepth,
}

impl LimitKind {
    /// Canonical lowercase identifier for diagnostics and telemetry.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fuel => "fuel",
            Self::HeapMemory => "heap_memory",
            Self::WallTime => "wall_time",
            Self::StackDepth => "stack_depth",
        }
    }
}

impl fmt::Display for LimitKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Fuel => f.write_str("fuel limit exceeded"),
            Self::HeapMemory => f.write_str("heap memory limit exceeded"),
            Self::WallTime => f.write_str("wall-time limit exceeded"),
            Self::StackDepth => f.write_str("maximum stack depth exceeded"),
        }
    }
}

/// Monotonic millisecond clock used to enforce [`ResourceLimits::wall_time_ms`].
///
/// Native targets default to [`SystemWallClock`] (`std::time::Instant`);
/// `wasm32` embedders (`#997` `flick-vm-wasm`) provide a host monotonic clock
/// (e.g. `performance.now()`) via [`ResourceLimits::with_clock`] so wall-time
/// limits behave identically across native and browser environments.
pub trait WallClock: fmt::Debug + Send + Sync {
    /// Returns a monotonically non-decreasing timestamp in milliseconds.
    fn now_ms(&self) -> u64;
}

/// Native monotonic clock backed by `std::time::Instant`.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug)]
pub struct SystemWallClock {
    origin: Instant,
}

#[cfg(not(target_arch = "wasm32"))]
impl Default for SystemWallClock {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl SystemWallClock {
    /// Creates a monotonic clock anchored at the current instant.
    #[must_use]
    pub fn new() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl WallClock for SystemWallClock {
    fn now_ms(&self) -> u64 {
        u64::try_from(self.origin.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
}

/// Deterministic atomic millisecond clock for tests and wasm32 hosts.
#[derive(Debug)]
pub struct MockWallClock {
    now_ms: AtomicU64,
}

impl MockWallClock {
    /// Creates a mock clock initialized to `initial_ms`.
    #[must_use]
    pub const fn new(initial_ms: u64) -> Self {
        Self {
            now_ms: AtomicU64::new(initial_ms),
        }
    }

    /// Advances the clock by `delta_ms` milliseconds.
    pub fn advance(&self, delta_ms: u64) {
        self.now_ms.fetch_add(delta_ms, Ordering::Relaxed);
    }

    /// Sets the clock directly to `ms`.
    pub fn set(&self, ms: u64) {
        self.now_ms.store(ms, Ordering::Relaxed);
    }
}

impl WallClock for MockWallClock {
    fn now_ms(&self) -> u64 {
        self.now_ms.load(Ordering::Relaxed)
    }
}

/// Returns the default platform [`WallClock`] (`SystemWallClock` on native
/// targets, zero-anchored [`MockWallClock`] on `wasm32` when no host clock is
/// supplied).
#[must_use]
pub fn default_wall_clock() -> Arc<dyn WallClock> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        Arc::new(SystemWallClock::new())
    }
    #[cfg(target_arch = "wasm32")]
    {
        Arc::new(MockWallClock::new(0))
    }
}

/// Active wall-clock deadline computed at the start of one VM execution.
#[derive(Clone, Debug)]
pub struct WallDeadline {
    clock: Arc<dyn WallClock>,
    deadline_ms: u64,
}

impl WallDeadline {
    /// Starts a deadline expiring after `budget_ms` on `clock`.
    #[must_use]
    pub fn start(clock: Arc<dyn WallClock>, budget_ms: u64) -> Self {
        let start_ms = clock.now_ms();
        Self {
            clock,
            deadline_ms: start_ms.saturating_add(budget_ms),
        }
    }

    /// Returns `true` once the clock reaches or passes the deadline.
    #[must_use]
    pub fn is_expired(&self) -> bool {
        self.clock.now_ms() >= self.deadline_ms
    }
}

/// Resource limits enforced on a sandboxed VM execution or session.
#[derive(Clone, Debug)]
pub struct ResourceLimits {
    /// Maximum execution steps / primitive fuel units per evaluation.
    pub fuel: Option<u64>,
    /// Maximum live counted-heap bytes permitted at any instant.
    pub heap_bytes: Option<usize>,
    /// Maximum elapsed wall-clock milliseconds per evaluation.
    pub wall_time_ms: Option<u64>,
    /// Maximum call-frame stack depth permitted on frame push.
    pub max_stack_depth: Option<usize>,
    clock: Option<Arc<dyn WallClock>>,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self::unlimited()
    }
}

impl PartialEq for ResourceLimits {
    fn eq(&self, other: &Self) -> bool {
        self.fuel == other.fuel
            && self.heap_bytes == other.heap_bytes
            && self.wall_time_ms == other.wall_time_ms
            && self.max_stack_depth == other.max_stack_depth
    }
}

impl Eq for ResourceLimits {}

impl ResourceLimits {
    /// No resource limits active (default for trusted interactive sessions).
    #[must_use]
    pub const fn unlimited() -> Self {
        Self {
            fuel: None,
            heap_bytes: None,
            wall_time_ms: None,
            max_stack_depth: None,
            clock: None,
        }
    }

    /// Conservative defaults suitable for running untrusted code snippets.
    #[must_use]
    pub const fn sandbox_defaults() -> Self {
        Self {
            fuel: Some(1_000_000),
            heap_bytes: Some(16 * 1024 * 1024),
            wall_time_ms: Some(5_000),
            max_stack_depth: Some(512),
            clock: None,
        }
    }

    /// Sets the per-evaluation fuel budget.
    #[must_use]
    pub const fn with_fuel(mut self, fuel: u64) -> Self {
        self.fuel = Some(fuel);
        self
    }

    /// Sets the live counted-heap byte ceiling.
    #[must_use]
    pub const fn with_heap_bytes(mut self, bytes: usize) -> Self {
        self.heap_bytes = Some(bytes);
        self
    }

    /// Sets the wall-clock deadline in milliseconds.
    #[must_use]
    pub const fn with_wall_time_ms(mut self, ms: u64) -> Self {
        self.wall_time_ms = Some(ms);
        self
    }

    /// Sets the maximum call-frame depth.
    #[must_use]
    pub const fn with_max_stack_depth(mut self, depth: usize) -> Self {
        self.max_stack_depth = Some(depth);
        self
    }

    /// Installs a custom [`WallClock`] for [`Self::wall_time_ms`] checks.
    #[must_use]
    pub fn with_clock(mut self, clock: Arc<dyn WallClock>) -> Self {
        self.clock = Some(clock);
        self
    }

    /// Returns `true` when all four resource limits are `None`.
    #[must_use]
    pub const fn is_unlimited(&self) -> bool {
        self.fuel.is_none()
            && self.heap_bytes.is_none()
            && self.wall_time_ms.is_none()
            && self.max_stack_depth.is_none()
    }

    /// Starts an active [`WallDeadline`] if [`Self::wall_time_ms`] is set.
    #[must_use]
    pub fn start_wall_deadline(&self) -> Option<WallDeadline> {
        let budget_ms = self.wall_time_ms?;
        let clock = self
            .clock
            .as_ref()
            .map_or_else(default_wall_clock, Arc::clone);
        Some(WallDeadline::start(clock, budget_ms))
    }
}

/// Complete sandbox policy combining capability grants and resource limits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SandboxConfig {
    pub capabilities: CapabilitySet,
    pub limits: ResourceLimits,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self::untrusted()
    }
}

impl SandboxConfig {
    /// Default untrusted sandbox: pure + console only, no host capabilities.
    #[must_use]
    pub const fn untrusted() -> Self {
        Self {
            capabilities: CapabilitySet::default_untrusted(),
            limits: ResourceLimits::unlimited(),
        }
    }

    /// Unrestricted configuration granting all capabilities and no limits.
    #[must_use]
    pub const fn unrestricted() -> Self {
        Self {
            capabilities: CapabilitySet::all(),
            limits: ResourceLimits::unlimited(),
        }
    }

    /// Replaces the active capability set.
    #[must_use]
    pub const fn with_capabilities(mut self, capabilities: CapabilitySet) -> Self {
        self.capabilities = capabilities;
        self
    }

    /// Grants an individual capability.
    #[must_use]
    pub const fn allow(mut self, cap: super::capabilities::Capability) -> Self {
        self.capabilities = self.capabilities.allow(cap);
        self
    }

    /// Denies an individual capability.
    #[must_use]
    pub const fn deny(mut self, cap: super::capabilities::Capability) -> Self {
        self.capabilities = self.capabilities.deny(cap);
        self
    }

    /// Replaces the active resource limits.
    #[must_use]
    pub fn with_limits(mut self, limits: ResourceLimits) -> Self {
        self.limits = limits;
        self
    }

    /// Sets the fuel budget.
    #[must_use]
    pub fn with_fuel(mut self, fuel: u64) -> Self {
        self.limits.fuel = Some(fuel);
        self
    }

    /// Sets the live heap-byte cap.
    #[must_use]
    pub fn with_heap_bytes(mut self, bytes: usize) -> Self {
        self.limits.heap_bytes = Some(bytes);
        self
    }

    /// Sets the wall-clock deadline in milliseconds.
    #[must_use]
    pub fn with_wall_time_ms(mut self, ms: u64) -> Self {
        self.limits.wall_time_ms = Some(ms);
        self
    }

    /// Sets the maximum stack-frame depth.
    #[must_use]
    pub fn with_max_stack_depth(mut self, depth: usize) -> Self {
        self.limits.max_stack_depth = Some(depth);
        self
    }

    /// Installs a custom [`WallClock`] for wall-time checks.
    #[must_use]
    pub fn with_clock(mut self, clock: Arc<dyn WallClock>) -> Self {
        self.limits = self.limits.with_clock(clock);
        self
    }
}
