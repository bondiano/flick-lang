//! Byte-level value encoder, decoder, and layout structural invariant verifier
//! used by the property tests.

use std::collections::BTreeSet;

use flick_ir::TypeRef;

use super::super::primitives::max_for_width;
use super::super::{
    Discriminant, FieldLayout, FieldStorage, LayoutEngine, Niche, Primitive, PrimitiveLayout,
    TypeLayout, TypeRepr,
};

/// A concrete runtime value of a monomorphic type for layout round-trip checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Value {
    Unit,
    Bool(bool),
    Int(i32),
    FloatBits(u64),
    Int8(i8),
    Int16(i16),
    Int64(i64),
    UInt8(u8),
    UInt16(u16),
    UInt32(u32),
    UInt64(u64),
    Float32Bits(u32),
    /// Opaque non-niche handle / pointer / closure payload encoded in `size` bytes.
    Handle(Vec<u8>),
    Tuple(Vec<Value>),
    Record(Vec<(String, Value)>),
    Data {
        variant: usize,
        fields: Vec<Value>,
    },
}

/// Simple non-null heap storing pointer-indirected pointees (addresses start at 1).
#[derive(Debug, Default)]
pub(super) struct Heap {
    blocks: Vec<Vec<u8>>,
}

impl Heap {
    fn alloc(&mut self, bytes: Vec<u8>) -> u64 {
        self.blocks.push(bytes);
        // 1-based so 0 is never a valid heap pointer (respecting null niche).
        self.blocks.len() as u64
    }

    fn get(&self, addr: u64) -> &[u8] {
        let idx = usize::try_from(addr.checked_sub(1).expect("non-null heap address"))
            .expect("heap index fits usize");
        &self.blocks[idx]
    }
}

/// Builds a deterministic valid byte representation for a primitive handle
/// (`Pointer`, `Closure`, `String`, `BigInt`, `Bytes`) from `seed`, guaranteed
/// never to fall inside `layout.niche`.
pub(super) fn valid_handle_bytes(layout: PrimitiveLayout, seed: u64) -> Vec<u8> {
    let size = usize::try_from(layout.size).expect("handle size fits usize");
    let mut bytes = vec![0u8; size];
    for (i, b) in bytes.iter_mut().enumerate() {
        let shift = (i % 8) * 8;
        #[allow(clippy::cast_possible_truncation)]
        {
            *b = ((seed.wrapping_add(i as u64 + 1)) >> shift) as u8;
        }
    }
    if let Some(niche) = layout.niche {
        let cur = read_uint(&bytes, niche.offset, niche.width);
        if niche.contains(cur) {
            let valid = pick_valid_scalar(niche, seed);
            write_uint(&mut bytes, niche.offset, niche.width, valid);
        }
    }
    bytes
}

fn pick_valid_scalar(niche: Niche, seed: u64) -> u64 {
    let max_val = max_for_width(niche.width).expect("valid niche width");
    if niche.start > 0 {
        seed % niche.start
    } else {
        let after = niche.end.checked_add(1).expect("at least one valid value");
        let span = max_val - after + 1;
        after + (seed % span)
    }
}

/// Encodes `value` into a byte buffer of exact length `layout.size`, filling
/// unwritten padding bytes with `poison`.
pub(super) fn encode(
    engine: &mut LayoutEngine<'_>,
    ty: &TypeRef,
    value: &Value,
    heap: &mut Heap,
    poison: u8,
) -> Vec<u8> {
    let layout = engine.layout(ty).expect("layout succeeds");
    let size = usize::try_from(layout.size).expect("size fits usize");
    let mut buf = vec![poison; size];
    encode_into(engine, &layout, value, &mut buf, heap, poison);
    buf
}

