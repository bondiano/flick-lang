//! Target primitive table and free bit pattern (niche) descriptions.

use std::cmp::Reverse;

use super::LayoutError;

/// A contiguous range of unused unsigned integer bit patterns at a fixed byte
/// offset inside a value's representation.
///
/// The unsigned integer of width `width` bytes (`1`, `2`, `4`, or `8`) at byte
/// offset `offset` never holds any value in `start..=end` for valid values of
/// the type. Enclosing sum types may encode payload-free constructors using
/// values from this range without allocating a separate discriminant field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Niche {
    /// Byte offset from the start of the value where the niche scalar lies.
    pub offset: u64,
    /// Width in bytes of the unsigned integer scalar (`1`, `2`, `4`, or `8`).
    pub width: u64,
    /// First free (unused) bit pattern, inclusive.
    pub start: u64,
    /// Last free (unused) bit pattern, inclusive (`start <= end`).
    pub end: u64,
}

impl Niche {
    /// Non-null pointer niche: bit pattern `0` is unused at offset `0`.
    pub const fn null_pointer(width: u64) -> Niche {
        Niche {
            offset: 0,
            width,
            start: 0,
            end: 0,
        }
    }

    /// Standard `Bool` niche: values `0` and `1` are valid, leaving `2..=255`
    /// unused in a 1-byte boolean.
    pub const fn bool() -> Niche {
        Niche {
            offset: 0,
            width: 1,
            start: 2,
            end: 255,
        }
    }

    /// Number of distinct unused bit patterns in this niche.
    pub const fn count(self) -> u128 {
        (self.end as u128) - (self.start as u128) + 1
    }

    /// Shifts this niche's byte offset by `delta`, returning `None` on overflow.
    pub const fn shifted(self, delta: u64) -> Option<Niche> {
        match self.offset.checked_add(delta) {
            Some(offset) => Some(Niche { offset, ..self }),
            None => None,
        }
    }

    /// Consumes `used` consecutive patterns from `start`, returning the
    /// remaining unused niche if any patterns remain.
    pub const fn consume(self, used: u64) -> Option<Niche> {
        let next = (self.start as u128) + (used as u128);
        if next > (self.end as u128) {
            None
        } else {
            #[allow(clippy::cast_possible_truncation)]
            Some(Niche {
                start: next as u64,
                ..self
            })
        }
    }

    /// Whether `value` falls inside this niche range.
    pub const fn contains(self, value: u64) -> bool {
        value >= self.start && value <= self.end
    }
}

/// Maximum unsigned value representable in `width` bytes (`1`, `2`, `4`, `8`).
pub(crate) const fn max_for_width(width: u64) -> Option<u64> {
    match width {
        1 => Some(u8::MAX as u64),
        2 => Some(u16::MAX as u64),
        4 => Some(u32::MAX as u64),
        8 => Some(u64::MAX),
        _ => None,
    }
}

/// Minimal byte width (`1`, `2`, `4`, or `8`) able to hold `variant_count`
/// distinct unsigned discriminant tags `0..variant_count`.
pub(crate) const fn discriminant_width(variant_count: usize) -> u64 {
    if variant_count <= (u8::MAX as usize) + 1 {
        1
    } else if variant_count <= (u16::MAX as usize) + 1 {
        2
    } else if (variant_count as u128) <= (u32::MAX as u128) + 1 {
        4
    } else {
        8
    }
}

/// Picks the preferred niche between `a` and `b` deterministically:
/// larger capacity first, then narrower scalar width, then lower offset,
/// then lower starting bit pattern.
pub(crate) fn better_niche(a: Option<Niche>, b: Option<Niche>) -> Option<Niche> {
    match (a, b) {
        (None, other) | (other, None) => other,
        (Some(x), Some(y)) => {
            let x_key = (Reverse(x.count()), x.width, x.offset, x.start);
            let y_key = (Reverse(y.count()), y.width, y.offset, y.start);
            if x_key <= y_key { Some(x) } else { Some(y) }
        }
    }
}

/// Size, alignment, and optional niche of a single primitive or handle in a
/// backend's [`PrimitiveTable`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PrimitiveLayout {
    pub size: u64,
    pub align: u64,
    pub niche: Option<Niche>,
}

impl PrimitiveLayout {
    pub const fn new(size: u64, align: u64, niche: Option<Niche>) -> PrimitiveLayout {
        PrimitiveLayout { size, align, niche }
    }

