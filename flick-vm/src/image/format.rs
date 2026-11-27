//! Self-framing binary container format (`FLKM`) for Flick VM images.
//!
//! Layout (little-endian):
//! - `[0..4]`   magic: `b"FLKM"`
//! - `[4..8]`   `format_version`: `u32` (1)
//! - `[8..40]`  `compiler_hash`: `[u8; 32]`
//! - `[40..72]` `memory_ir_hash`: `[u8; 32]`
//! - `[72..76]` `section_count`: `u32`
//! - `[76..80]` reserved: `u32` (must be 0)
//! - `[80..80 + section_count * 20]` section directory entries:
//!   - `tag: u32`
//!   - `offset: u64`
//!   - `length: u64`
//! - Followed by section payloads encoded with CBOR (`ciborium`).

use std::collections::BTreeMap;
use std::io::Cursor;

use flick_core::ir::memory::{self, Program};
use serde::Serialize;
use serde::de::DeserializeOwned;
use sha2::{Digest as _, Sha256};

use super::heap_types::HeapSnapshot;
use super::{
    GlobalEntry, ImageError, ImageHash, ImageManifest, ModuleRecord, PreparedMetadata, VmImage,
    compiler_hash, memory_ir_hash,
};

/// Fixed 4-byte container magic header.
pub const IMAGE_MAGIC: &[u8; 4] = b"FLKM";

/// Current binary container format version.
pub const IMAGE_FORMAT_VERSION: u32 = 1;

const HEADER_BYTES: usize = 80;
const DIR_ENTRY_BYTES: usize = 20;
const MAX_SECTIONS: u32 = 16;

const TAG_MANIFEST: u32 = 1;
const TAG_INTERFACES: u32 = 2;
const TAG_MEMORY_IR: u32 = 3;
const TAG_PREPARED: u32 = 4;
const TAG_GLOBALS: u32 = 5;
const TAG_HEAP: u32 = 6;

pub(crate) fn sha256_bytes(bytes: &[u8]) -> ImageHash {
    Sha256::digest(bytes).into()
}

pub(crate) fn cbor_encode<T: Serialize>(value: &T, label: &str) -> Result<Vec<u8>, ImageError> {
    let mut buf = Vec::new();
    ciborium::into_writer(value, &mut buf)
        .map_err(|err| ImageError::Corrupt(format!("failed to encode {label} section: {err}")))?;
    Ok(buf)
}

fn cbor_decode<T: DeserializeOwned>(slice: &[u8], label: &str) -> Result<T, ImageError> {
    let mut cursor = Cursor::new(slice);
    let value: T = ciborium::from_reader(&mut cursor)
        .map_err(|err| ImageError::Corrupt(format!("failed to decode {label} section: {err}")))?;
    let consumed = usize::try_from(cursor.position()).unwrap_or(usize::MAX);
    if consumed != slice.len() {
        return Err(ImageError::Corrupt(format!(
            "{label} section has {} trailing bytes",
            slice.len().saturating_sub(consumed)
        )));
    }
    Ok(value)
}

