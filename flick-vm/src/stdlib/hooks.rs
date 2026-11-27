//! Caller-supplied fuel, allocation, and host environment hooks for the
//! tier-neutral stdlib interface.

use crate::tier0::{Error, Heap};

use super::host::{HostMode, HostServices};
use super::primitives::IsaacRng;

/// Fuel and allocation accounting hook supplied by any VM tier calling into
/// [`crate::stdlib`].
///
/// Tier 0 implements this directly over its `&mut u64` fuel counter so fuel and
/// allocation counts stay bit-for-bit identical. Higher tiers (bytecode VM, JIT)
/// supply their own metering or budget hooks against the same primitive library.
pub trait Accounting {
    /// Charges `work` units of execution fuel before performing metered work.
    /// Exhaustion must return [`Error::OutOfFuel`] without over-consuming.
    fn charge_fuel(&mut self, work: usize) -> Result<(), Error>;

    /// Notified after a primitive call when the [`Heap`] observed new
    /// allocations or frees during that call.
    fn record_allocation_delta(&mut self, _allocations: usize, _frees: usize) {}
}

/// Host environment and I/O hook supplied by the calling VM tier.
pub trait HostEnv {
    /// Selects deterministic vs live OS environment behavior.
    fn host_mode(&self) -> HostMode {
        HostMode::Deterministic
    }

    /// Appends text to the execution's standard output stream.
    fn write_stdout(&mut self, text: &str);

    /// Appends text to the execution's standard error stream.
    /// Defaults to a no-op for deterministic model runs.
    fn write_stderr(&mut self, _text: &str) {}

    /// Program CLI arguments forwarded after `--`.
    fn program_args(&self) -> &[String] {
        &[]
    }

    /// Mutable access to live OS host service resources (files, temp paths,
    /// child processes, monotonic start, OS PRNG).
    fn host_services(&mut self) -> Option<&mut HostServices> {
        None
    }

    /// Optional virtual wall/monotonic clock time in nanoseconds (`Clock`/`TestClock`).
    /// Returning `None` falls back to [`HostMode`]'s fixed deterministic clock
    /// (or real OS time in [`HostMode::LiveOs`]).
    fn virtual_now_ns(&self) -> Option<u64> {
        None
    }

    /// Optional next draw in `[0, 1)` from an active seeded RNG (`Random.with_seed`).
    /// Returning `None` falls back to [`super::DRAW`] (or live OS randomness in
    /// [`HostMode::LiveOs`]).
    fn next_seeded_random(&mut self) -> Option<f64> {
        None
    }
}

/// Combined accounting and host environment hook required by host/IO intrinsics.
pub trait VmHooks: Accounting + HostEnv {}

impl<T: Accounting + HostEnv + ?Sized> VmHooks for T {}

/// Internal short alias for trait-object fuel accounting inside `stdlib`.
pub(crate) type Fuel<'a> = &'a mut dyn Accounting;

/// Charges `amount` units of fuel through the caller's [`Accounting`] hook.
pub(crate) fn charge(hooks: Fuel<'_>, amount: usize) -> Result<(), Error> {
    hooks.charge_fuel(amount)
}

/// Runs `f` while reporting any [`Heap`] allocation/free deltas to `hooks`.
pub(crate) fn with_alloc_accounting<R>(
    heap: &mut Heap,
    hooks: Fuel<'_>,
    f: impl FnOnce(&mut Heap, Fuel<'_>) -> Result<R, Error>,
) -> Result<R, Error> {
    let before = heap.stats();
    let result = f(heap, hooks);
    let after = heap.stats();
    let alloc_delta = after.allocations.saturating_sub(before.allocations);
    let free_delta = after.frees.saturating_sub(before.frees);
    if alloc_delta > 0 || free_delta > 0 {
        hooks.record_allocation_delta(alloc_delta, free_delta);
    }
    result
}

/// Exact `u64` fuel counter adapter used by `tier0`'s program executor.
pub struct FuelCounter<'a>(pub &'a mut u64);

impl Accounting for FuelCounter<'_> {
    fn charge_fuel(&mut self, work: usize) -> Result<(), Error> {
        let amount = u64::try_from(work).map_err(|_| Error::OutOfFuel)?;
        *self.0 = self.0.checked_sub(amount).ok_or(Error::OutOfFuel)?;
        Ok(())
    }
}

