//! Small, nonrecursive, closed ADTs (including concrete `Result`
//! instantiations) as dedicated Go value structs carried by value across
//! typed Go paths.

use flick_backend_support::Known;
use flick_ir::{Constructor, IrType, Lang};
use flick_pretty::{Document, DocumentArena, docvec};

use super::arith::Native;
use super::go_type::{GoType, OptRepr, TypeDecls};
use super::layout::call;
use super::names;
use super::record::RecordShape;

/// A typed payload slot inside a by-value closed ADT struct (never `flickrt.Value`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum AdtSlot {
    Scalar(Native),
    Opt(OptRepr),
    Record(RecordShape),
    Adt(Box<AdtShape>),
    Unit,
}

/// One field of an ADT constructor variant and its slot index in `AdtShape.slots`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct AdtField {
    pub(super) slot: AdtSlot,
    /// Index into `AdtShape.slots`, or `None` for zero-storage `AdtSlot::Unit`.
    pub(super) pool: Option<usize>,
}

/// One constructor variant of a by-value closed ADT, in declaration order.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct AdtVariant {
    pub(super) name: String,
    pub(super) tag: String,
    pub(super) fields: Vec<AdtField>,
}

/// Layout and identity of an eligible by-value closed ADT or concrete instantiation.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct AdtShape {
    pub(super) module: String,
    pub(super) name: String,
    pub(super) parameterized: bool,
    pub(super) is_result: bool,
    pub(super) variants: Vec<AdtVariant>,
    pub(super) slots: Vec<AdtSlot>,
}

/// On-demand helper operations requested for an `AdtShape` in `adt_types.go`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct AdtOps(u8);

impl AdtOps {
    pub(super) const EQ_WITH: Self = Self(1 << 0);
    pub(super) const ORD: Self = Self(1 << 1);
    pub(super) const HASH: Self = Self(1 << 2);
    pub(super) const SHOW: Self = Self(1 << 3);
    pub(super) const TAG: Self = Self(1 << 4);

    pub(super) const fn has(self, flag: Self) -> bool {
        (self.0 & flag.0) != 0
    }

    pub(super) const fn with(self, flag: Self) -> Self {
        Self(self.0 | flag.0)
    }

    pub(super) fn merge(&mut self, other: AdtOps) -> bool {
        let before = self.0;
        self.0 |= other.0;
        self.0 != before
    }
}

impl AdtSlot {
    pub(super) fn of(lang: &Known, decls: &TypeDecls, ty: &IrType) -> Option<AdtSlot> {
        if ty
            .as_bare_con()
            .is_some_and(|(m, n)| lang.is(Lang::Unit, m, n))
        {
            return Some(AdtSlot::Unit);
        }
        if let Some(native) = Native::of(lang, Some(ty)) {
            return Some(AdtSlot::Scalar(native));
        }
        if let Some(opt) = OptRepr::of(lang, decls, Some(ty))
            && !opt.contains_value()
        {
            return Some(AdtSlot::Opt(opt));
        }
        if let Some(rec) = RecordShape::of(lang, decls, Some(ty))
            && !rec.has_value_slots()
        {
            return Some(AdtSlot::Record(rec));
        }
        AdtShape::of(lang, decls, Some(ty)).map(|adt| AdtSlot::Adt(Box::new(adt)))
    }

    pub(super) fn go_type(&self) -> String {
        match self {
            AdtSlot::Scalar(n) => n.go().to_owned(),
            AdtSlot::Opt(r) => r.go_type(),
            AdtSlot::Record(r) => r.go_type(),
            AdtSlot::Adt(a) => a.go_type(),
            AdtSlot::Unit => "flickrt.UnitType".to_owned(),
        }
    }