    pub(crate) fn validate(self, label: &str) -> Result<(), LayoutError> {
        if !self.align.is_power_of_two() {
            return Err(LayoutError::InvalidPrimitiveTable(format!(
                "`{label}` alignment {} is not a power of two",
                self.align
            )));
        }
        if !self.size.is_multiple_of(self.align) {
            return Err(LayoutError::InvalidPrimitiveTable(format!(
                "`{label}` size {} is not a multiple of alignment {}",
                self.size, self.align
            )));
        }
        if let Some(niche) = self.niche {
            validate_niche(niche, self.size, self.align, label)?;
        }
        Ok(())
    }
}

fn validate_niche(niche: Niche, size: u64, align: u64, label: &str) -> Result<(), LayoutError> {
    let Some(max_val) = max_for_width(niche.width) else {
        return Err(LayoutError::InvalidPrimitiveTable(format!(
            "`{label}` niche width {} must be 1, 2, 4, or 8",
            niche.width
        )));
    };
    if niche.start > niche.end {
        return Err(LayoutError::InvalidPrimitiveTable(format!(
            "`{label}` niche start {} exceeds end {}",
            niche.start, niche.end
        )));
    }
    if niche.end > max_val {
        return Err(LayoutError::InvalidPrimitiveTable(format!(
            "`{label}` niche end {} does not fit in {} byte(s)",
            niche.end, niche.width
        )));
    }
    if niche.start == 0 && niche.end == max_val {
        return Err(LayoutError::InvalidPrimitiveTable(format!(
            "`{label}` niche leaves no valid bit patterns"
        )));
    }
    let Some(end_byte) = niche.offset.checked_add(niche.width) else {
        return Err(LayoutError::InvalidPrimitiveTable(format!(
            "`{label}` niche offset {} overflows",
            niche.offset
        )));
    };
    if end_byte > size {
        return Err(LayoutError::InvalidPrimitiveTable(format!(
            "`{label}` niche [{}, {}) exceeds size {}",
            niche.offset, end_byte, size
        )));
    }
    let req_align = niche.width.min(align);
    if !niche.offset.is_multiple_of(req_align) {
        return Err(LayoutError::InvalidPrimitiveTable(format!(
            "`{label}` niche offset {} is not aligned to {req_align}",
            niche.offset
        )));
    }
    Ok(())
}

/// Built-in primitive, fixed-width numeric, pointer, closure, and runtime
/// handle kinds known to the layout engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Primitive {
    Unit,
    Never,
    Bool,
    Int,
    Float,
    Int8,
    Int16,
    Int64,
    UInt8,
    UInt16,
    UInt32,
    UInt64,
    Float32,
    Pointer,
    Closure,
    String,
    BigInt,
    Bytes,
}

/// Backend-supplied sizes, alignments, and niches for all primitive types,
/// pointers, closures, and runtime handles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrimitiveTable {
    pub unit: PrimitiveLayout,
    pub bool: PrimitiveLayout,
    pub int: PrimitiveLayout,
    pub float: PrimitiveLayout,
    pub int8: PrimitiveLayout,
    pub int16: PrimitiveLayout,
    pub int64: PrimitiveLayout,
    pub uint8: PrimitiveLayout,
    pub uint16: PrimitiveLayout,
    pub uint32: PrimitiveLayout,
    pub uint64: PrimitiveLayout,
    pub float32: PrimitiveLayout,
    pub pointer: PrimitiveLayout,
    pub closure: PrimitiveLayout,
    pub string: PrimitiveLayout,
    pub bigint: PrimitiveLayout,
    pub bytes: PrimitiveLayout,
}

