//! Self-framing binary VM images (`.fvm`), per-module save/load, and live
//! module unloading.
//!
//! A [`VmImage`] captures a selected set of modules (or an entire live
//! [`crate::session::VmSession`]): closed type interfaces, memory-IR slices,
//! optional tier-1 prepared bytecode metadata, global table entries, and a
//! sharing- and cycle-preserving snapshot of heap values reachable from those
//! globals.

pub(crate) mod format;
mod heap_restore;
mod heap_restore_extra;
mod heap_snap;
mod heap_snap_extra;
pub(crate) mod heap_types;
mod heap_validate;
mod load;
mod save;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_modules;
mod unload;

use std::collections::BTreeMap;
use std::{error, fmt};

use flick_core::iface::{IFACE_VERSION, Interface};
use flick_core::ir::memory::{CodeId, Name, Program};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

pub use format::{IMAGE_FORMAT_VERSION, IMAGE_MAGIC};
pub use load::{
    LoadReport, execute_image_main_with_args, execute_image_program, load_image_into_session,
};
pub use save::{SaveSelection, build_static_image, save_session_image, save_session_modules};
pub use unload::{
    HeapInventory, UnloadReport, reclaim_unloaded, session_heap_inventory, unload_module,
};

use heap_types::HeapSnapshot;

use crate::sandbox::SandboxError;

/// Exact 32-byte SHA-256 digest used for compiler and memory-IR compatibility checks.
pub type ImageHash = [u8; 32];

/// Computes the exact compiler compatibility hash for this build.
///
/// Images built by a different compiler version, interface schema, or image
/// format version fail to load without attempting migration.
///
/// **Note**: This hash check is a *compatibility* check only, NOT a security
/// boundary. Untrusted images are validated by [`crate::sandbox::verify_image`]
/// and [`crate::sandbox::verify_bytecode`] before loading or execution.
#[must_use]
pub fn compiler_hash() -> ImageHash {
    let mut hasher = Sha256::new();
    hasher.update(b"flick-vm-compiler-v1\0");
    hasher.update(flick_core::VERSION.as_bytes());
    hasher.update(b"\0");
    hasher.update(IFACE_VERSION.to_le_bytes());
    hasher.update(IMAGE_FORMAT_VERSION.to_le_bytes());
    if let Some(rev) = option_env!("FLICK_COMPILER_REV") {
        hasher.update(rev.as_bytes());
    }
    hasher.finalize().into()
}

/// Computes the exact memory-IR and heap-snapshot schema hash for this build.
#[must_use]
pub fn memory_ir_hash() -> ImageHash {
    let mut hasher = Sha256::new();
    hasher.update(b"flick-vm-memory-ir-v1\0");
    // Schema tags for memory-IR operations, intrinsics, value reps, and heap snapshot v1.
    hasher.update(b"memory-ir:Program(functions,globals,op_spans)\0");
    hasher.update(b"ops:Literal,Construct,ConstructWithDicts,Project,Record,Field,Match,Update,Closure,Task,Await,Apply,Call,Within,ReadOwnerEvidence,WithOwnerEvidence,Global,Binary,Not,Neg,Intrinsic,DropReuse,Reuse,ReuseWithDicts,ReuseRecord,BoxAggregate,UnboxBorrow,UnboxConsume,Handle,Perform,ServiceTag,Dictionary,Method,Structure,RecoveryResult,VarMake,VarGet,VarSet,Loop,LoopExit,FunctionBoundary,UnwrapOption,GenerateStrict,GenerateLazy,Yield,SeqOpen,SeqNext,Label,ForeignCall\0");
    hasher.update(b"heap-snap:v1\0");
    hasher.update(IFACE_VERSION.to_le_bytes());
    hasher.finalize().into()
}

/// Formats an [`ImageHash`] as 64 lowercase hexadecimal characters.
#[must_use]
pub fn format_hash(hash: &ImageHash) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(64);
    for byte in hash {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Per-module metadata stored in a [`VmImage`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleRecord {
    pub name: String,
    pub interface: Interface,
    /// Fingerprints of imported external modules checked at load time.
    #[serde(default)]
    pub import_fingerprints: BTreeMap<String, String>,
}

/// Optional tier-1 preparation metadata bundled when the entry is tier-1-preparable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreparedMetadata {
    pub entry: Name,
    pub tier1_version: u32,
    pub functions: usize,
    pub instructions: usize,
    pub fused_compute: usize,
    pub fused_compare_branch: usize,
    pub fused_update_cell: usize,
    pub fused_match_flag: usize,
    pub fused_tail_calls: usize,
    pub memory_ir_digest: ImageHash,
}

/// Global binding entry stored in a [`VmImage`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlobalEntry {
    pub name: Name,
    pub initializer: CodeId,
    /// Present when the global was already initialized in the saved VM state.
    pub value: Option<heap_types::SnapValue>,
}

/// Manifest section describing the image's entry point, modules, and code ownership.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ImageManifest {
    pub(crate) entry: Option<Name>,
    pub(crate) root_layer: Option<Name>,
    pub(crate) modules: Vec<String>,
    /// Closed interfaces of external modules imported by modules in this image.
    pub(crate) external_imports: BTreeMap<String, Interface>,
    /// Owning module name for each `CodeId` in `program`.
    pub(crate) code_owners: BTreeMap<CodeId, String>,
}

/// Decoded or freshly built VM module image.
#[derive(Debug, Clone)]
pub struct VmImage {
    pub(crate) compiler_hash: ImageHash,
    pub(crate) memory_ir_hash: ImageHash,
    pub(crate) manifest: ImageManifest,
    pub(crate) interfaces: Vec<ModuleRecord>,
    pub(crate) program: Program,
    pub(crate) prepared: Option<PreparedMetadata>,
    pub(crate) globals: Vec<GlobalEntry>,
    pub(crate) heap: HeapSnapshot,
}

