//! Security verifier for `#988` [`VmImage`] binaries and decoded images.
//!
//! ## Security boundary
//!
//! `#988`'s `compiler_hash` and `memory_ir_hash` header digests are
//! **compatibility checks only** — they guard against accidental cross-version
//! mismatch, NOT malicious or corrupted input. This verifier is the security
//! boundary for loading untrusted `.fvm` images:
//! - enforces CBOR container nesting depth and item-count budgets before
//!   deserialization via [`verify_cbor_budget`]
//! - validates header, section bounds, manifest/interface consistency,
//!   memory-IR function bodies, all `CodeId`/global indices and call arities,
//!   `op_spans` bounds, heap snapshot indices/tags/field counts/acyclicity,
//!   and embedded tier-1 prepared bytecode metadata when present
//! - future native regions compile only from [`super::verify_bytecode`]-verified
//!   bytecode under [`super::NATIVE_REGION_ARENA_LIMIT`] (64 MiB).

use std::collections::{BTreeMap, BTreeSet};
use std::{error, fmt};

use flick_core::ir::memory::{self, Block, CodeId, Exit, Function, Instruction, Name, Operation};

use super::verify_bytecode::verify_bytecode;
pub use super::verify_cbor::{MAX_CBOR_DEPTH, verify_cbor_budget};
use crate::image::{ImageError, VmImage};
use crate::tier1;

/// Maximum permitted raw `.fvm` container size (64 MiB).
pub const MAX_IMAGE_BYTES: usize = 64 * 1024 * 1024;

/// Supported tier-1 embedded metadata version.
const TIER1_METADATA_VERSION: u32 = 1;

/// Structured error produced by [`verify_image`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageVerifyError {
    pub section: &'static str,
    pub detail: String,
}

impl ImageVerifyError {
    fn new(section: &'static str, detail: impl Into<String>) -> Self {
        Self {
            section,
            detail: detail.into(),
        }
    }
}

impl fmt::Display for ImageVerifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.section, self.detail)
    }
}

impl error::Error for ImageVerifyError {}

/// Decodes and fully verifies raw `FLKM` image bytes without panicking or
/// performing out-of-bounds reads.
///
/// # Errors
/// Returns [`ImageError`] on truncation, corrupt framing, CBOR budget failure,
/// hash mismatch, or structural verification failure.
pub fn verify_image_bytes(bytes: &[u8]) -> Result<VmImage, ImageError> {
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(ImageError::Corrupt(format!(
            "VM image size {} exceeds maximum {MAX_IMAGE_BYTES} bytes",
            bytes.len()
        )));
    }
    let image = VmImage::decode(bytes)?;
    verify_image(&image)?;
    Ok(image)
}

/// Performs full security verification on a decoded [`VmImage`] before loading
/// or execution.
///
/// # Errors
/// Returns [`ImageError::Corrupt`] if any structural or security invariant is
/// violated.
pub fn verify_image(image: &VmImage) -> Result<(), ImageError> {
    verify_manifest(image)?;
    verify_program_and_globals(image)?;
    verify_heap(image)?;
    verify_prepared_metadata(image)?;
    Ok(())
}

fn verify_manifest(image: &VmImage) -> Result<(), ImageError> {
    let mut seen_modules = BTreeSet::new();
    for module in &image.manifest.modules {
        if !seen_modules.insert(module.as_str()) {
            return Err(ImageError::Corrupt(
                ImageVerifyError::new("manifest", format!("duplicate module `{module}`"))
                    .to_string(),
            ));
        }
    }

    let mut seen_ifaces = BTreeSet::new();
    for rec in &image.interfaces {
        if rec.interface.module != rec.name {
            return Err(ImageError::Corrupt(
                ImageVerifyError::new(
                    "interfaces",
                    format!(
                        "interface record `{}` carries interface for `{}`",
                        rec.name, rec.interface.module
                    ),
                )
                .to_string(),
            ));
        }
        if !seen_ifaces.insert(rec.name.as_str()) {
            return Err(ImageError::Corrupt(
                ImageVerifyError::new("interfaces", format!("duplicate interface `{}`", rec.name))
                    .to_string(),
            ));
        }
        if !seen_modules.contains(rec.name.as_str()) {
            return Err(ImageError::Corrupt(
                ImageVerifyError::new(
                    "interfaces",
                    format!("interface `{}` is not listed in manifest.modules", rec.name),
                )
                .to_string(),
            ));
        }
    }

    for (&code_id, owner) in &image.manifest.code_owners {
        if !image.program.functions.contains_key(&code_id) {
            return Err(ImageError::Corrupt(
                ImageVerifyError::new(
                    "manifest",
                    format!("code_owners references missing CodeId {code_id} (owner `{owner}`)"),
                )
                .to_string(),
            ));
        }
    }

    Ok(())
}