fn encode_into(
    engine: &mut LayoutEngine<'_>,
    layout: &TypeLayout,
    value: &Value,
    dst: &mut [u8],
    heap: &mut Heap,
    poison: u8,
) {
    assert_eq!(dst.len() as u64, layout.size);
    match (&layout.repr, value) {
        (TypeRepr::Primitive(prim), val) => encode_primitive(*prim, layout, val, dst),
        (TypeRepr::Tuple(t), Value::Tuple(items)) => {
            assert_eq!(t.fields.len(), items.len());
            for (field, item) in t.fields.iter().zip(items) {
                encode_field(engine, field, item, dst, heap, poison);
            }
        }
        (TypeRepr::Record(r), Value::Record(items)) => {
            assert_eq!(r.fields.len(), items.len());
            for (rf, (label, item)) in r.fields.iter().zip(items) {
                assert_eq!(&rf.label, label);
                encode_field(engine, &rf.field, item, dst, heap, poison);
            }
        }
        (TypeRepr::Data(d), Value::Data { variant, fields }) => {
            let v = &d.variants[*variant];
            assert_eq!(v.fields.len(), fields.len());
            match &d.discriminant {
                Discriminant::None => {}
                Discriminant::Tag { offset, width } => {
                    let tag = v.tag.expect("tagged variant has tag");
                    write_uint(dst, *offset, *width, tag);
                }
                Discriminant::Niche {
                    offset,
                    width,
                    data_variant,
                    ..
                } => {
                    if variant != data_variant {
                        let tag = v.tag.expect("niche variant has tag");
                        write_uint(dst, *offset, *width, tag);
                    }
                }
            }
            for (field, val) in v.fields.iter().zip(fields) {
                encode_field(engine, field, val, dst, heap, poison);
            }
        }
        (repr, val) => panic!("value {val:?} does not match layout {repr:?}"),
    }
}

fn encode_field(
    engine: &mut LayoutEngine<'_>,
    field: &FieldLayout,
    val: &Value,
    dst: &mut [u8],
    heap: &mut Heap,
    poison: u8,
) {
    let start = usize::try_from(field.offset).expect("offset fits usize");
    let end = start + usize::try_from(field.size).expect("size fits usize");
    match field.storage {
        FieldStorage::ByValue => {
            let field_layout = engine.layout(&field.ty).expect("field layout");
            encode_into(
                engine,
                &field_layout,
                val,
                &mut dst[start..end],
                heap,
                poison,
            );
        }
        FieldStorage::Pointer => {
            let pointee = encode(engine, &field.ty, val, heap, poison);
            let addr = heap.alloc(pointee);
            let ptr_layout = engine.primitives().pointer;
            let mut ptr_bytes = valid_handle_bytes(ptr_layout, addr);
            // Store the heap address in the pointer's low bytes (outside any niche).
            let addr_width = ptr_layout.size.min(8);
            let encoded_addr = encode_heap_addr(addr, ptr_layout.niche);
            write_uint(&mut ptr_bytes, 0, addr_width, encoded_addr);
            dst[start..end].copy_from_slice(&ptr_bytes);
        }
    }
}

/// Maps 1-based heap `addr` to a valid bit pattern avoiding `niche` (when at offset 0).
fn encode_heap_addr(addr: u64, niche: Option<Niche>) -> u64 {
    let Some(n) = niche else {
        return addr;
    };
    if n.offset != 0 {
        return addr;
    }
    let idx = addr - 1;
    if idx < n.start {
        idx
    } else {
        n.end + 1 + (idx - n.start)
    }
}

fn decode_heap_addr(raw: u64, niche: Option<Niche>) -> u64 {
    let Some(n) = niche else {
        return raw;
    };
    if n.offset != 0 {
        return raw;
    }
    assert!(!n.contains(raw), "decoded pointer fell inside niche {n:?}");
    if raw < n.start {
        raw + 1
    } else {
        (raw - n.end - 1) + n.start + 1
    }
}