impl VmImage {
    /// Returns the compiler hash recorded in this image.
    #[must_use]
    pub const fn compiler_hash(&self) -> &ImageHash {
        &self.compiler_hash
    }

    /// Returns the memory-IR hash recorded in this image.
    #[must_use]
    pub const fn memory_ir_hash(&self) -> &ImageHash {
        &self.memory_ir_hash
    }

    /// Returns the optional runnable entry global (e.g. `App.Main.main`).
    #[must_use]
    pub fn entry(&self) -> Option<&Name> {
        self.manifest.entry.as_ref()
    }

    /// Returns the optional root layer global (e.g. `Host.live`).
    #[must_use]
    pub fn root_layer(&self) -> Option<&Name> {
        self.manifest.root_layer.as_ref()
    }

    /// Returns the ordered list of module names contained in this image.
    #[must_use]
    pub fn modules(&self) -> &[String] {
        &self.manifest.modules
    }

    /// Returns the closed module interfaces stored in this image.
    #[must_use]
    pub fn interfaces(&self) -> &[ModuleRecord] {
        &self.interfaces
    }

    /// Returns the memory-IR program slice stored in this image.
    #[must_use]
    pub const fn program(&self) -> &Program {
        &self.program
    }

    /// Returns optional tier-1 prepared bytecode metadata if present.
    #[must_use]
    pub fn prepared(&self) -> Option<&PreparedMetadata> {
        self.prepared.as_ref()
    }

    /// Returns whether any global in this image carries a snapshotted initial value.
    #[must_use]
    pub fn has_initialized_globals(&self) -> bool {
        self.globals.iter().any(|g| g.value.is_some())
    }

    /// Serializes this image into its self-framing `FLKM` binary container.
    pub fn encode(&self) -> Result<Vec<u8>, ImageError> {
        format::encode(self)
    }

    /// Decodes and structurally validates a `FLKM` binary image.
    pub fn decode(bytes: &[u8]) -> Result<Self, ImageError> {
        format::decode(bytes)
    }

    /// Returns a copy with overridden header hashes (for negative compatibility testing).
    #[must_use]
    pub fn with_hashes(mut self, compiler: ImageHash, memory_ir: ImageHash) -> Self {
        self.compiler_hash = compiler;
        self.memory_ir_hash = memory_ir;
        self
    }
}

/// Structured errors returned by VM image save, encode, decode, load, and unload operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageError {
    /// Image bytes ended before the header, section directory, or a section payload completed.
    Truncated(String),
    /// Header magic, format version, section directory, CBOR payload, or graph indices are invalid.
    Corrupt(String),
    /// Image was built by a different compiler revision.
    CompilerHashMismatch {
        expected: ImageHash,
        found: ImageHash,
    },
    /// Image was built with a different memory-IR revision.
    MemoryIrHashMismatch {
        expected: ImageHash,
        found: ImageHash,
    },
    /// A value reachable from a saved global holds an unserializable host resource or transient handle.
    UnserializableHostResource {
        global: Name,
        path: String,
        kind: String,
    },
    /// A module interface is open (contains solver-local variables or error markers).
    OpenInterface { module: String, detail: String },
    /// A requested module to save or unload is not present in the session.
    UnknownModule(String),
    /// A required imported module is not loaded in the target VM.
    MissingDependency { module: String, dependency: String },
    /// An imported module is loaded in the target VM, but its interface differs from what the image expects.
    InterfaceMismatch {
        dependency: String,
        expected: String,
        found: String,
    },
    /// A module in the image is already loaded in the target VM.
    ModuleAlreadyLoaded(String),
    /// A sandbox policy check (such as capability denial) rejected the image.
    Sandbox(SandboxError),
    /// Underlying memory-IR validation or counted-heap error.
    Vm(String),
}

impl fmt::Display for ImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated(detail) => write!(f, "truncated VM image: {detail}"),
            Self::Corrupt(detail) => write!(f, "corrupt VM image: {detail}"),
            Self::CompilerHashMismatch { expected, found } => write!(
                f,
                "VM image compiler hash mismatch: expected {}, found {}",
                format_hash(expected),
                format_hash(found)
            ),
            Self::MemoryIrHashMismatch { expected, found } => write!(
                f,
                "VM image memory-IR hash mismatch: expected {}, found {}",
                format_hash(expected),
                format_hash(found)
            ),
            Self::UnserializableHostResource { global, path, kind } => write!(
                f,
                "cannot serialize host resource ({kind}) reachable from global {}.{} at `{path}`",
                global.module, global.name
            ),
            Self::OpenInterface { module, detail } => {
                write!(
                    f,
                    "cannot serialize open interface for `{module}`: {detail}"
                )
            }
            Self::UnknownModule(module) => write!(f, "module `{module}` is not loaded in the VM"),
            Self::MissingDependency { module, dependency } => write!(
                f,
                "cannot load module `{module}`: required dependency `{dependency}` is not loaded in the VM"
            ),
            Self::InterfaceMismatch {
                dependency,
                expected,
                found,
            } => write!(
                f,
                "dependency `{dependency}` interface mismatch: expected fingerprint {expected}, found {found}"
            ),
            Self::ModuleAlreadyLoaded(module) => {
                write!(f, "module `{module}` is already loaded in the VM")
            }
            Self::Sandbox(err) => write!(f, "VM image sandbox error: {err}"),
            Self::Vm(detail) => write!(f, "VM image error: {detail}"),
        }
    }
}

impl From<SandboxError> for ImageError {
    fn from(err: SandboxError) -> Self {
        Self::Sandbox(err)
    }
}

impl error::Error for ImageError {}
