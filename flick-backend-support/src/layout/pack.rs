//! Field packing (with padding-reducing reordering) and ADT discriminant /
//! niche layout construction.

use std::collections::BTreeSet;

use flick_ir::TypeRef;

use super::primitives::{better_niche, discriminant_width, max_for_width};
use super::{
    DataLayout, DictFieldLayout, Discriminant, FieldLayout, FieldStorage, LayoutError, Niche,
    PrimitiveLayout, TypeLayout, TypeRepr, VariantLayout,
};

/// Input descriptor for one item to pack inside a product or variant.
#[derive(Debug, Clone)]
pub(crate) struct PackItem {
    pub size: u64,
    pub align: u64,
    pub niche: Option<Niche>,
}

/// Result of packing a sequence of items after a reserved prefix `[0, prefix)`.
#[derive(Debug, Clone)]
pub(crate) struct PackedProduct {
    /// Byte offset of each input item, in the original semantic order.
    pub offsets: Vec<u64>,
    /// Unaligned end byte (`max(prefix, max_i(offset_i + size_i))`).
    pub end: u64,
    /// Total aligned size (`align_up(end, align)`).
    pub size: u64,
    /// Overall power-of-two alignment.
    pub align: u64,
    /// Best niche propagated from the packed items (shifted to this product's
    /// coordinate frame).
    pub niche: Option<Niche>,
}

/// Rounds `offset` up to the next multiple of `align` (a non-zero power of two).
pub(crate) fn align_up(offset: u64, align: u64) -> Result<u64, LayoutError> {
    debug_assert!(align.is_power_of_two());
    let mask = align - 1;
    offset
        .checked_add(mask)
        .map(|v| v & !mask)
        .ok_or(LayoutError::SizeOverflow)
}

/// Packs `items` after a reserved prefix `[0, prefix)` with minimum alignment
/// `min_align`, reordering fields by `(align desc, size desc, semantic_index asc)`
/// with first-fit placement into alignment gaps to reduce padding while
/// remaining 100% deterministic.
pub(crate) fn pack_items(
    prefix: u64,
    min_align: u64,
    items: &[PackItem],
) -> Result<PackedProduct, LayoutError> {
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by(|&a, &b| {
        items[b]
            .align
            .cmp(&items[a].align)
            .then_with(|| items[b].size.cmp(&items[a].size))
            .then_with(|| a.cmp(&b))
    });

    // Free byte gaps `(start, end)` strictly below `cursor`.
    let mut gaps: Vec<(u64, u64)> = Vec::new();
    let mut cursor = prefix;
    let mut max_align = min_align.max(1);
    let mut offsets = vec![0u64; items.len()];

    for &idx in &order {
        let item = &items[idx];
        max_align = max_align.max(item.align);
        if item.size == 0 {
            // Zero-sized fields occupy no bytes; offset 0 is always aligned
            // and never overlaps any non-empty byte interval.
            offsets[idx] = 0;
            continue;
        }
        if let Some((gap_idx, placed, placed_end)) = find_gap(&gaps, item.size, item.align)? {
            let (g_start, g_end) = gaps[gap_idx];
            offsets[idx] = placed;
            // Replace used gap with up to two remaining sub-gaps.
            gaps.remove(gap_idx);
            let mut insert_at = gap_idx;
            if g_start < placed {
                gaps.insert(insert_at, (g_start, placed));
                insert_at += 1;
            }
            if placed_end < g_end {
                gaps.insert(insert_at, (placed_end, g_end));
            }
        } else {
            let placed = align_up(cursor, item.align)?;
            if placed > cursor {
                gaps.push((cursor, placed));
            }
            let next_cursor = placed
                .checked_add(item.size)
                .ok_or(LayoutError::SizeOverflow)?;
            offsets[idx] = placed;
            cursor = next_cursor;
        }
    }

    let size = align_up(cursor, max_align)?;
    let mut niche = None;
    for (idx, item) in items.iter().enumerate() {
        if let Some(inner) = item.niche {
            let shifted = inner
                .shifted(offsets[idx])
                .ok_or(LayoutError::SizeOverflow)?;
            niche = better_niche(niche, Some(shifted));
        }
    }

    Ok(PackedProduct {
        offsets,
        end: cursor,
        size,
        align: max_align,
        niche,
    })
}