    pub(super) fn to_go_type(&self) -> Option<GoType> {
        match self {
            AdtSlot::Scalar(n) => Some(GoType::Scalar(*n)),
            AdtSlot::Opt(r) => Some(GoType::Opt(r.clone())),
            AdtSlot::Record(r) => Some(GoType::Record(r.clone())),
            AdtSlot::Adt(a) => Some(GoType::Adt((**a).clone())),
            AdtSlot::Unit => None,
        }
    }

    pub(super) fn has_pointers(&self) -> bool {
        match self {
            AdtSlot::Scalar(Native::String) => true,
            AdtSlot::Scalar(_) | AdtSlot::Unit => false,
            AdtSlot::Opt(r) => r.has_pointers(),
            AdtSlot::Record(r) => r.has_pointers(),
            AdtSlot::Adt(a) => a.has_pointers(),
        }
    }

    pub(super) fn size_align(&self) -> (usize, usize) {
        match self {
            AdtSlot::Scalar(n) => native_size_align(*n),
            AdtSlot::Opt(r) => r.size_align(),
            AdtSlot::Record(r) => r.size_align(),
            AdtSlot::Adt(a) => a.size_align(),
            AdtSlot::Unit => (0, 1),
        }
    }

    pub(super) fn layout_rank(&self) -> u8 {
        if self.has_pointers() {
            return 1;
        }
        match self.size_align().1 {
            8 => 2,
            4 => 3,
            2 => 4,
            _ => 5,
        }
    }

    pub(super) fn is_go_comparable(&self) -> bool {
        match self {
            AdtSlot::Scalar(Native::Float | Native::Float32) => false,
            AdtSlot::Scalar(_) | AdtSlot::Unit => true,
            AdtSlot::Opt(r) => r.is_go_comparable(),
            AdtSlot::Record(r) => r.is_go_comparable(),
            AdtSlot::Adt(a) => a.is_go_comparable(),
        }
    }

    pub(super) fn feed_hash(&self, feed: &mut impl FnMut(&[u8])) {
        match self {
            AdtSlot::Unit => feed(&[1]),
            AdtSlot::Scalar(n) => feed(&[2, *n as u8]),
            AdtSlot::Opt(r) => {
                feed(&[3]);
                let s = r.go_type();
                feed(&(s.len() as u64).to_le_bytes());
                feed(s.as_bytes());
            }
            AdtSlot::Record(r) => {
                feed(&[4]);
                feed(&r.shape_hash().to_le_bytes());
            }
            AdtSlot::Adt(a) => {
                feed(&[5]);
                feed(&a.shape_hash().to_le_bytes());
            }
        }
    }
}

pub(super) fn native_size_align(n: Native) -> (usize, usize) {
    match n {
        Native::String => (16, 8),
        Native::Float | Native::Int64 | Native::UInt64 => (8, 8),
        Native::Int | Native::UInt32 | Native::Float32 => (4, 4),
        Native::Int16 | Native::UInt16 => (2, 2),
        Native::Int8 | Native::UInt8 | Native::Bool => (1, 1),
    }
}

pub(super) fn align_up(offset: usize, align: usize) -> usize {
    if align <= 1 {
        offset
    } else {
        (offset + align - 1) & !(align - 1)
    }
}

pub(super) fn struct_size_align(slots: &[AdtSlot], has_discriminant: bool) -> (usize, usize) {
    let mut offset = 0usize;
    let mut max_align = 1usize;
    for slot in slots {
        let (sz, al) = slot.size_align();
        max_align = max_align.max(al);
        offset = align_up(offset, al) + sz;
    }
    if has_discriminant {
        offset += 1;
    }
    (align_up(offset, max_align), max_align)
}

impl AdtShape {
    pub(super) fn has_discriminant(&self) -> bool {
        self.variants.len() > 1
    }

    pub(super) fn has_pointers(&self) -> bool {
        self.slots.iter().any(AdtSlot::has_pointers)
    }

    pub(super) fn size_align(&self) -> (usize, usize) {
        struct_size_align(&self.slots, self.has_discriminant())
    }

