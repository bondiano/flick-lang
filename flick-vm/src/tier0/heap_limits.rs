// Counted heap live-byte accounting and `heap_bytes` limit enforcement.

/// Deterministic per-object base overhead (bytes) shared identically across
/// native 64-bit and `wasm32` hosts so heap limits are platform-independent.
const OBJECT_HEADER_BYTES: usize = 64;

/// Deterministic inline cost per owned `Value` slot inside a container object.
const VALUE_SLOT_BYTES: usize = 16;

fn value_inline_bytes(value: &Value) -> usize {
    let mut total = 0usize;
    let mut stack = vec![value];
    while let Some(current) = stack.pop() {
        total = total.saturating_add(VALUE_SLOT_BYTES);
        if let Value::Aggregate { tag, fields, .. } = current {
            total = total.saturating_add(tag.len());
            stack.extend(fields.iter());
        }
    }
    total
}

fn data_byte_size(data: &Data) -> usize {
    let payload = match data {
        Data::String(s) | Data::BigInt(s) | Data::Label(s) | Data::ServiceTag { key: s } => s.len(),
        Data::Utf16(units) => units.len().saturating_mul(2),
        Data::Bytes(buf) => buf.len(),
        Data::Tagged { tag, fields, .. } => {
            tag.len().saturating_add(fields.iter().map(value_inline_bytes).sum())
        }
        Data::Record(fields) => fields
            .iter()
            .map(|(k, v)| k.len().saturating_add(value_inline_bytes(v)))
            .sum(),
        Data::Dictionary(entries) => entries
            .iter()
            .map(|(k, v)| k.len().saturating_add(value_inline_bytes(v.value())))
            .sum(),
        Data::Closure { captures, .. }
        | Data::Task { captures, .. }
        | Data::Seq(SeqData::Generator { captures, .. }) => {
            captures.iter().map(value_inline_bytes).sum()
        }
        Data::Arena { contents, .. } => contents.iter().map(value_inline_bytes).sum(),
        Data::Context(entries) => entries
            .iter()
            .map(|(k, v)| k.len().saturating_add(value_inline_bytes(v)))
            .sum(),
        Data::CapturedEnv { services, refs } => {
            let svc: usize = services.iter().map(value_inline_bytes).sum();
            let rfs: usize = refs
                .iter()
                .map(|(k, v)| k.len().saturating_add(value_inline_bytes(v)))
                .sum();
            svc.saturating_add(rfs)
        }
        Data::Cause(cause) => cause.reasons.len().saturating_mul(24),
        Data::BoxedAggregate { value, .. } => value_inline_bytes(value),
        Data::RegExp { pattern, flags, .. } => pattern
            .len()
            .saturating_add(flags.len())
            .saturating_add(256),
        Data::BigDecimal { value, .. } => {
            usize::try_from(value.bits() / 8).unwrap_or(usize::MAX).saturating_add(16)
        }
        Data::Graph(g) => g
            .nodes
            .len()
            .saturating_mul(24)
            .saturating_add(g.edges.len().saturating_mul(32)),
        Data::GraphWalker(w) => w
            .indices
            .len()
            .saturating_add(w.values.len())
            .saturating_mul(VALUE_SLOT_BYTES),
        Data::Layer(LayerData::MergeAll(layers)) => {
            layers.iter().map(value_inline_bytes).sum()
        }
        _ => 0,
    };
    OBJECT_HEADER_BYTES.saturating_add(payload)
}

impl Heap {
    /// Returns the currently live accounted byte count across all occupied slots.
    #[must_use]
    pub const fn live_bytes(&self) -> usize {
        self.live_bytes
    }

    /// Returns the peak live accounted byte count observed on this heap.
    #[must_use]
    pub const fn peak_live_bytes(&self) -> usize {
        self.peak_live_bytes
    }

    /// Returns the active heap-byte ceiling, if configured.
    #[must_use]
    pub const fn byte_limit(&self) -> Option<usize> {
        self.byte_limit
    }

