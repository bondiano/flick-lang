//! Native transport of the component ABI through Wasmtime. These helpers
//! also serve the existing language-plugin host.

mod memory;
pub mod runtime;

pub mod command;
mod component;
pub use component::{Budget, WasmBackend, bindings};
