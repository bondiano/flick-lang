//! The public backend ABI is `flick:backend`, a component world. The native
//! trait uses its exact records for development and parity tests; Rust's ABI
//! is not a separately supported plugin interface.

mod driver;
mod handshake;
#[cfg(feature = "host")]
pub mod host;
mod inputs;
mod request;
mod validate;
mod wit;

pub use driver::{Failure, Generated, generate};
pub use handshake::handshake;
pub use inputs::{DeclaredInputs, Inputs, sha256};
pub use request::{validate_request, validate_source_spans};
pub use validate::{
    Limits, Policy, validate_output_paths, validate_result, validate_serialized_size,
};

/// The version of the component contract, independent of the portable IR.
pub const VERSION: &str = "0.1.0";

// Generate the native records from the WIT itself, so the two cannot drift.
#[allow(clippy::all, clippy::pedantic, missing_docs)]
#[doc(hidden)]
pub mod bindings {
    wit_bindgen::generate!({
        world: "backend",
        path: "wit",
        generate_unused_types: true,
        additional_derives: [serde::Serialize, serde::Deserialize, PartialEq, Eq],
        pub_export_macro: true,
        export_macro_name: "export_component",
        default_bindings_module: "flick_backend::bindings",
    });
}

pub use bindings::flick::backend::types::*;

/// The same operations as the component world, using the same records.
/// This trait is an implementation convenience, not a native public ABI.
pub trait Backend {
    /// The host's immutable binding to loaded bytes, checked before input reads.
    /// Native development implementations need not bind a component image.
    /// This host metadata does not add an operation to the component ABI.
    fn loaded_pin(&self) -> Option<&BackendPin> {
        None
    }
    /// Check host cancellation without entering the guest or reading inputs.
    /// Once canceled, a host binding keeps refusing later preparation.
    fn check_cancelled(&self) -> Result<(), Vec<Diagnostic>> {
        Ok(())
    }
    fn describe(&self) -> Descriptor;
    fn check_target(
        &self,
        request: &BuildRequest,
        inputs: &mut dyn Inputs,
    ) -> Result<TargetSupport, Vec<Diagnostic>>;
    fn generate(
        &self,
        request: &BuildRequest,
        inputs: &mut dyn Inputs,
    ) -> Result<BuildResult, Vec<Diagnostic>>;
}

/// Deterministic CBOR of a request, including the backend digest and all side
/// input digests. Ordered WIT lists are significant, including options.
pub fn cache_key(request: &BuildRequest) -> String {
    let mut bytes = Vec::new();
    ciborium::into_writer(request, &mut bytes).expect("a build request serializes");
    sha256(&bytes)
}
