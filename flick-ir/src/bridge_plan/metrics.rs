//! Per-thread test instrumentation; never part of the metadata or public API.
use std::cell::Cell;

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Counts {
    pub merge: usize,
    pub retained: usize,
    pub lookup: usize,
    pub ancestor: usize,
    pub kind_enumerated: usize,
    pub kind_checks: usize,
    pub range: usize,
    pub memo_entries: usize,
    pub memo_lookups: usize,
}
thread_local! {
    static COUNTS: Cell<Counts> = Cell::new(Counts::default());
}
pub(super) fn reset() {
    COUNTS.set(Counts::default());
}
pub(super) fn counts() -> Counts {
    COUNTS.get()
}
pub(super) fn merge() {
    COUNTS.with(|cell| {
        let mut counts = cell.get();
        counts.merge += 1;
        cell.set(counts);
    });
}
pub(super) fn retain(cells: usize) {
    COUNTS.with(|cell| {
        let mut counts = cell.get();
        counts.retained += cells;
        cell.set(counts);
    });
}
pub(super) fn lookup() {
    COUNTS.with(|cell| {
        let mut counts = cell.get();
        counts.lookup += 1;
        cell.set(counts);
    });
}

pub(super) fn ancestor() {
    COUNTS.with(|cell| {
        let mut counts = cell.get();
        counts.ancestor += 1;
        cell.set(counts);
    });
}

pub(super) fn kind_enumeration() {
    COUNTS.with(|cell| {
        let mut counts = cell.get();
        counts.kind_enumerated += 1;
        cell.set(counts);
    });
}
pub(super) fn kind_check() {
    COUNTS.with(|cell| {
        let mut counts = cell.get();
        counts.kind_checks += 1;
        cell.set(counts);
    });
}

pub(super) fn range_step() {
    COUNTS.with(|cell| {
        let mut counts = cell.get();
        counts.range += 1;
        cell.set(counts);
    });
}

pub(super) fn memo_lookup() {
    COUNTS.with(|cell| {
        let mut counts = cell.get();
        counts.memo_lookups += 1;
        cell.set(counts);
    });
}
pub(super) fn memo_entry() {
    COUNTS.with(|cell| {
        let mut counts = cell.get();
        counts.memo_entries += 1;
        counts.retained += 1;
        cell.set(counts);
    });
}