/// Unmetered accounting adapter for standalone `tier0::execute(Function)` and
/// unfueled unit probes.
pub struct Unmetered;

impl Accounting for Unmetered {
    fn charge_fuel(&mut self, _work: usize) -> Result<(), Error> {
        Ok(())
    }
}

/// Diagnostic accounting adapter that records cumulative fuel charged and
/// heap allocation/free deltas while delegating to a `u64` fuel counter.
pub struct CountingHooks<'a> {
    fuel: &'a mut u64,
    pub fuel_charged: u64,
    pub allocations_observed: usize,
    pub frees_observed: usize,
}

impl<'a> CountingHooks<'a> {
    pub fn new(fuel: &'a mut u64) -> Self {
        Self {
            fuel,
            fuel_charged: 0,
            allocations_observed: 0,
            frees_observed: 0,
        }
    }
}

impl Accounting for CountingHooks<'_> {
    fn charge_fuel(&mut self, work: usize) -> Result<(), Error> {
        let amount = u64::try_from(work).map_err(|_| Error::OutOfFuel)?;
        *self.fuel = self.fuel.checked_sub(amount).ok_or(Error::OutOfFuel)?;
        self.fuel_charged = self.fuel_charged.saturating_add(amount);
        Ok(())
    }

    fn record_allocation_delta(&mut self, allocations: usize, frees: usize) {
        self.allocations_observed += allocations;
        self.frees_observed += frees;
    }
}

/// Default [`VmHooks`] implementation bundling a fuel counter, [`HostMode`],
/// stdout/stderr buffers, CLI program arguments, optional virtual clock,
/// optional seeded RNG, and optional live [`HostServices`].
pub struct HostContext<'a> {
    fuel: &'a mut u64,
    host_mode: HostMode,
    stdout: &'a mut String,
    stderr: Option<&'a mut String>,
    program_args: &'a [String],
    virtual_now_ns: Option<u64>,
    seeded_rng: Option<&'a mut IsaacRng>,
    host_services: Option<&'a mut HostServices>,
}

impl<'a> HostContext<'a> {
    pub fn new(fuel: &'a mut u64, host_mode: HostMode, stdout: &'a mut String) -> Self {
        Self {
            fuel,
            host_mode,
            stdout,
            stderr: None,
            program_args: &[],
            virtual_now_ns: None,
            seeded_rng: None,
            host_services: None,
        }
    }

    #[must_use]
    pub fn with_stderr(mut self, stderr: &'a mut String) -> Self {
        self.stderr = Some(stderr);
        self
    }

    #[must_use]
    pub fn with_program_args(mut self, args: &'a [String]) -> Self {
        self.program_args = args;
        self
    }

    #[must_use]
    pub fn with_host_services(mut self, services: &'a mut HostServices) -> Self {
        self.host_services = Some(services);
        self
    }

    #[must_use]
    pub fn with_virtual_now_ns(mut self, now_ns: u64) -> Self {
        self.virtual_now_ns = Some(now_ns);
        self
    }

    pub(crate) fn with_seeded_rng(mut self, rng: &'a mut IsaacRng) -> Self {
        self.seeded_rng = Some(rng);
        self
    }
}

impl Accounting for HostContext<'_> {
    fn charge_fuel(&mut self, work: usize) -> Result<(), Error> {
        FuelCounter(self.fuel).charge_fuel(work)
    }
}

impl HostEnv for HostContext<'_> {
    fn host_mode(&self) -> HostMode {
        self.host_mode
    }

    fn write_stdout(&mut self, text: &str) {
        self.stdout.push_str(text);
    }

    fn write_stderr(&mut self, text: &str) {
        if let Some(stderr) = self.stderr.as_deref_mut() {
            stderr.push_str(text);
        }
    }

    fn program_args(&self) -> &[String] {
        self.program_args
    }

    fn host_services(&mut self) -> Option<&mut HostServices> {
        self.host_services.as_deref_mut()
    }

    fn virtual_now_ns(&self) -> Option<u64> {
        self.virtual_now_ns
    }

    fn next_seeded_random(&mut self) -> Option<f64> {
        self.seeded_rng.as_deref_mut().map(IsaacRng::next_f64)
    }
}