fn find_gap(
    gaps: &[(u64, u64)],
    size: u64,
    align: u64,
) -> Result<Option<(usize, u64, u64)>, LayoutError> {
    for (i, &(start, end)) in gaps.iter().enumerate() {
        let placed = align_up(start, align)?;
        let placed_end = placed.checked_add(size).ok_or(LayoutError::SizeOverflow)?;
        if placed_end <= end {
            return Ok(Some((i, placed, placed_end)));
        }
    }
    Ok(None)
}

/// Prepared field metadata for one constructor before variant packing.
#[derive(Debug, Clone)]
pub(crate) struct PreparedField {
    pub index: usize,
    pub ty: TypeRef,
    pub storage: FieldStorage,
    pub size: u64,
    pub align: u64,
    pub niche: Option<Niche>,
}

/// Prepared constructor metadata before building a [`DataLayout`].
#[derive(Debug, Clone)]
pub(crate) struct PreparedVariant {
    pub name: String,
    pub decl_index: usize,
    pub fields: Vec<PreparedField>,
    pub dict_count: usize,
}

/// Computes the complete [`TypeLayout`] of an algebraic data type given its
/// prepared active constructors.
pub(crate) fn layout_data(
    variants: Vec<PreparedVariant>,
    unboxed: bool,
    pointer_layout: PrimitiveLayout,
) -> Result<TypeLayout, LayoutError> {
    let pointer_fields = collect_pointer_fields(&variants);

    // 0 constructors (e.g. unreachable GADT instantiation).
    if variants.is_empty() {
        return Ok(TypeLayout {
            size: 0,
            align: 1,
            niche: None,
            repr: TypeRepr::Data(DataLayout {
                discriminant: Discriminant::None,
                variants: Vec::new(),
                unboxed: false,
                pointer_fields,
            }),
        });
    }

    // 1 constructor: no discriminant; `unboxed` shares its single field's layout.
    if variants.len() == 1 {
        let v = variants
            .into_iter()
            .next()
            .expect("single-variant slice has one element");
        let packed = pack_variant(&v, 0, 1, pointer_layout)?;
        let variant = build_variant_layout(&v, None, &packed.offsets, pointer_layout);
        return Ok(TypeLayout {
            size: packed.size,
            align: packed.align,
            niche: packed.niche,
            repr: TypeRepr::Data(DataLayout {
                discriminant: Discriminant::None,
                variants: vec![variant],
                unboxed,
                pointer_fields,
            }),
        });
    }

    // Multiple constructors: check whether all are payload-free, whether niche
    // encoding applies, or whether an explicit discriminant tag is required.
    let mut unprefixed = Vec::with_capacity(variants.len());
    for v in &variants {
        unprefixed.push(pack_variant(v, 0, 1, pointer_layout)?);
    }

    if let Some(niche_layout) =
        try_niche_layout(&variants, &unprefixed, &pointer_fields, pointer_layout)?
    {
        return Ok(niche_layout);
    }

    layout_tagged(&variants, pointer_fields, pointer_layout)
}

/// Niche encoding: succeeds when exactly one constructor has non-zero payload
/// size and exposes enough free bit patterns for all remaining variants.
fn try_niche_layout(
    variants: &[PreparedVariant],
    unprefixed: &[PackedProduct],
    pointer_fields: &BTreeSet<(usize, usize)>,
    pointer_layout: PrimitiveLayout,
) -> Result<Option<TypeLayout>, LayoutError> {
    let non_zero: Vec<usize> = unprefixed
        .iter()
        .enumerate()
        .filter_map(|(i, p)| (p.size > 0).then_some(i))
        .collect();
    let [data_idx] = non_zero.as_slice() else {
        return Ok(None);
    };
    let data_packed = &unprefixed[*data_idx];
    let tag_count = (variants.len() - 1) as u64;
    let Some(niche) = data_packed.niche else {
        return Ok(None);
    };
    if niche.count() < u128::from(tag_count) {
        return Ok(None);
    }

    let overall_align = unprefixed
        .iter()
        .map(|p| p.align)
        .max()
        .unwrap_or(1)
        .max(data_packed.align);
    let overall_size = align_up(data_packed.size, overall_align)?;
    let niche_start = niche.start;
    let niche_end = niche_start + (tag_count - 1);
    let remaining = niche.consume(tag_count);
    let mut assigned = 0u64;
    let mut out_variants = Vec::with_capacity(variants.len());
    for (i, v) in variants.iter().enumerate() {
        if i == *data_idx {
            out_variants.push(build_variant_layout(
                v,
                None,
                &data_packed.offsets,
                pointer_layout,
            ));
        } else {
            let tag = niche_start + assigned;
            assigned += 1;
            out_variants.push(build_variant_layout(
                v,
                Some(tag),
                &unprefixed[i].offsets,
                pointer_layout,
            ));
        }
    }
    Ok(Some(TypeLayout {
        size: overall_size,
        align: overall_align,
        niche: remaining,
        repr: TypeRepr::Data(DataLayout {
            discriminant: Discriminant::Niche {
                offset: niche.offset,
                width: niche.width,
                data_variant: *data_idx,
                niche_start,
                niche_end,
            },
            variants: out_variants,
            unboxed: false,
            pointer_fields: pointer_fields.clone(),
        }),
    }))
}

