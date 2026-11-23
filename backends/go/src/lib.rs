//! The official Go backend: a whole program as one Go module per entry,
//! with the runtime package beside it, which the host's `go build` makes
//! an executable of.

pub mod adapter;
pub mod codegen;

#[cfg(feature = "component")]
mod component;
#[cfg(feature = "component")]
pub use component::Go;
