//! Tier-neutral Stage 4 cooperative runtime primitives and helpers:
//! `Context.*` maps, `Layer.*` constructors/lookup, virtual clock constants,
//! and value decoding helpers shared across VM tiers.

pub(crate) mod context;
pub(crate) mod coop;
pub(crate) mod layers;

pub use coop::{DEFAULT_VIRTUAL_NOW_NS, FIBER_QUANTUM, QUANTUM_VIRTUAL_NS};