impl PrimitiveTable {
    /// Standard 32-bit target table (e.g. `wasm32` in linear memory or ILP32 C):
    /// 4-byte non-null pointers and handles, 8-byte closures (`{fn_ptr, env_ptr}`),
    /// 4-byte `Int`, 8-byte `Float`, 1-byte `Bool` with niche `2..=255`, 0-byte `Unit`.
    pub const fn target_32() -> PrimitiveTable {
        let ptr = PrimitiveLayout::new(4, 4, Some(Niche::null_pointer(4)));
        let closure = PrimitiveLayout::new(8, 4, Some(Niche::null_pointer(4)));
        PrimitiveTable {
            unit: PrimitiveLayout::new(0, 1, None),
            bool: PrimitiveLayout::new(1, 1, Some(Niche::bool())),
            int: PrimitiveLayout::new(4, 4, None),
            float: PrimitiveLayout::new(8, 8, None),
            int8: PrimitiveLayout::new(1, 1, None),
            int16: PrimitiveLayout::new(2, 2, None),
            int64: PrimitiveLayout::new(8, 8, None),
            uint8: PrimitiveLayout::new(1, 1, None),
            uint16: PrimitiveLayout::new(2, 2, None),
            uint32: PrimitiveLayout::new(4, 4, None),
            uint64: PrimitiveLayout::new(8, 8, None),
            float32: PrimitiveLayout::new(4, 4, None),
            pointer: ptr,
            closure,
            string: ptr,
            bigint: ptr,
            bytes: ptr,
        }
    }

    /// Standard 64-bit target table (e.g. LP64 C or LLVM on 64-bit targets):
    /// 8-byte non-null pointers and handles, 16-byte closures (`{fn_ptr, env_ptr}`),
    /// 4-byte `Int`, 8-byte `Float`, 1-byte `Bool` with niche `2..=255`, 0-byte `Unit`.
    pub const fn target_64() -> PrimitiveTable {
        let ptr = PrimitiveLayout::new(8, 8, Some(Niche::null_pointer(8)));
        let closure = PrimitiveLayout::new(16, 8, Some(Niche::null_pointer(8)));
        PrimitiveTable {
            unit: PrimitiveLayout::new(0, 1, None),
            bool: PrimitiveLayout::new(1, 1, Some(Niche::bool())),
            int: PrimitiveLayout::new(4, 4, None),
            float: PrimitiveLayout::new(8, 8, None),
            int8: PrimitiveLayout::new(1, 1, None),
            int16: PrimitiveLayout::new(2, 2, None),
            int64: PrimitiveLayout::new(8, 8, None),
            uint8: PrimitiveLayout::new(1, 1, None),
            uint16: PrimitiveLayout::new(2, 2, None),
            uint32: PrimitiveLayout::new(4, 4, None),
            uint64: PrimitiveLayout::new(8, 8, None),
            float32: PrimitiveLayout::new(4, 4, None),
            pointer: ptr,
            closure,
            string: ptr,
            bigint: ptr,
            bytes: ptr,
        }
    }

    /// Looks up the layout of `prim` in this table.
    pub const fn get(&self, prim: Primitive) -> PrimitiveLayout {
        match prim {
            Primitive::Unit => self.unit,
            Primitive::Never => PrimitiveLayout::new(0, 1, None),
            Primitive::Bool => self.bool,
            Primitive::Int => self.int,
            Primitive::Float => self.float,
            Primitive::Int8 => self.int8,
            Primitive::Int16 => self.int16,
            Primitive::Int64 => self.int64,
            Primitive::UInt8 => self.uint8,
            Primitive::UInt16 => self.uint16,
            Primitive::UInt32 => self.uint32,
            Primitive::UInt64 => self.uint64,
            Primitive::Float32 => self.float32,
            Primitive::Pointer => self.pointer,
            Primitive::Closure => self.closure,
            Primitive::String => self.string,
            Primitive::BigInt => self.bigint,
            Primitive::Bytes => self.bytes,
        }
    }

    /// Checks that every entry in the table has valid alignment, size, and niche.
    pub fn validate(&self) -> Result<(), LayoutError> {
        let entries = [
            ("unit", self.unit),
            ("bool", self.bool),
            ("int", self.int),
            ("float", self.float),
            ("int8", self.int8),
            ("int16", self.int16),
            ("int64", self.int64),
            ("uint8", self.uint8),
            ("uint16", self.uint16),
            ("uint32", self.uint32),
            ("uint64", self.uint64),
            ("float32", self.float32),
            ("pointer", self.pointer),
            ("closure", self.closure),
            ("string", self.string),
            ("bigint", self.bigint),
            ("bytes", self.bytes),
        ];
        for (name, layout) in entries {
            layout.validate(name)?;
        }
        if self.pointer.size == 0 {
            return Err(LayoutError::InvalidPrimitiveTable(
                "`pointer` size must be non-zero".into(),
            ));
        }
        Ok(())
    }
}