/// Encodes `image` into the self-framing `FLKM` binary format.
pub(crate) fn encode(image: &VmImage) -> Result<Vec<u8>, ImageError> {
    let manifest_bytes = cbor_encode(&image.manifest, "manifest")?;
    let interfaces_bytes = cbor_encode(&image.interfaces, "interfaces")?;
    let memory_ir_bytes = cbor_encode(&image.program, "memory-ir")?;
    let prepared_bytes = match &image.prepared {
        Some(meta) => Some(cbor_encode(meta, "prepared")?),
        None => None,
    };
    let globals_bytes = cbor_encode(&image.globals, "globals")?;
    let heap_bytes = cbor_encode(&image.heap, "heap-snapshot")?;

    let mut sections: Vec<(u32, Vec<u8>)> = vec![
        (TAG_MANIFEST, manifest_bytes),
        (TAG_INTERFACES, interfaces_bytes),
        (TAG_MEMORY_IR, memory_ir_bytes),
    ];
    if let Some(bytes) = prepared_bytes {
        sections.push((TAG_PREPARED, bytes));
    }
    sections.push((TAG_GLOBALS, globals_bytes));
    sections.push((TAG_HEAP, heap_bytes));

    let section_count = u32::try_from(sections.len())
        .map_err(|_| ImageError::Corrupt("section count overflow".into()))?;
    let dir_bytes = sections.len() * DIR_ENTRY_BYTES;
    let payload_total: usize = sections.iter().map(|(_, b)| b.len()).sum();
    let total_len = HEADER_BYTES
        .checked_add(dir_bytes)
        .and_then(|n| n.checked_add(payload_total))
        .ok_or_else(|| ImageError::Corrupt("image size overflow".into()))?;

    let mut out = Vec::with_capacity(total_len);
    out.extend_from_slice(IMAGE_MAGIC);
    out.extend_from_slice(&IMAGE_FORMAT_VERSION.to_le_bytes());
    out.extend_from_slice(&image.compiler_hash);
    out.extend_from_slice(&image.memory_ir_hash);
    out.extend_from_slice(&section_count.to_le_bytes());
    out.extend_from_slice(&0_u32.to_le_bytes());

    let mut next_offset = u64::try_from(HEADER_BYTES + dir_bytes)
        .map_err(|_| ImageError::Corrupt("section offset overflow".into()))?;
    for (tag, payload) in &sections {
        let len = u64::try_from(payload.len())
            .map_err(|_| ImageError::Corrupt("section length overflow".into()))?;
        out.extend_from_slice(&tag.to_le_bytes());
        out.extend_from_slice(&next_offset.to_le_bytes());
        out.extend_from_slice(&len.to_le_bytes());
        next_offset = next_offset
            .checked_add(len)
            .ok_or_else(|| ImageError::Corrupt("section offset overflow".into()))?;
    }
    for (_, payload) in sections {
        out.extend_from_slice(&payload);
    }
    Ok(out)
}

/// Decodes and structurally validates a `FLKM` binary container.
pub(crate) fn decode(bytes: &[u8]) -> Result<VmImage, ImageError> {
    let (found_compiler, found_ir, section_count) = parse_header(bytes)?;
    let slices = parse_section_directory(bytes, section_count)?;

    let manifest_slice = require_section(&slices, TAG_MANIFEST, "manifest")?;
    let interfaces_slice = require_section(&slices, TAG_INTERFACES, "interfaces")?;
    let memory_ir_slice = require_section(&slices, TAG_MEMORY_IR, "memory-ir")?;
    let globals_slice = require_section(&slices, TAG_GLOBALS, "globals")?;
    let heap_slice = require_section(&slices, TAG_HEAP, "heap-snapshot")?;

    let manifest: ImageManifest = cbor_decode(manifest_slice, "manifest")?;
    let interfaces: Vec<ModuleRecord> = cbor_decode(interfaces_slice, "interfaces")?;
    let program: Program = cbor_decode(memory_ir_slice, "memory-ir")?;
    let prepared: Option<PreparedMetadata> = match slices.get(&TAG_PREPARED) {
        Some(slice) => Some(cbor_decode(slice, "prepared")?),
        None => None,
    };
    let globals: Vec<GlobalEntry> = cbor_decode(globals_slice, "globals")?;
    let heap: HeapSnapshot = cbor_decode(heap_slice, "heap-snapshot")?;

    validate_decoded_image(
        &manifest,
        &interfaces,
        &program,
        prepared.as_ref(),
        memory_ir_slice,
        &globals,
        &heap,
    )?;

    Ok(VmImage {
        compiler_hash: found_compiler,
        memory_ir_hash: found_ir,
        manifest,
        interfaces,
        program,
        prepared,
        globals,
        heap,
    })
}

