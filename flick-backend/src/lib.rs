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
        generate_all,
        additional_derives: [serde::Serialize, serde::Deserialize, PartialEq, Eq],
        pub_export_macro: true,
        export_macro_name: "export_component",
        default_bindings_module: "flick_backend::bindings",
    });
}

pub use bindings::flick::backend::types::*;

/// The records of the command interface a backend or a plugin implements.
pub mod command {
    pub use crate::bindings::flick::command::types::*;

    impl Plan {
        /// A plan that does nothing.
        pub fn empty() -> Self {
            Self {
                files: vec![],
                steps: vec![],
                diagnostics: vec![],
                stdout: vec![],
                resume: None,
                needs: vec![],
            }
        }
    }

    impl Diagnostic {
        /// An error of the contributor, `code` without its `FLICK-` prefix.
        pub fn error(code: &str, message: String) -> Self {
            Self {
                code: code.into(),
                severity: Severity::Error,
                message,
                notes: vec![],
                span: None,
                help: None,
            }
        }
    }

    /// The answer of a contributor that has no command `name`.
    pub fn unknown(name: &str) -> Vec<Diagnostic> {
        vec![Diagnostic::error(
            "P052",
            format!("this contributor has no command `{name}`"),
        )]
    }
}

/// Exports the command interface of a component through the [`Backend`]
/// value `$backend`, beside the component's other exports.
#[macro_export]
macro_rules! export_commands {
    ($component:ty, $backend:expr) => {
        impl $crate::bindings::exports::flick::command::command::Guest for $component {
            fn describe() -> ::std::vec::Vec<$crate::command::Descriptor> {
                $crate::Backend::commands(&$backend)
            }
            fn run(
                invocation: $crate::command::Invocation,
            ) -> ::std::result::Result<
                $crate::command::Plan,
                ::std::vec::Vec<$crate::command::Diagnostic>,
            > {
                $crate::Backend::run(&$backend, &invocation)
            }
            fn resume(
                state: ::std::vec::Vec<u8>,
                outputs: ::std::vec::Vec<$crate::command::File>,
                answers: ::std::vec::Vec<$crate::command::Answer>,
            ) -> ::std::result::Result<
                $crate::command::Plan,
                ::std::vec::Vec<$crate::command::Diagnostic>,
            > {
                $crate::Backend::resume(&$backend, state, outputs, answers)
            }
            fn open(
                invocation: $crate::command::Invocation,
            ) -> ::std::result::Result<
                $crate::command::Opened,
                ::std::vec::Vec<$crate::command::Diagnostic>,
            > {
                $crate::Backend::open(&$backend, &invocation)
            }
            fn handle(
                state: ::std::vec::Vec<u8>,
                request: ::std::vec::Vec<u8>,
                answers: ::std::vec::Vec<$crate::command::Answer>,
            ) -> ::std::result::Result<$crate::command::Handled, ::std::string::String> {
                $crate::Backend::handle(&$backend, state, request, answers)
            }
            fn close(state: ::std::vec::Vec<u8>) -> $crate::command::Plan {
                $crate::Backend::close(&$backend, state)
            }
        }
    };
}

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
    /// The files of a project the backend reads to report its packages.
    fn package_files(&self) -> Vec<String> {
        Vec::new()
    }
    /// The backend's packages, from the files `package_files` names that
    /// the project has and those an earlier answer needed.
    fn packages(
        &self,
        files: &[ProjectFile],
        dependencies: &[command::Dependency],
        options: &[OptionValue],
    ) -> Packages {
        let _ = (files, dependencies, options);
        Packages {
            name: None,
            sources: Vec::new(),
            runtime: Vec::new(),
            diagnostics: Vec::new(),
            needs: Vec::new(),
        }
    }
    /// The commands the backend adds to `flick`.
    fn commands(&self) -> Vec<command::Descriptor> {
        Vec::new()
    }
    /// The plan of the plan command `invocation.name`.
    fn run(
        &self,
        invocation: &command::Invocation,
    ) -> Result<command::Plan, Vec<command::Diagnostic>> {
        Err(command::unknown(&invocation.name))
    }
    /// The rest of a plan whose steps have run, `outputs` holding their files.
    fn resume(
        &self,
        state: Vec<u8>,
        outputs: Vec<command::File>,
        answers: Vec<command::Answer>,
    ) -> Result<command::Plan, Vec<command::Diagnostic>> {
        let _ = (state, outputs, answers);
        Err(vec![command::Diagnostic::error(
            "P055",
            "this contributor resumes no plan".into(),
        )])
    }
    /// The state of a new session of the session command `invocation.name`.
    fn open(
        &self,
        invocation: &command::Invocation,
    ) -> Result<command::Opened, Vec<command::Diagnostic>> {
        Err(command::unknown(&invocation.name))
    }
    /// The answer to one request of a session, `answers` those of the needs
    /// its last call for this request returned.
    fn handle(
        &self,
        state: Vec<u8>,
        request: Vec<u8>,
        answers: Vec<command::Answer>,
    ) -> Result<command::Handled, String> {
        let _ = (state, request, answers);
        Err("this contributor has no session command".into())
    }
    /// The plan of a session that ends.
    fn close(&self, state: Vec<u8>) -> command::Plan {
        let _ = state;
        command::Plan::empty()
    }
}

/// Deterministic CBOR of a request, including the backend digest and all side
/// input digests. Ordered WIT lists are significant, including options.
pub fn cache_key(request: &BuildRequest) -> String {
    let mut bytes = Vec::new();
    ciborium::into_writer(request, &mut bytes).expect("a build request serializes");
    sha256(&bytes)
}