fn encode_primitive(prim: Primitive, layout: &TypeLayout, val: &Value, dst: &mut [u8]) {
    match (prim, val) {
        (Primitive::Unit, Value::Unit) => {
            dst.fill(0);
        }
        (Primitive::Bool, Value::Bool(b)) => {
            dst.fill(0);
            if !dst.is_empty() {
                dst[0] = u8::from(*b);
            }
        }
        (Primitive::Int, Value::Int(n)) => write_le_bytes(dst, &n.to_le_bytes()),
        (Primitive::Float, Value::FloatBits(bits)) => write_le_bytes(dst, &bits.to_le_bytes()),
        (Primitive::Int8, Value::Int8(n)) => write_le_bytes(dst, &n.to_le_bytes()),
        (Primitive::Int16, Value::Int16(n)) => write_le_bytes(dst, &n.to_le_bytes()),
        (Primitive::Int64, Value::Int64(n)) => write_le_bytes(dst, &n.to_le_bytes()),
        (Primitive::UInt8, Value::UInt8(n)) => write_le_bytes(dst, &n.to_le_bytes()),
        (Primitive::UInt16, Value::UInt16(n)) => write_le_bytes(dst, &n.to_le_bytes()),
        (Primitive::UInt32, Value::UInt32(n)) => write_le_bytes(dst, &n.to_le_bytes()),
        (Primitive::UInt64, Value::UInt64(n)) => write_le_bytes(dst, &n.to_le_bytes()),
        (Primitive::Float32, Value::Float32Bits(bits)) => write_le_bytes(dst, &bits.to_le_bytes()),
        (
            Primitive::Pointer
            | Primitive::Closure
            | Primitive::String
            | Primitive::BigInt
            | Primitive::Bytes,
            Value::Handle(bytes),
        ) => {
            assert_eq!(bytes.len() as u64, layout.size);
            dst.copy_from_slice(bytes);
        }
        (p, v) => panic!("cannot encode {v:?} as {p:?}"),
    }
}

fn write_le_bytes(dst: &mut [u8], src: &[u8]) {
    dst.fill(0);
    let n = dst.len().min(src.len());
    dst[..n].copy_from_slice(&src[..n]);
}

/// Decodes a value from `bytes` and `heap` using ONLY the computed `TypeLayout`.
pub(super) fn decode(
    engine: &mut LayoutEngine<'_>,
    ty: &TypeRef,
    bytes: &[u8],
    heap: &Heap,
) -> Value {
    let layout = engine.layout(ty).expect("layout succeeds");
    decode_with(engine, &layout, bytes, heap)
}

fn decode_with(
    engine: &mut LayoutEngine<'_>,
    layout: &TypeLayout,
    src: &[u8],
    heap: &Heap,
) -> Value {
    assert_eq!(src.len() as u64, layout.size);
    match &layout.repr {
        TypeRepr::Primitive(prim) => decode_primitive(*prim, src),
        TypeRepr::Tuple(t) => {
            let items = t
                .fields
                .iter()
                .map(|f| decode_field(engine, f, src, heap))
                .collect();
            Value::Tuple(items)
        }
        TypeRepr::Record(r) => {
            let items = r
                .fields
                .iter()
                .map(|rf| (rf.label.clone(), decode_field(engine, &rf.field, src, heap)))
                .collect();
            Value::Record(items)
        }
        TypeRepr::Data(d) => {
            let variant = decode_variant_index(&d.discriminant, &d.variants, src);
            let fields = d.variants[variant]
                .fields
                .iter()
                .map(|f| decode_field(engine, f, src, heap))
                .collect();
            Value::Data { variant, fields }
        }
    }
}