fn parse_header(bytes: &[u8]) -> Result<(ImageHash, ImageHash, u32), ImageError> {
    if bytes.len() < HEADER_BYTES {
        return Err(ImageError::Truncated(format!(
            "expected at least {HEADER_BYTES}-byte header, got {} bytes",
            bytes.len()
        )));
    }
    if &bytes[0..4] != IMAGE_MAGIC {
        return Err(ImageError::Corrupt(format!(
            "invalid magic header: expected {:?}, got {:?}",
            IMAGE_MAGIC,
            &bytes[0..4]
        )));
    }
    let version = read_u32(bytes, 4)?;
    if version != IMAGE_FORMAT_VERSION {
        return Err(ImageError::Corrupt(format!(
            "unsupported VM image format version {version} (expected {IMAGE_FORMAT_VERSION})"
        )));
    }
    let found_compiler: ImageHash = bytes[8..40]
        .try_into()
        .map_err(|_| ImageError::Truncated("compiler hash".into()))?;
    let expected_compiler = compiler_hash();
    if found_compiler != expected_compiler {
        return Err(ImageError::CompilerHashMismatch {
            expected: expected_compiler,
            found: found_compiler,
        });
    }
    let found_ir: ImageHash = bytes[40..72]
        .try_into()
        .map_err(|_| ImageError::Truncated("memory-IR hash".into()))?;
    let expected_ir = memory_ir_hash();
    if found_ir != expected_ir {
        return Err(ImageError::MemoryIrHashMismatch {
            expected: expected_ir,
            found: found_ir,
        });
    }
    let section_count = read_u32(bytes, 72)?;
    let reserved = read_u32(bytes, 76)?;
    if reserved != 0 {
        return Err(ImageError::Corrupt(format!(
            "non-zero reserved header flags: {reserved:#x}"
        )));
    }
    if section_count > MAX_SECTIONS {
        return Err(ImageError::Corrupt(format!(
            "section count {section_count} exceeds maximum {MAX_SECTIONS}"
        )));
    }
    Ok((found_compiler, found_ir, section_count))
}

fn parse_section_directory(
    bytes: &[u8],
    section_count: u32,
) -> Result<BTreeMap<u32, &[u8]>, ImageError> {
    let dir_len = (section_count as usize)
        .checked_mul(DIR_ENTRY_BYTES)
        .ok_or_else(|| ImageError::Corrupt("section directory size overflow".into()))?;
    let dir_end = HEADER_BYTES
        .checked_add(dir_len)
        .ok_or_else(|| ImageError::Corrupt("section directory end overflow".into()))?;
    if bytes.len() < dir_end {
        return Err(ImageError::Truncated(format!(
            "section directory requires {dir_end} bytes, got {}",
            bytes.len()
        )));
    }

    let mut ranges: Vec<(usize, usize, u32)> = Vec::with_capacity(section_count as usize);
    let mut slices: BTreeMap<u32, &[u8]> = BTreeMap::new();
    for i in 0..(section_count as usize) {
        let base = HEADER_BYTES + i * DIR_ENTRY_BYTES;
        let tag = read_u32(bytes, base)?;
        let offset = read_u64(bytes, base + 4)?;
        let length = read_u64(bytes, base + 12)?;
        if !matches!(
            tag,
            TAG_MANIFEST | TAG_INTERFACES | TAG_MEMORY_IR | TAG_PREPARED | TAG_GLOBALS | TAG_HEAP
        ) {
            return Err(ImageError::Corrupt(format!("unknown section tag {tag}")));
        }
        if slices.contains_key(&tag) {
            return Err(ImageError::Corrupt(format!("duplicate section tag {tag}")));
        }
        let start = usize::try_from(offset)
            .map_err(|_| ImageError::Corrupt(format!("section {tag} offset overflow")))?;
        let len = usize::try_from(length)
            .map_err(|_| ImageError::Corrupt(format!("section {tag} length overflow")))?;
        let end = start
            .checked_add(len)
            .ok_or_else(|| ImageError::Corrupt(format!("section {tag} range overflow")))?;
        if start < dir_end {
            return Err(ImageError::Corrupt(format!(
                "section {tag} starts at {start} inside header/directory (end {dir_end})"
            )));
        }
        if end > bytes.len() {
            return Err(ImageError::Truncated(format!(
                "section {tag} range {start}..{end} exceeds image length {}",
                bytes.len()
            )));
        }
        ranges.push((start, end, tag));
        slices.insert(tag, &bytes[start..end]);
    }

    ranges.sort_unstable_by_key(|&(start, _, _)| start);
    for pair in ranges.windows(2) {
        if pair[0].1 > pair[1].0 {
            return Err(ImageError::Corrupt(format!(
                "section {} overlaps section {}",
                pair[0].2, pair[1].2
            )));
        }
    }
    Ok(slices)
}

