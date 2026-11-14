//! Portable code generation helpers shared by the JS/Go emitters and frontend.
//! These Rust modules are build dependencies, not a public plugin ABI.

mod bridge;
pub mod effects;
pub mod exports;
mod lang;
pub mod loops;
pub mod names;
pub mod order;
pub mod reach;
pub mod tail;

pub use bridge::EffectBridge;
pub use lang::Known;

pub use flick_ir as ir;
pub mod consteval;
pub mod numeric;
pub mod opt;
