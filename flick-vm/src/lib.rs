//! The Flick virtual machine for the explicit ownership IR (`flick-ir/src/memory/`).
//!
//! - [`stdlib`] is the tier-neutral standard primitive library, Stage 4
//!   cooperative runtime helpers, Stage 5 host modes and `HostBridge`, and the
//!   caller-supplied fuel/allocation accounting hooks shared by all VM tiers.
//! - [`tier0`] is the reference counted heap and executor specified in
//!   `docs/formal/memory.md`, executing `dup`/`drop`/`reuse` directly without
//!   relying on Rust `Rc` or a tracing collector, and delegating all primitive
//!   and host operations to [`stdlib`].

#![allow(
    clippy::many_single_char_names,
    reason = "primitive and VM algorithms use concise mathematical names matching the formal specs"
)]

pub mod image;
pub mod sandbox;
pub mod session;
pub mod stdlib;
pub mod tier0;
pub mod tier1;
