//! The Flick virtual machine for the explicit ownership IR (`flick-ir/src/memory/`).
//!
//! `tier0` is the reference counted heap and executor specified in
//! `docs/formal/memory.md`, executing `dup`/`drop`/`reuse` directly without
//! relying on Rust `Rc` or a tracing collector.

#![allow(
    clippy::many_single_char_names,
    reason = "primitive and VM algorithms use concise mathematical names matching the formal specs"
)]

pub mod tier0;