/// Explicit tagged layout (also covers all-fieldless enums like `Ordering`
/// and JS `untagged type` / literal enums).
fn layout_tagged(
    variants: &[PreparedVariant],
    pointer_fields: BTreeSet<(usize, usize)>,
    pointer_layout: PrimitiveLayout,
) -> Result<TypeLayout, LayoutError> {
    let tag_width = discriminant_width(variants.len());
    let mut max_align = tag_width;
    let mut max_end = tag_width;
    let mut out_variants = Vec::with_capacity(variants.len());

    for (i, v) in variants.iter().enumerate() {
        let tag = i as u64;
        if v.fields.is_empty() && v.dict_count == 0 {
            out_variants.push(VariantLayout {
                name: v.name.clone(),
                decl_index: v.decl_index,
                tag: Some(tag),
                fields: Vec::new(),
                dicts: Vec::new(),
            });
        } else {
            let packed = pack_variant(v, tag_width, tag_width, pointer_layout)?;
            max_align = max_align.max(packed.align);
            max_end = max_end.max(packed.end);
            out_variants.push(build_variant_layout(
                v,
                Some(tag),
                &packed.offsets,
                pointer_layout,
            ));
        }
    }

    let total_size = align_up(max_end, max_align)?;
    let max_tag = max_for_width(tag_width).expect("discriminant width is 1, 2, 4, or 8");
    let tag_niche = if (variants.len() as u128) <= u128::from(max_tag) {
        Some(Niche {
            offset: 0,
            width: tag_width,
            start: variants.len() as u64,
            end: max_tag,
        })
    } else {
        None
    };

    Ok(TypeLayout {
        size: total_size,
        align: max_align,
        niche: tag_niche,
        repr: TypeRepr::Data(DataLayout {
            discriminant: Discriminant::Tag {
                offset: 0,
                width: tag_width,
            },
            variants: out_variants,
            unboxed: false,
            pointer_fields,
        }),
    })
}

fn pack_variant(
    v: &PreparedVariant,
    prefix: u64,
    min_align: u64,
    pointer_layout: PrimitiveLayout,
) -> Result<PackedProduct, LayoutError> {
    let mut items = Vec::with_capacity(v.fields.len() + v.dict_count);
    for f in &v.fields {
        items.push(PackItem {
            size: f.size,
            align: f.align,
            niche: f.niche,
        });
    }
    for _ in 0..v.dict_count {
        items.push(PackItem {
            size: pointer_layout.size,
            align: pointer_layout.align,
            niche: pointer_layout.niche,
        });
    }
    pack_items(prefix, min_align, &items)
}

fn build_variant_layout(
    v: &PreparedVariant,
    tag: Option<u64>,
    offsets: &[u64],
    pointer_layout: PrimitiveLayout,
) -> VariantLayout {
    let fields = v
        .fields
        .iter()
        .enumerate()
        .map(|(i, f)| FieldLayout {
            index: f.index,
            offset: offsets[i],
            size: f.size,
            align: f.align,
            ty: f.ty.clone(),
            storage: f.storage,
        })
        .collect();
    let dicts = (0..v.dict_count)
        .map(|d| DictFieldLayout {
            index: d,
            offset: offsets[v.fields.len() + d],
            size: pointer_layout.size,
            align: pointer_layout.align,
        })
        .collect();
    VariantLayout {
        name: v.name.clone(),
        decl_index: v.decl_index,
        tag,
        fields,
        dicts,
    }
}

fn collect_pointer_fields(variants: &[PreparedVariant]) -> BTreeSet<(usize, usize)> {
    let mut set = BTreeSet::new();
    for (v_idx, v) in variants.iter().enumerate() {
        for f in &v.fields {
            if f.storage == FieldStorage::Pointer {
                set.insert((v_idx, f.index));
            }
        }
    }
    set
}