    pub(super) fn is_go_comparable(&self) -> bool {
        self.slots.iter().all(AdtSlot::is_go_comparable)
    }

    pub(super) fn shape_hash(&self) -> u64 {
        const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
        const PRIME: u64 = 0x0100_0000_01b3;
        let mut h = OFFSET;
        let mut feed = |bytes: &[u8]| {
            for &b in bytes {
                h ^= u64::from(b);
                h = h.wrapping_mul(PRIME);
            }
        };
        feed(&(self.module.len() as u64).to_le_bytes());
        feed(self.module.as_bytes());
        feed(&[0xff]);
        feed(&(self.name.len() as u64).to_le_bytes());
        feed(self.name.as_bytes());
        feed(&[0xfe, u8::from(self.parameterized)]);
        feed(&(self.variants.len() as u64).to_le_bytes());
        for v in &self.variants {
            feed(&(v.name.len() as u64).to_le_bytes());
            feed(v.name.as_bytes());
            feed(&(v.fields.len() as u64).to_le_bytes());
            for f in &v.fields {
                f.slot.feed_hash(&mut feed);
            }
        }
        h
    }

    pub(super) fn go_type(&self) -> String {
        if self.parameterized {
            names::adt_inst_type(&self.module, &self.name, self.shape_hash())
        } else {
            names::adt_type(&self.module, &self.name)
        }
    }

    pub(super) fn box_fn(&self) -> String {
        format!("Box_{}", self.go_type())
    }

    pub(super) fn unbox_fn(&self) -> String {
        format!("As_{}", self.go_type())
    }

    pub(super) fn eq_fn(&self) -> String {
        format!("Eq_{}", self.go_type())
    }

    pub(super) fn equal_with_fn(&self) -> String {
        format!("EqualWith_{}", self.go_type())
    }

    pub(super) fn cmp_fn(&self) -> String {
        format!("Cmp_{}", self.go_type())
    }

    pub(super) fn ord_fn(&self) -> String {
        format!("Ord_{}", self.go_type())
    }

    pub(super) fn hash_fn(&self) -> String {
        format!("Hash_{}", self.go_type())
    }

    pub(super) fn show_fn(&self) -> String {
        format!("Show_{}", self.go_type())
    }

    pub(super) fn tag_fn(&self) -> String {
        format!("Tag_{}", self.go_type())
    }

    pub(super) fn variant_index(&self, c: &Constructor) -> Option<usize> {
        if c.module != self.module || c.ty != self.name {
            return None;
        }
        self.variants
            .iter()
            .position(|v| v.name == c.name && v.fields.len() == c.arity)
    }

    pub(super) fn field_go_type(&self, vi: usize, field_idx: usize) -> Option<GoType> {
        self.variants
            .get(vi)?
            .fields
            .get(field_idx)?
            .slot
            .to_go_type()
    }

    pub(super) fn slot_index(&self, vi: usize, field_idx: usize) -> Option<usize> {
        self.variants.get(vi)?.fields.get(field_idx)?.pool
    }

    pub(super) fn box_doc<'a>(&self, a: &'a DocumentArena<'a>, v: Document<'a>) -> Document<'a> {
        call(a, a.alloc_str(&self.box_fn()), [v])
    }

    pub(super) fn unbox_doc<'a>(&self, a: &'a DocumentArena<'a>, v: Document<'a>) -> Document<'a> {
        call(a, a.alloc_str(&self.unbox_fn()), [v])
    }

    pub(super) fn eq_doc<'a>(
        &self,
        a: &'a DocumentArena<'a>,
        lhs: Document<'a>,
        rhs: Document<'a>,
    ) -> Document<'a> {
        if self.is_go_comparable() {
            docvec![a, "(", lhs, " == ", rhs, ")"]
        } else {
            call(a, a.alloc_str(&self.eq_fn()), [lhs, rhs])
        }
    }
}
