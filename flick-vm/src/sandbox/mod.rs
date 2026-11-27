//! Capability sandboxing, resource limits, and bytecode/image verification for
//! `flick-vm`.
//!
//! ## Security boundary and native/wasm32 parity contract
//!
//! - **Verifier is the security boundary**: `#988`'s `compiler_hash` and
//!   `memory_ir_hash` header digests are *compatibility* checks only (preventing
//!   accidental cross-version loading), NOT a security boundary. Untrusted
//!   images and tier-1 bytecode must pass [`verify_image`] / [`verify_image_bytes`]
//!   and [`verify_bytecode`] before loading or execution.
//! - **Future native regions**: Any future native/JIT compilation tier must
//!   compile exclusively from [`verify_bytecode`]-verified bytecode and cap
//!   native code arenas at [`NATIVE_REGION_ARENA_LIMIT`] (64 MiB).
//! - **Wasm32 parity**: Everything in `flick_vm::sandbox` compiles cleanly
//!   without OS-only assumptions. Wall-time budgets accept a pluggable
//!   [`WallClock`] (defaulting to `std::time::Instant` on native targets and an
//!   embedder-supplied monotonic clock such as `performance.now()` on `wasm32`),
//!   so `flick-vm-wasm` (`#997`) enforces the exact same capabilities, limits,
//!   and verifier checks as native `flick-vm`.

pub mod bytecode_fuzz;
pub mod capabilities;
pub mod limits;
pub mod verify_bytecode;
mod verify_bytecode_ctrl;
mod verify_bytecode_instr;
mod verify_cbor;
pub mod verify_image;

use std::{error, fmt};

use flick_core::ir::memory::MemorySpan;

pub use capabilities::{
    Capability, CapabilitySet, check_program_capabilities, required_capability_for_effect,
    required_capability_for_foreign_call, required_capability_for_host_primitive,
    required_capability_for_host_service,
};
#[cfg(not(target_arch = "wasm32"))]
pub use limits::SystemWallClock;
pub use limits::{
    LimitKind, MockWallClock, ResourceLimits, SandboxConfig, WallClock, WallDeadline,
    default_wall_clock,
};
pub use verify_bytecode::{BytecodeVerifyError, verify_bytecode};
pub use verify_image::{
    ImageVerifyError, MAX_CBOR_DEPTH, MAX_IMAGE_BYTES, verify_cbor_budget, verify_image,
    verify_image_bytes,
};

/// Maximum executable native-region code arena size (64 MiB).
///
/// Future native/JIT tiers compile only from [`verify_bytecode`]-verified
/// bytecode and must not exceed this arena allocation cap per session/region.
pub const NATIVE_REGION_ARENA_LIMIT: usize = 64 * 1024 * 1024;

/// Structured error raised when a sandbox policy or verifier rejects a program,
/// bytecode image, or VM image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SandboxError {
    /// A linked or executed program required a capability not granted by the
    /// active [`CapabilitySet`].
    CapabilityDenied {
        capability: Capability,
        binding: String,
        span: Option<MemorySpan>,
    },
    /// Tier-1 register bytecode failed structural verification.
    BytecodeVerification(BytecodeVerifyError),
    /// A `VmImage` failed structural or security verification.
    ImageVerification(ImageVerifyError),
}

impl SandboxError {
    /// Returns the source span associated with a capability denial, if known.
    #[must_use]
    pub const fn span(&self) -> Option<MemorySpan> {
        match self {
            Self::CapabilityDenied { span, .. } => *span,
            Self::BytecodeVerification(_) | Self::ImageVerification(_) => None,
        }
    }
}

impl fmt::Display for SandboxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CapabilityDenied {
                capability,
                binding,
                span: Some(span),
            } => write!(
                f,
                "capability `{capability}` denied for `{binding}` (file {}, bytes {}..{})",
                span.file, span.start, span.end
            ),
            Self::CapabilityDenied {
                capability,
                binding,
                span: None,
            } => write!(f, "capability `{capability}` denied for `{binding}`"),
            Self::BytecodeVerification(err) => write!(f, "bytecode verification failed: {err}"),
            Self::ImageVerification(err) => write!(f, "image verification failed: {err}"),
        }
    }
}

impl error::Error for SandboxError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Self::CapabilityDenied { .. } => None,
            Self::BytecodeVerification(err) => Some(err),
            Self::ImageVerification(err) => Some(err),
        }
    }
}

impl From<BytecodeVerifyError> for SandboxError {
    fn from(err: BytecodeVerifyError) -> Self {
        Self::BytecodeVerification(err)
    }
}

impl From<ImageVerifyError> for SandboxError {
    fn from(err: ImageVerifyError) -> Self {
        Self::ImageVerification(err)
    }
}