fn verify_program_and_globals(image: &VmImage) -> Result<(), ImageError> {
    let program = &image.program;
    if image.manifest.external_imports.is_empty() {
        memory::validate_program(program).map_err(|err| {
            ImageError::Corrupt(
                ImageVerifyError::new("memory-ir", format!("invalid memory IR: {err}")).to_string(),
            )
        })?;
    } else {
        for (&code, func) in &program.functions {
            memory::validate(func).map_err(|err| {
                ImageError::Corrupt(
                    ImageVerifyError::new("memory-ir", format!("invalid function {code}: {err}"))
                        .to_string(),
                )
            })?;
            verify_block_code_refs(code, &func.body, &program.functions)?;
        }
    }

    for (&(code, _), span) in &program.op_spans {
        if !program.functions.contains_key(&code) {
            return Err(ImageError::Corrupt(
                ImageVerifyError::new(
                    "memory-ir",
                    format!("op_spans references missing CodeId {code}"),
                )
                .to_string(),
            ));
        }
        if span.start > span.end {
            return Err(ImageError::Corrupt(
                ImageVerifyError::new(
                    "memory-ir",
                    format!(
                        "op_spans for CodeId {code} has inverted span {}..{}",
                        span.start, span.end
                    ),
                )
                .to_string(),
            ));
        }
    }

    let prog_globals = verify_program_globals(image)?;
    verify_global_entries(image, &prog_globals)
}

fn verify_program_globals(image: &VmImage) -> Result<BTreeMap<&Name, CodeId>, ImageError> {
    let program = &image.program;
    let mut prog_globals: BTreeMap<&Name, CodeId> = BTreeMap::new();
    for g in &program.globals {
        if prog_globals.insert(&g.name, g.initializer).is_some() {
            return Err(ImageError::Corrupt(
                ImageVerifyError::new(
                    "memory-ir",
                    format!("duplicate program global {}.{}", g.name.module, g.name.name),
                )
                .to_string(),
            ));
        }
        let Some(init_fn) = program.functions.get(&g.initializer) else {
            return Err(ImageError::Corrupt(
                ImageVerifyError::new(
                    "memory-ir",
                    format!(
                        "global {}.{} references missing initializer code {}",
                        g.name.module, g.name.name, g.initializer
                    ),
                )
                .to_string(),
            ));
        };
        if !init_fn.params.is_empty() {
            return Err(ImageError::Corrupt(
                ImageVerifyError::new(
                    "memory-ir",
                    format!(
                        "global {}.{} initializer code {} expects {} params, must be 0",
                        g.name.module,
                        g.name.name,
                        g.initializer,
                        init_fn.params.len()
                    ),
                )
                .to_string(),
            ));
        }
    }

    if let Some(entry) = &image.manifest.entry
        && !prog_globals.contains_key(entry)
    {
        return Err(ImageError::Corrupt(
            ImageVerifyError::new(
                "manifest",
                format!(
                    "entry global {}.{} not found in program",
                    entry.module, entry.name
                ),
            )
            .to_string(),
        ));
    }
    if let Some(root) = &image.manifest.root_layer
        && !prog_globals.contains_key(root)
    {
        return Err(ImageError::Corrupt(
            ImageVerifyError::new(
                "manifest",
                format!(
                    "root_layer global {}.{} not found in program",
                    root.module, root.name
                ),
            )
            .to_string(),
        ));
    }
    Ok(prog_globals)
}

fn verify_global_entries(
    image: &VmImage,
    prog_globals: &BTreeMap<&Name, CodeId>,
) -> Result<(), ImageError> {
    let program = &image.program;
    let mut seen_entries = BTreeSet::new();
    for entry in &image.globals {
        if !seen_entries.insert(&entry.name) {
            return Err(ImageError::Corrupt(
                ImageVerifyError::new(
                    "globals",
                    format!(
                        "duplicate global entry {}.{}",
                        entry.name.module, entry.name.name
                    ),
                )
                .to_string(),
            ));
        }
        let Some(init_fn) = program.functions.get(&entry.initializer) else {
            return Err(ImageError::Corrupt(
                ImageVerifyError::new(
                    "globals",
                    format!(
                        "global entry {}.{} references missing code {}",
                        entry.name.module, entry.name.name, entry.initializer
                    ),
                )
                .to_string(),
            ));
        };
        if !init_fn.params.is_empty() {
            return Err(ImageError::Corrupt(
                ImageVerifyError::new(
                    "globals",
                    format!(
                        "global entry {}.{} initializer code {} must take 0 params",
                        entry.name.module, entry.name.name, entry.initializer
                    ),
                )
                .to_string(),
            ));
        }
        if let Some(&prog_init) = prog_globals.get(&entry.name)
            && prog_init != entry.initializer
        {
            return Err(ImageError::Corrupt(
                ImageVerifyError::new(
                    "globals",
                    format!(
                        "global entry {}.{} initializer {} disagrees with program initializer {}",
                        entry.name.module, entry.name.name, entry.initializer, prog_init
                    ),
                )
                .to_string(),
            ));
        }
    }
    Ok(())
}

