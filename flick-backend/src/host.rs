//! Native transport of the component ABI through Wasmtime. These helpers
//! also serve the existing language-plugin host.

mod memory;
pub mod runtime;

mod component;
pub use component::{Budget, WasmBackend};
