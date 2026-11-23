//! Output representation types for computed data layouts.

use std::collections::BTreeSet;

use flick_ir::TypeRef;

use super::{Niche, Primitive};

/// Complete computed memory layout of one monomorphic IR type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeLayout {
    /// Total byte size of the value (always a multiple of `align`).
    pub size: u64,
    /// Required byte alignment (always a non-zero power of two).
    pub align: u64,
    /// Unused bit patterns inside this layout available to enclosing sum types.
    pub niche: Option<Niche>,
    /// Structural layout details.
    pub repr: TypeRepr,
}

impl TypeLayout {
    /// Returns the set of `(variant_index, field_index)` pairs stored behind
    /// pointers when this is a data layout, or an empty set otherwise.
    pub fn pointer_fields(&self) -> Option<&BTreeSet<(usize, usize)>> {
        match &self.repr {
            TypeRepr::Data(data) => Some(&data.pointer_fields),
            _ => None,
        }
    }
}

/// Structural details of a [`TypeLayout`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeRepr {
    /// A primitive scalar, closure, or runtime handle from the primitive table.
    Primitive(Primitive),
    /// A tuple `(T0, T1, ...)`, with fields in positional order `0..n`.
    Tuple(TupleLayout),
    /// A closed record `{ l0 : T0, ... }`, with fields in canonical
    /// `Record.labels` UTF-16 order.
    Record(RecordLayout),
    /// An algebraic data type or `type error`.
    Data(DataLayout),
}

/// How a single field is stored inside its enclosing aggregate or variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FieldStorage {
    /// Stored inline by value.
    ByValue,
    /// Stored behind a non-null pointer selected by indirection analysis
    /// (for growing recursive instantiations, cycle edges, or erased
    /// existential types).
    Pointer,
}

/// Byte placement and storage mode of one field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldLayout {
    /// Semantic 0-based index (declaration order for constructors, positional
    /// index for tuples, canonical `Record.labels` index for records).
    pub index: usize,
    /// Byte offset from the start of the enclosing value.
    pub offset: u64,
    /// Byte size occupied inside the enclosing value (`pointer.size` when
    /// `storage == FieldStorage::Pointer`).
    pub size: u64,
    /// Byte alignment inside the enclosing value (`pointer.align` when
    /// `storage == FieldStorage::Pointer`).
    pub align: u64,
    /// Monomorphic IR type of the field (the pointee type when
    /// `storage == FieldStorage::Pointer`).
    pub ty: TypeRef,
    /// Whether the field is stored inline by value or behind a pointer.
    pub storage: FieldStorage,
}

impl FieldLayout {
    /// Whether this field is stored behind a pointer rather than inline.
    pub const fn is_pointer(&self) -> bool {
        matches!(self.storage, FieldStorage::Pointer)
    }
}

/// One labeled field of a closed [`RecordLayout`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordFieldLayout {
    /// Record label (ordered canonically by UTF-16 code units, matching
    /// `Record.labels`).
    pub label: String,
    /// Byte placement and type of the field.
    pub field: FieldLayout,
}

/// Layout of a tuple `(T0, T1, ...)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TupleLayout {
    /// Tuple elements in positional order `0..n`. Physical byte offsets may be
    /// reordered to reduce padding.
    pub fields: Vec<FieldLayout>,
}

/// Layout of a closed record `{ l0 : T0, ... }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordLayout {
    /// Record fields in canonical `Record.labels` (UTF-16) order. Physical
    /// byte offsets may be reordered to reduce padding.
    pub fields: Vec<RecordFieldLayout>,
}

/// Layout of an algebraic data type or `type error`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataLayout {
    /// How constructors are distinguished at runtime.
    pub discriminant: Discriminant,
    /// Active constructors in declaration order.
    pub variants: Vec<VariantLayout>,
    /// Whether this layout comes from an `unboxed type` wrapper whose
    /// representation is its single field.
    pub unboxed: bool,
    /// Set of `(variant_index, field_index)` pairs stored behind pointers
    /// (`variant_index` indexes [`DataLayout::variants`]).
    pub pointer_fields: BTreeSet<(usize, usize)>,
}

/// How constructors of a [`DataLayout`] are distinguished at runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Discriminant {
    /// Zero or one constructor: no discriminant is stored.
    None,
    /// Explicit unsigned integer tag of `width` bytes (`1`, `2`, `4`, or `8`)
    /// stored at byte `offset`.
    Tag { offset: u64, width: u64 },
    /// Niche-encoded discriminant: `data_variant` occupies the full value, and
    /// every other (zero-payload) constructor is encoded as a distinct bit
    /// pattern in `niche_start..=niche_end` inside the `width`-byte unsigned
    /// integer at `offset`.
    Niche {
        offset: u64,
        width: u64,
        data_variant: usize,
        niche_start: u64,
        niche_end: u64,
    },
}

/// Layout of one constructor of a [`DataLayout`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantLayout {
    /// Constructor name from the type declaration.
    pub name: String,
    /// 0-based index of this constructor in [`flick_ir::TypeDecl::constructors`].
    pub decl_index: usize,
    /// Unsigned tag or niche bit pattern identifying this constructor, or
    /// `None` when [`Discriminant::None`] or when this is the `data_variant`
    /// of [`Discriminant::Niche`].
    pub tag: Option<u64>,
    /// Constructor fields in declaration order (empty for payload-free
    /// constructors).
    pub fields: Vec<FieldLayout>,
    /// Trailing GADT dictionary pointer slots in `ConstructorDecl::constraints`
    /// order.
    pub dicts: Vec<DictFieldLayout>,
}

/// Byte placement of one trailing GADT class dictionary pointer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictFieldLayout {
    /// 0-based index in `ConstructorDecl::constraints`.
    pub index: usize,
    /// Byte offset from the start of the enclosing value.
    pub offset: u64,
    /// Byte size (`pointer.size`).
    pub size: u64,
    /// Byte alignment (`pointer.align`).
    pub align: u64,
}
