use std::mem;

use wasmtime::{Error, ResourceLimiter, StoreLimits, StoreLimitsBuilder};

/// One budget shared by all linear memories and table slots of a store.
/// Wasmtime's engine/JIT and embedder allocations have separate accounting.
pub struct MemoryLimits {
    items: StoreLimits,
    remaining: usize,
}

pub fn memory_limits(bytes: usize) -> MemoryLimits {
    MemoryLimits {
        items: StoreLimitsBuilder::new()
            .memory_size(bytes)
            .trap_on_grow_failure(true)
            .build(),
        remaining: bytes,
    }
}

impl MemoryLimits {
    fn reserve(&mut self, bytes: usize) -> wasmtime::Result<()> {
        self.remaining = self
            .remaining
            .checked_sub(bytes)
            .ok_or_else(|| Error::msg("component memory budget exceeded"))?;
        Ok(())
    }
}

impl ResourceLimiter for MemoryLimits {
    fn memory_growing(
        &mut self,
        current: usize,
        desired: usize,
        maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        let allowed = self.items.memory_growing(current, desired, maximum)?;
        if allowed {
            self.reserve(desired.saturating_sub(current))?;
        }
        Ok(allowed)
    }

    fn memory_grow_failed(&mut self, error: Error) -> wasmtime::Result<()> {
        // Growth failures trap and end the call, so a failed reservation
        // cannot be reused by deliberately requesting an invalid growth.
        self.items.memory_grow_failed(error)
    }

    fn table_growing(
        &mut self,
        current: usize,
        desired: usize,
        maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        let allowed = self.items.table_growing(current, desired, maximum)?;
        if allowed {
            let bytes = desired
                .saturating_sub(current)
                .checked_mul(mem::size_of::<usize>())
                .ok_or_else(|| Error::msg("component table size overflow"))?;
            self.reserve(bytes)?;
        }
        Ok(allowed)
    }

    fn table_grow_failed(&mut self, error: Error) -> wasmtime::Result<()> {
        self.items.table_grow_failed(error)
    }
    fn instances(&self) -> usize {
        self.items.instances()
    }
    fn memories(&self) -> usize {
        self.items.memories()
    }
    fn tables(&self) -> usize {
        self.items.tables()
    }
}