fn verify_block_code_refs(
    caller: CodeId,
    block: &Block,
    functions: &BTreeMap<CodeId, Function>,
) -> Result<(), ImageError> {
    for inst in &block.instructions {
        let Instruction::Let { value, .. } = inst else {
            continue;
        };
        match value {
            Operation::Closure { code, .. }
            | Operation::Task { code, .. }
            | Operation::GenerateLazy { code, .. } => {
                let target = functions.get(code).ok_or_else(|| {
                    ImageError::Corrupt(format!(
                        "function {caller} references unknown CodeId {code}"
                    ))
                })?;
                if target.params.len() != 2 {
                    return Err(ImageError::Corrupt(format!(
                        "closure/task target CodeId {code} expects {} params, must be 2",
                        target.params.len()
                    )));
                }
            }
            Operation::Call { code, args }
            | Operation::Within { code, args, .. }
            | Operation::WithOwnerEvidence { code, args, .. }
            | Operation::Loop { code, args, .. }
            | Operation::FunctionBoundary { code, args }
            | Operation::GenerateStrict { code, args } => {
                let target = functions.get(code).ok_or_else(|| {
                    ImageError::Corrupt(format!("function {caller} calls unknown CodeId {code}"))
                })?;
                if target.params.len() != args.len() {
                    return Err(ImageError::Corrupt(format!(
                        "function {caller} calls CodeId {code} with {} args, expected {}",
                        args.len(),
                        target.params.len()
                    )));
                }
            }
            _ => {}
        }
    }
    if let Exit::Branch { then_, else_, .. } = &block.exit {
        verify_block_code_refs(caller, then_, functions)?;
        verify_block_code_refs(caller, else_, functions)?;
    }
    Ok(())
}

fn verify_heap(image: &VmImage) -> Result<(), ImageError> {
    for entry in &image.globals {
        if let Some(val) = &entry.value {
            image.heap.validate_value_indices(val)?;
        }
    }
    image.heap.validate_structure(&image.program.functions)
}

fn verify_prepared_metadata(image: &VmImage) -> Result<(), ImageError> {
    let Some(meta) = &image.prepared else {
        return Ok(());
    };
    if meta.tier1_version != TIER1_METADATA_VERSION {
        return Err(ImageError::Corrupt(
            ImageVerifyError::new(
                "prepared",
                format!(
                    "unsupported tier-1 bytecode metadata version {} (expected {TIER1_METADATA_VERSION})",
                    meta.tier1_version
                ),
            )
            .to_string(),
        ));
    }
    if !image.program.globals.iter().any(|g| g.name == meta.entry) {
        return Err(ImageError::Corrupt(
            ImageVerifyError::new(
                "prepared",
                format!(
                    "prepared entry {}.{} does not exist in program globals",
                    meta.entry.module, meta.entry.name
                ),
            )
            .to_string(),
        ));
    }
    if meta.functions == 0 || meta.functions > image.program.functions.len() {
        return Err(ImageError::Corrupt(
            ImageVerifyError::new(
                "prepared",
                format!(
                    "prepared function count {} out of range (program functions: {})",
                    meta.functions,
                    image.program.functions.len()
                ),
            )
            .to_string(),
        ));
    }
    if meta.instructions == 0 {
        return Err(ImageError::Corrupt(
            ImageVerifyError::new("prepared", "prepared bytecode reports 0 instructions")
                .to_string(),
        ));
    }
    if image.manifest.external_imports.is_empty() {
        let prepared = tier1::prepare(&image.program, &meta.entry).map_err(|err| {
            ImageError::Corrupt(
                ImageVerifyError::new(
                    "prepared",
                    format!("embedded tier-1 entry failed preparation: {err}"),
                )
                .to_string(),
            )
        })?;
        verify_bytecode(&prepared).map_err(|err| {
            ImageError::Corrupt(
                ImageVerifyError::new("prepared", format!("bytecode verification failed: {err}"))
                    .to_string(),
            )
        })?;
        let stats = prepared.stats();
        if stats.functions != meta.functions || stats.instructions != meta.instructions {
            return Err(ImageError::Corrupt(
                ImageVerifyError::new(
                    "prepared",
                    format!(
                        "prepared stats mismatch: metadata ({} fns, {} instrs) != verified ({} fns, {} instrs)",
                        meta.functions, meta.instructions, stats.functions, stats.instructions
                    ),
                )
                .to_string(),
            ));
        }
    }
    Ok(())
}