fn require_section<'a>(
    slices: &BTreeMap<u32, &'a [u8]>,
    tag: u32,
    name: &str,
) -> Result<&'a [u8], ImageError> {
    slices
        .get(&tag)
        .copied()
        .ok_or_else(|| ImageError::Corrupt(format!("missing required {name} section (tag {tag})")))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, ImageError> {
    let slice: [u8; 4] = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| ImageError::Truncated(format!("u32 at offset {offset}")))?
        .try_into()
        .map_err(|_| ImageError::Truncated(format!("u32 at offset {offset}")))?;
    Ok(u32::from_le_bytes(slice))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, ImageError> {
    let slice: [u8; 8] = bytes
        .get(offset..offset + 8)
        .ok_or_else(|| ImageError::Truncated(format!("u64 at offset {offset}")))?
        .try_into()
        .map_err(|_| ImageError::Truncated(format!("u64 at offset {offset}")))?;
    Ok(u64::from_le_bytes(slice))
}

fn validate_decoded_image(
    manifest: &ImageManifest,
    interfaces: &[ModuleRecord],
    program: &Program,
    prepared: Option<&PreparedMetadata>,
    memory_ir_slice: &[u8],
    globals: &[GlobalEntry],
    heap: &HeapSnapshot,
) -> Result<(), ImageError> {
    // Structural check of each function body and code reference in `program`.
    // Note: partial module images may reference globals of external imported
    // modules not yet linked, so we validate function bodies and local code IDs
    // directly, and validate full `memory::validate_program` when self-contained.
    if manifest.external_imports.is_empty() {
        memory::validate_program(program)
            .map_err(|err| ImageError::Corrupt(format!("invalid memory IR: {err}")))?;
    } else {
        for (&code, func) in &program.functions {
            memory::validate(func)
                .map_err(|err| ImageError::Corrupt(format!("invalid function {code}: {err}")))?;
        }
        for g in &program.globals {
            if !program.functions.contains_key(&g.initializer) {
                return Err(ImageError::Corrupt(format!(
                    "global {}.{} references missing initializer code {}",
                    g.name.module, g.name.name, g.initializer
                )));
            }
        }
    }

    if let Some(meta) = prepared {
        let actual_digest = sha256_bytes(memory_ir_slice);
        if meta.memory_ir_digest != actual_digest {
            return Err(ImageError::Corrupt(
                "prepared bytecode section digest does not match memory-IR section".into(),
            ));
        }
    }

    // Check every interface matches a module declared in manifest.
    for rec in interfaces {
        if rec.interface.module != rec.name {
            return Err(ImageError::Corrupt(format!(
                "interface record `{}` carries interface for `{}`",
                rec.name, rec.interface.module
            )));
        }
    }

    // Check globals table indices & code references.
    for entry in globals {
        if !program.functions.contains_key(&entry.initializer) {
            return Err(ImageError::Corrupt(format!(
                "global entry {}.{} references missing code {}",
                entry.name.module, entry.name.name, entry.initializer
            )));
        }
        if let Some(val) = &entry.value {
            heap.validate_value_indices(val)?;
        }
    }

    heap.validate_structure(&program.functions)?;
    Ok(())
}
