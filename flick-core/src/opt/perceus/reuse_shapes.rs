//! Static candidate/allocation shape lattice for Stage 2 FBIP reuse.

use crate::ir::memory::CellLayout;
use crate::ir::{Pat, Rep};

/// Static knowledge of a heap cell's optional [`CellLayout`] descriptor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LayoutKnowledge {
    /// Layout descriptor not known statically; defer compatibility check to runtime.
    Unknown,
    /// Exact layout descriptor known statically from construction/reuse site.
    Known(Option<CellLayout>),
}

/// Static allocation/layout knowledge for a local, used only to avoid emitting
/// unproductive `DropReuse` operations and to match capacities and cell layouts
/// accurately. Runtime uniqueness and layout checks remain mandatory for soundness.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CandidateShape {
    /// Statically known non-reusable (immediate, string, closure, task, `$match`, etc.).
    Never,
    /// Tagged constructor/tuple/list with exact physical field count (`ordinary + dicts`)
    /// and optional statically known [`CellLayout`].
    TaggedExact(usize, LayoutKnowledge),
    /// Tagged constructor/tuple with dynamically determined field count.
    TaggedAny,
    /// Record with exact field count.
    RecordExact(usize),
    /// Record with dynamically determined field count.
    RecordAny,
    /// Unknown shape; not speculatively converted to `DropReuse`.
    Unknown,
}

impl CandidateShape {
    pub(crate) fn matches_alloc(&self, alloc: &AllocShape) -> bool {
        match (self, alloc) {
            (
                Self::TaggedExact(cap, LayoutKnowledge::Known(cand_lay)),
                AllocShape::Tagged(needed, alloc_lay),
            ) => cap == needed && cand_lay == alloc_lay,
            (Self::TaggedExact(cap, LayoutKnowledge::Unknown), AllocShape::Tagged(needed, _))
            | (Self::RecordExact(cap), AllocShape::Record(needed)) => cap == needed,
            (Self::TaggedAny, AllocShape::Tagged(_, _))
            | (Self::RecordAny, AllocShape::Record(_)) => true,
            _ => false,
        }
    }

    pub(crate) fn refine(existing: Self, incoming: Self) -> Self {
        match (existing, incoming) {
            (Self::Never, _) | (_, Self::Never) => Self::Never,
            (Self::TaggedExact(n, LayoutKnowledge::Known(lay)), _)
            | (_, Self::TaggedExact(n, LayoutKnowledge::Known(lay))) => {
                Self::TaggedExact(n, LayoutKnowledge::Known(lay))
            }
            (Self::TaggedExact(n, LayoutKnowledge::Unknown), _)
            | (_, Self::TaggedExact(n, LayoutKnowledge::Unknown)) => {
                Self::TaggedExact(n, LayoutKnowledge::Unknown)
            }
            (Self::RecordExact(n), _) | (_, Self::RecordExact(n)) => Self::RecordExact(n),
            (Self::TaggedAny, _) | (_, Self::TaggedAny) => Self::TaggedAny,
            (Self::RecordAny, _) | (_, Self::RecordAny) => Self::RecordAny,
            (Self::Unknown, Self::Unknown) => Self::Unknown,
        }
    }

    pub(crate) fn join(self, other: Self) -> Self {
        match (self, other) {
            (Self::TaggedExact(a, la), Self::TaggedExact(b, lb)) if a == b => {
                let lay = if la == lb {
                    la
                } else {
                    LayoutKnowledge::Unknown
                };
                Self::TaggedExact(a, lay)
            }
            (
                Self::TaggedExact(_, _) | Self::TaggedAny,
                Self::TaggedExact(_, _) | Self::TaggedAny,
            ) => Self::TaggedAny,
            (Self::RecordExact(a), Self::RecordExact(b)) if a == b => Self::RecordExact(a),
            (Self::RecordExact(_) | Self::RecordAny, Self::RecordExact(_) | Self::RecordAny) => {
                Self::RecordAny
            }
            (Self::Never, Self::Never) => Self::Never,
            _ => Self::Unknown,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AllocShape {
    Tagged(usize, Option<CellLayout>),
    Record(usize),
}

pub(crate) fn pattern_shape(pat: &Pat) -> CandidateShape {
    match pat {
        Pat::Con(con, args) => match &con.rep {
            Rep::Tagged => {
                CandidateShape::TaggedExact(con.arity + con.dicts, LayoutKnowledge::Unknown)
            }
            Rep::Literal(_) => CandidateShape::Never,
            Rep::Unboxed | Rep::Typeof(_) if con.arity == 1 && con.dicts == 0 => {
                args.first().map_or(CandidateShape::Unknown, pattern_shape)
            }
            _ => CandidateShape::Unknown,
        },
        Pat::Tuple(items) if items.is_empty() => CandidateShape::Never,
        Pat::Tuple(items) | Pat::List(items, None) => {
            CandidateShape::TaggedExact(items.len(), LayoutKnowledge::Unknown)
        }
        Pat::List(_, Some(_)) | Pat::Wild | Pat::Bind(_) => CandidateShape::Unknown,
        Pat::Record(_) => CandidateShape::RecordAny,
        Pat::Lit(_) => CandidateShape::Never,
        Pat::Or(alts) => {
            let mut iter = alts.iter().map(pattern_shape);
            let Some(first) = iter.next() else {
                return CandidateShape::Unknown;
            };
            iter.fold(first, CandidateShape::join)
        }
    }
}

/// Computes the combined scrutinee shape across all arms of a `match` expression,
/// allowing wildcard fallback arms to dynamically reuse ADT/record scrutinees
/// without emitting speculative `DropReuse` on scalar/literal matches.
pub(crate) fn arms_scrutinee_shape<'a>(pats: impl IntoIterator<Item = &'a Pat>) -> CandidateShape {
    let mut acc: Option<CandidateShape> = None;
    for pat in pats {
        let shape = pattern_shape(pat);
        if shape == CandidateShape::Unknown {
            continue;
        }
        acc = Some(match acc {
            None => shape,
            Some(prev) => prev.join(shape),
        });
    }
    match acc {
        Some(CandidateShape::TaggedExact(_, _) | CandidateShape::TaggedAny) => {
            CandidateShape::TaggedAny
        }
        Some(CandidateShape::RecordExact(_) | CandidateShape::RecordAny) => {
            CandidateShape::RecordAny
        }
        Some(CandidateShape::Never) => CandidateShape::Never,
        _ => CandidateShape::Unknown,
    }
}