    /// Configures or clears the live heap-byte ceiling.
    pub fn set_byte_limit(&mut self, limit: Option<usize>) {
        self.byte_limit = limit;
        if let Some(cap) = limit {
            self.byte_limit_exceeded = self.live_bytes > cap;
        } else {
            self.byte_limit_exceeded = false;
        }
    }

    /// Returns whether any allocation has crossed [`Self::byte_limit`].
    #[must_use]
    pub const fn is_byte_limit_exceeded(&self) -> bool {
        self.byte_limit_exceeded
    }

    /// Clears the latched byte-limit-exceeded flag (used while entering finalizer unwind).
    pub(crate) fn clear_byte_limit_exceeded(&mut self) {
        self.byte_limit_exceeded = false;
    }

    /// Returns `Err(Error::LimitExceeded(LimitKind::HeapMemory))` if the active
    /// heap-byte ceiling has been exceeded.
    pub fn check_byte_limit(&self) -> Result<(), Error> {
        if self.byte_limit_exceeded
            || self.byte_limit.is_some_and(|cap| self.live_bytes > cap)
        {
            return Err(Error::LimitExceeded(LimitKind::HeapMemory));
        }
        Ok(())
    }

    /// Checks whether adding `extra_bytes` would exceed [`Self::byte_limit`].
    pub(crate) fn would_exceed_byte_limit(&self, extra_bytes: usize) -> bool {
        self.byte_limit
            .is_some_and(|cap| self.live_bytes.saturating_add(extra_bytes) > cap)
    }

    /// Allocates `data` if it fits within [`Self::byte_limit`]; otherwise
    /// cleanly releases all child tokens owned by `data` and returns
    /// `Error::LimitExceeded(LimitKind::HeapMemory)` with zero leaks.
    pub(crate) fn try_allocate(&mut self, data: Data) -> Result<Value, Error> {
        let bytes = data_byte_size(&data);
        if self.would_exceed_byte_limit(bytes) {
            self.byte_limit_exceeded = true;
            self.release_unallocated_data(data)?;
            return Err(Error::LimitExceeded(LimitKind::HeapMemory));
        }
        Ok(self.allocate(data))
    }

    fn release_unallocated_data(&mut self, data: Data) -> Result<(), Error> {
        let mut pending = Vec::new();
        match data {
            Data::Cell { contents, .. } => pending.extend(contents),
            Data::Arena { contents, .. } => pending.extend(contents),
            Data::Continuation { .. }
            | Data::ForeignHandle { .. }
            | Data::ForeignCycle { .. }
            | Data::ServiceTag { .. } => {}
            other => release_data_children(other, &mut pending),
        }
        self.drop_values(pending)
    }

    /// Replaces `object.data` in place while keeping `live_bytes` exact.
    pub(crate) fn replace_object_data(
        &mut self,
        id: ObjectId,
        new_data: Data,
    ) -> Result<Data, Error> {
        let new_bytes = data_byte_size(&new_data);
        let object = self.object_mut(id)?;
        let old_bytes = object.bytes;
        object.bytes = new_bytes;
        let old = mem::replace(&mut object.data, new_data);
        self.live_bytes = self
            .live_bytes
            .saturating_sub(old_bytes)
            .saturating_add(new_bytes);
        self.peak_live_bytes = self.peak_live_bytes.max(self.live_bytes);
        if let Some(cap) = self.byte_limit
            && self.live_bytes > cap
        {
            self.byte_limit_exceeded = true;
        }
        Ok(old)
    }

    /// Recomputes and updates the tracked byte size of an object mutated in place.
    pub(crate) fn refresh_object_bytes(&mut self, id: ObjectId) -> Result<(), Error> {
        let object = self.object_mut(id)?;
        let old_bytes = object.bytes;
        let new_bytes = data_byte_size(&object.data);
        object.bytes = new_bytes;
        self.live_bytes = self
            .live_bytes
            .saturating_sub(old_bytes)
            .saturating_add(new_bytes);
        self.peak_live_bytes = self.peak_live_bytes.max(self.live_bytes);
        if let Some(cap) = self.byte_limit
            && self.live_bytes > cap
        {
            self.byte_limit_exceeded = true;
        }
        Ok(())
    }
}