pub(super) fn decode_variant_index(
    disc: &Discriminant,
    variants: &[super::super::VariantLayout],
    src: &[u8],
) -> usize {
    match disc {
        Discriminant::None => 0,
        Discriminant::Tag { offset, width } => {
            let tag = read_uint(src, *offset, *width);
            variants
                .iter()
                .position(|v| v.tag == Some(tag))
                .unwrap_or_else(|| panic!("unknown discriminant tag {tag}"))
        }
        Discriminant::Niche {
            offset,
            width,
            data_variant,
            niche_start,
            niche_end,
        } => {
            let raw = read_uint(src, *offset, *width);
            if raw >= *niche_start && raw <= *niche_end {
                variants
                    .iter()
                    .position(|v| v.tag == Some(raw))
                    .unwrap_or_else(|| panic!("unknown niche tag {raw}"))
            } else {
                *data_variant
            }
        }
    }
}

fn decode_field(
    engine: &mut LayoutEngine<'_>,
    field: &FieldLayout,
    src: &[u8],
    heap: &Heap,
) -> Value {
    let start = usize::try_from(field.offset).expect("offset fits usize");
    let end = start + usize::try_from(field.size).expect("size fits usize");
    match field.storage {
        FieldStorage::ByValue => {
            let field_layout = engine.layout(&field.ty).expect("field layout");
            decode_with(engine, &field_layout, &src[start..end], heap)
        }
        FieldStorage::Pointer => {
            let ptr_layout = engine.primitives().pointer;
            let raw = read_uint(&src[start..end], 0, ptr_layout.size.min(8));
            let addr = decode_heap_addr(raw, ptr_layout.niche);
            let pointee = heap.get(addr);
            decode(engine, &field.ty, pointee, heap)
        }
    }
}

fn decode_primitive(prim: Primitive, src: &[u8]) -> Value {
    match prim {
        Primitive::Unit => Value::Unit,
        Primitive::Bool => Value::Bool(src.first().copied().unwrap_or(0) != 0),
        Primitive::Int => Value::Int(i32::from_le_bytes(read_fixed::<4>(src))),
        Primitive::Float => Value::FloatBits(u64::from_le_bytes(read_fixed::<8>(src))),
        Primitive::Int8 => Value::Int8(i8::from_le_bytes(read_fixed::<1>(src))),
        Primitive::Int16 => Value::Int16(i16::from_le_bytes(read_fixed::<2>(src))),
        Primitive::Int64 => Value::Int64(i64::from_le_bytes(read_fixed::<8>(src))),
        Primitive::UInt8 => Value::UInt8(u8::from_le_bytes(read_fixed::<1>(src))),
        Primitive::UInt16 => Value::UInt16(u16::from_le_bytes(read_fixed::<2>(src))),
        Primitive::UInt32 => Value::UInt32(u32::from_le_bytes(read_fixed::<4>(src))),
        Primitive::UInt64 => Value::UInt64(u64::from_le_bytes(read_fixed::<8>(src))),
        Primitive::Float32 => Value::Float32Bits(u32::from_le_bytes(read_fixed::<4>(src))),
        Primitive::Pointer
        | Primitive::Closure
        | Primitive::String
        | Primitive::BigInt
        | Primitive::Bytes => Value::Handle(src.to_vec()),
        Primitive::Never => panic!("cannot decode uninhabited Never value"),
    }
}

fn read_fixed<const N: usize>(src: &[u8]) -> [u8; N] {
    let mut out = [0u8; N];
    let n = src.len().min(N);
    out[..n].copy_from_slice(&src[..n]);
    out
}

pub(super) fn read_uint(buf: &[u8], offset: u64, width: u64) -> u64 {
    let off = usize::try_from(offset).expect("offset fits usize");
    match width {
        1 => u64::from(buf[off]),
        2 => u64::from(u16::from_le_bytes([buf[off], buf[off + 1]])),
        4 => u64::from(u32::from_le_bytes([
            buf[off],
            buf[off + 1],
            buf[off + 2],
            buf[off + 3],
        ])),
        8 => u64::from_le_bytes([
            buf[off],
            buf[off + 1],
            buf[off + 2],
            buf[off + 3],
            buf[off + 4],
            buf[off + 5],
            buf[off + 6],
            buf[off + 7],
        ]),
        _ => panic!("unsupported uint width {width}"),
    }
}

