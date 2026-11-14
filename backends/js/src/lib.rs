//! JavaScript and TypeScript emission without a checker or project driver.
//! Component integration uses the separate `flick:backend` contract.

pub mod codegen;

pub mod numeric;

pub use flick_backend_support::effects;
pub use flick_ir as ir;