pub(super) fn write_uint(buf: &mut [u8], offset: u64, width: u64, val: u64) {
    let off = usize::try_from(offset).expect("offset fits usize");
    match width {
        1 => {
            buf[off] = u8::try_from(val).expect("fits u8");
        }
        2 => {
            let b = u16::try_from(val).expect("fits u16").to_le_bytes();
            buf[off..off + 2].copy_from_slice(&b);
        }
        4 => {
            let b = u32::try_from(val).expect("fits u32").to_le_bytes();
            buf[off..off + 4].copy_from_slice(&b);
        }
        8 => {
            let b = val.to_le_bytes();
            buf[off..off + 8].copy_from_slice(&b);
        }
        _ => panic!("unsupported uint width {width}"),
    }
}

/// Verifies universal structural invariants on `layout` (alignment, size,
/// non-overlapping fields, bounds, niche validity, pointer-field consistency).
pub(super) fn assert_layout_invariants(engine: &mut LayoutEngine<'_>, layout: &TypeLayout) {
    assert!(layout.align.is_power_of_two());
    assert_eq!(layout.size % layout.align, 0);
    if let Some(niche) = layout.niche {
        let max_val = max_for_width(niche.width).expect("valid niche width");
        assert!(niche.start <= niche.end);
        assert!(niche.end <= max_val);
        assert!(niche.offset + niche.width <= layout.size);
    }
    match &layout.repr {
        TypeRepr::Primitive(_) => {}
        TypeRepr::Tuple(t) => assert_fields_disjoint(engine, layout, None, &t.fields),
        TypeRepr::Record(r) => {
            let fields: Vec<FieldLayout> = r.fields.iter().map(|rf| rf.field.clone()).collect();
            assert_fields_disjoint(engine, layout, None, &fields);
            for pair in r.fields.windows(2) {
                assert!(flick_ir::cmp_utf16(&pair[0].label, &pair[1].label).is_lt());
            }
        }
        TypeRepr::Data(d) => {
            let reserved = match d.discriminant {
                Discriminant::Tag { offset, width } => Some((offset, offset + width)),
                Discriminant::None | Discriminant::Niche { .. } => None,
            };
            let mut expected_ptrs = BTreeSet::new();
            for (v_idx, v) in d.variants.iter().enumerate() {
                assert_fields_disjoint(engine, layout, reserved, &v.fields);
                for f in &v.fields {
                    if f.is_pointer() {
                        expected_ptrs.insert((v_idx, f.index));
                    }
                }
            }
            assert_eq!(d.pointer_fields, expected_ptrs);
        }
    }
}

fn assert_fields_disjoint(
    engine: &mut LayoutEngine<'_>,
    parent: &TypeLayout,
    reserved: Option<(u64, u64)>,
    fields: &[FieldLayout],
) {
    let mut ranges: Vec<(u64, u64)> = Vec::new();
    if let Some(r) = reserved {
        ranges.push(r);
    }
    for (pos, f) in fields.iter().enumerate() {
        assert_eq!(f.index, pos);
        assert!(f.align.is_power_of_two());
        assert!(f.align <= parent.align);
        assert_eq!(f.offset % f.align, 0);
        assert!(f.offset + f.size <= parent.size);
        if f.is_pointer() {
            assert_eq!(f.size, engine.primitives().pointer.size);
            assert_eq!(f.align, engine.primitives().pointer.align);
        }
        if f.size > 0 {
            let range = (f.offset, f.offset + f.size);
            for &(s, e) in &ranges {
                assert!(
                    range.1 <= s || e <= range.0,
                    "overlapping field range {range:?} and ({s}, {e}) in {parent:?}"
                );
            }
            ranges.push(range);
        }
    }
}
