//! Frame-limited functional in-place (FBIP) reuse pass over token-balanced memory IR.
//!
//! Transforms dead `Drop(x)` instructions into `r = DropReuse(x)` and pairs `r`
//! with the first layout-compatible allocation (`Construct`, `ConstructWithDicts`,
//! or `Record`) reachable along each control-flow path in the same function frame,
//! inserting `Drop(r)` on branches that do not allocate.
//!
//! Because `DropReuse(x)` drops `x`'s old children immediately at the original
//! `Drop(x)` site (when unique) and keeps only an empty 0-child reuse token in
//! the frame, this preserves observable drop/cleanup order, restores child
//! uniqueness before recursive calls, keeps space bounded by frame depth, and
//! guarantees zero leaks and zero double-frees on early failure, abort, or fuel cut.
//!
//! Token-balance preservation for this transformation is mechanized in
//! `formal/Formal/MemoryReuse.lean`.

use std::collections::BTreeMap;
use std::mem;

use crate::ir::memory::{Block, Exit, Function, Instruction, Local, Operation, ValueRep};
use crate::ir::{Pat, Rep};

/// Static allocation/layout knowledge for a local, used only to avoid emitting
/// unproductive `DropReuse` operations and to match capacities accurately.
/// Runtime uniqueness and layout checks remain mandatory for soundness.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CandidateShape {
    /// Statically known non-reusable (immediate, string, closure, task, `$match`, etc.).
    Never,
    /// Tagged constructor/tuple/list with exact physical field count (`ordinary + dicts`).
    TaggedExact(usize),
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
    pub(crate) fn matches_alloc(self, alloc: AllocShape) -> bool {
        match (self, alloc) {
            (Self::TaggedExact(cap), AllocShape::Tagged(needed))
            | (Self::RecordExact(cap), AllocShape::Record(needed)) => cap == needed,
            (Self::TaggedAny, AllocShape::Tagged(_)) | (Self::RecordAny, AllocShape::Record(_)) => {
                true
            }
            _ => false,
        }
    }

    fn refine(existing: Self, incoming: Self) -> Self {
        match (existing, incoming) {
            (Self::Never, _) | (_, Self::Never) => Self::Never,
            (Self::TaggedExact(n), _) | (_, Self::TaggedExact(n)) => Self::TaggedExact(n),
            (Self::RecordExact(n), _) | (_, Self::RecordExact(n)) => Self::RecordExact(n),
            (Self::TaggedAny, _) | (_, Self::TaggedAny) => Self::TaggedAny,
            (Self::RecordAny, _) | (_, Self::RecordAny) => Self::RecordAny,
            (Self::Unknown, Self::Unknown) => Self::Unknown,
        }
    }

    pub(crate) fn join(self, other: Self) -> Self {
        match (self, other) {
            (Self::TaggedExact(a), Self::TaggedExact(b)) if a == b => Self::TaggedExact(a),
            (Self::TaggedExact(_) | Self::TaggedAny, Self::TaggedExact(_) | Self::TaggedAny) => {
                Self::TaggedAny
            }
            (Self::RecordExact(a), Self::RecordExact(b)) if a == b => Self::RecordExact(a),
            (Self::RecordExact(_) | Self::RecordAny, Self::RecordExact(_) | Self::RecordAny) => {
                Self::RecordAny
            }
            (Self::Never, Self::Never) => Self::Never,
            _ => Self::Unknown,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AllocShape {
    Tagged(usize),
    Record(usize),
}

pub(crate) fn pattern_shape(pat: &Pat) -> CandidateShape {
    match pat {
        Pat::Con(con, args) => match &con.rep {
            Rep::Tagged => CandidateShape::TaggedExact(con.arity + con.dicts),
            Rep::Literal(_) => CandidateShape::Never,
            Rep::Unboxed | Rep::Typeof(_) if con.arity == 1 && con.dicts == 0 => {
                args.first().map_or(CandidateShape::Unknown, pattern_shape)
            }
            _ => CandidateShape::Unknown,
        },
        Pat::Tuple(items) if items.is_empty() => CandidateShape::Never,
        Pat::Tuple(items) | Pat::List(items, None) => CandidateShape::TaggedExact(items.len()),
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
        Some(CandidateShape::TaggedExact(_) | CandidateShape::TaggedAny) => {
            CandidateShape::TaggedAny
        }
        Some(CandidateShape::RecordExact(_) | CandidateShape::RecordAny) => {
            CandidateShape::RecordAny
        }
        Some(CandidateShape::Never) => CandidateShape::Never,
        _ => CandidateShape::Unknown,
    }
}

/// Applies frame-limited FBIP reuse to a validated memory-IR function, using
/// optional parameter shape hints (for example, marking closure/task `self`
/// non-reusable or providing a `match` scrutinee's aggregate kind).
pub(crate) fn optimize_function(
    function: &Function,
    param_hints: &BTreeMap<Local, CandidateShape>,
) -> Function {
    let mut next_local = max_local_in_function(function).and_then(|m| m.checked_add(1));
    let mut body = function.body.clone();
    let mut shapes = param_hints.clone();
    let matches = BTreeMap::new();
    let match_conds = BTreeMap::new();
    optimize_block(
        &mut body,
        &mut shapes,
        &matches,
        &match_conds,
        &mut next_local,
    );
    Function {
        params: function.params.clone(),
        body,
    }
}

fn max_local_in_function(function: &Function) -> Option<Local> {
    let mut max = function.params.iter().copied().max();
    max_local_in_block(&function.body, &mut max);
    max
}

fn max_local_in_block(block: &Block, max: &mut Option<Local>) {
    let mut bump = |local: Local| {
        *max = Some(max.map_or(local, |current| current.max(local)));
    };
    for instruction in &block.instructions {
        match instruction {
            Instruction::Let { binding, value } => {
                bump(*binding);
                for local in value.consumed().into_iter().chain(value.borrowed()) {
                    bump(local);
                }
            }
            Instruction::Dup(local) | Instruction::Drop(local) => bump(*local),
        }
    }
    match &block.exit {
        Exit::Return(local) => bump(*local),
        Exit::Branch {
            condition,
            then_,
            else_,
        } => {
            bump(*condition);
            max_local_in_block(then_, max);
            max_local_in_block(else_, max);
        }
    }
}

fn operation_result_shape(
    value: &Operation,
    shapes: &BTreeMap<Local, CandidateShape>,
) -> CandidateShape {
    match value {
        Operation::Construct {
            fields,
            rep: ValueRep::Cell,
            ..
        }
        | Operation::Reuse { fields, .. } => CandidateShape::TaggedExact(fields.len()),
        Operation::ConstructWithDicts {
            fields,
            dicts,
            rep: ValueRep::Cell,
            ..
        }
        | Operation::ReuseWithDicts { fields, dicts, .. } => {
            CandidateShape::TaggedExact(fields.len() + dicts.len())
        }
        Operation::Record { fields } | Operation::ReuseRecord { fields, .. } => {
            CandidateShape::RecordExact(fields.len())
        }
        Operation::Update { object, .. } => match shapes.get(object).copied() {
            Some(CandidateShape::RecordExact(n)) => CandidateShape::RecordExact(n),
            _ => CandidateShape::RecordAny,
        },
        Operation::Construct {
            rep: ValueRep::Aggregate,
            ..
        }
        | Operation::ConstructWithDicts {
            rep: ValueRep::Aggregate,
            ..
        }
        | Operation::BoxAggregate { .. }
        | Operation::UnboxBorrow { .. }
        | Operation::UnboxConsume { .. }
        | Operation::Literal(_)
        | Operation::Closure { .. }
        | Operation::Task { .. }
        | Operation::Dictionary { .. }
        | Operation::ServiceTag(_)
        | Operation::Label(_)
        | Operation::VarMake(_)
        | Operation::RecoveryResult { .. }
        | Operation::Binary { .. }
        | Operation::Not(_)
        | Operation::Neg { .. }
        | Operation::SeqOpen(_)
        | Operation::GenerateLazy { .. }
        | Operation::ReadOwnerEvidence
        | Operation::Match { .. }
        | Operation::DropReuse(_)
        | Operation::Structure { .. } => CandidateShape::Never,
        _ => CandidateShape::Unknown,
    }
}

fn alloc_shape(op: &Operation) -> Option<AllocShape> {
    match op {
        Operation::Construct {
            fields,
            rep: ValueRep::Cell,
            ..
        } => Some(AllocShape::Tagged(fields.len())),
        Operation::ConstructWithDicts {
            fields,
            dicts,
            rep: ValueRep::Cell,
            ..
        } => Some(AllocShape::Tagged(fields.len() + dicts.len())),
        Operation::Record { fields } => Some(AllocShape::Record(fields.len())),
        _ => None,
    }
}

fn has_compatible_alloc_slice(
    instructions: &[Instruction],
    exit: &Exit,
    shape: CandidateShape,
) -> bool {
    for instruction in instructions {
        if let Instruction::Let { value, .. } = instruction
            && let Some(alloc) = alloc_shape(value)
            && shape.matches_alloc(alloc)
        {
            return true;
        }
    }
    match exit {
        Exit::Return(_) => false,
        Exit::Branch { then_, else_, .. } => {
            has_compatible_alloc_block(then_, shape) || has_compatible_alloc_block(else_, shape)
        }
    }
}

fn has_compatible_alloc_block(block: &Block, shape: CandidateShape) -> bool {
    has_compatible_alloc_slice(&block.instructions, &block.exit, shape)
}

/// Consumes the fresh reuse token `token` along every path through `(instructions, exit)`.
/// On paths with a compatible allocation, rewrites the first such allocation to a `Reuse*`
/// operation; on paths with no compatible allocation, prepends `Drop(token)` immediately.
fn pair_reuse_tail(
    instructions: &mut [Instruction],
    exit: &mut Exit,
    token: Local,
    shape: CandidateShape,
) {
    for instruction in instructions.iter_mut() {
        if let Instruction::Let { value, .. } = instruction
            && let Some(alloc) = alloc_shape(value)
            && shape.matches_alloc(alloc)
        {
            rewrite_alloc_to_reuse(value, token);
            return;
        }
    }
    if let Exit::Branch { then_, else_, .. } = exit {
        let then_has = has_compatible_alloc_block(then_, shape);
        let else_has = has_compatible_alloc_block(else_, shape);
        debug_assert!(
            then_has || else_has,
            "pair_reuse_tail is called only when a compatible allocation is reachable"
        );
        if then_has {
            pair_reuse_block(then_, token, shape);
        } else {
            then_.instructions.insert(0, Instruction::Drop(token));
        }
        if else_has {
            pair_reuse_block(else_, token, shape);
        } else {
            else_.instructions.insert(0, Instruction::Drop(token));
        }
    } else {
        unreachable!("pair_reuse_tail called without reachable compatible allocation");
    }
}

fn pair_reuse_block(block: &mut Block, token: Local, shape: CandidateShape) {
    for instruction in &mut block.instructions {
        if let Instruction::Let { value, .. } = instruction
            && let Some(alloc) = alloc_shape(value)
            && shape.matches_alloc(alloc)
        {
            rewrite_alloc_to_reuse(value, token);
            return;
        }
    }
    match &mut block.exit {
        Exit::Return(_) => {
            block.instructions.insert(0, Instruction::Drop(token));
        }
        Exit::Branch { then_, else_, .. } => {
            let then_has = has_compatible_alloc_block(then_, shape);
            let else_has = has_compatible_alloc_block(else_, shape);
            if then_has || else_has {
                if then_has {
                    pair_reuse_block(then_, token, shape);
                } else {
                    then_.instructions.insert(0, Instruction::Drop(token));
                }
                if else_has {
                    pair_reuse_block(else_, token, shape);
                } else {
                    else_.instructions.insert(0, Instruction::Drop(token));
                }
            } else {
                block.instructions.insert(0, Instruction::Drop(token));
            }
        }
    }
}

fn rewrite_alloc_to_reuse(value: &mut Operation, token: Local) {
    let old = mem::replace(value, Operation::ReadOwnerEvidence);
    *value = match old {
        Operation::Construct {
            tag,
            fields,
            rep: ValueRep::Cell,
            layout,
        } => Operation::Reuse {
            object: token,
            tag,
            fields,
            rep: ValueRep::Cell,
            layout,
        },
        Operation::ConstructWithDicts {
            tag,
            fields,
            dicts,
            rep: ValueRep::Cell,
            layout,
        } => Operation::ReuseWithDicts {
            object: token,
            tag,
            fields,
            dicts,
            rep: ValueRep::Cell,
            layout,
        },
        Operation::Record { fields } => Operation::ReuseRecord {
            object: token,
            fields,
        },
        other => other,
    };
}

fn record_let_shapes(
    binding: Local,
    value: &Operation,
    shapes: &mut BTreeMap<Local, CandidateShape>,
    matches: &mut BTreeMap<Local, (Local, CandidateShape)>,
    match_conds: &mut BTreeMap<Local, Local>,
) {
    match value {
        Operation::Field { object, field } if field != "_tag" => {
            let current = shapes
                .get(object)
                .copied()
                .unwrap_or(CandidateShape::Unknown);
            shapes.insert(
                *object,
                CandidateShape::refine(current, CandidateShape::RecordAny),
            );
        }
        Operation::Project { object, field, rep } => {
            if rep.is_aggregate() {
                shapes.insert(*object, CandidateShape::Never);
            } else if *field == 0
                && let Some(&(scrutinee, _)) = matches.get(object)
            {
                let _ = scrutinee;
                match_conds.insert(binding, *object);
            } else if !matches.contains_key(object) {
                let current = shapes
                    .get(object)
                    .copied()
                    .unwrap_or(CandidateShape::Unknown);
                shapes.insert(
                    *object,
                    CandidateShape::refine(current, CandidateShape::TaggedAny),
                );
            }
        }
        Operation::Match {
            object,
            pattern,
            rep,
        } => {
            if rep.is_aggregate() {
                shapes.insert(*object, CandidateShape::Never);
                matches.insert(binding, (*object, CandidateShape::Never));
            } else {
                matches.insert(binding, (*object, pattern_shape(pattern)));
            }
        }
        Operation::BoxAggregate { value, .. } => {
            shapes.insert(*value, CandidateShape::Never);
        }
        Operation::UnboxBorrow { object } | Operation::UnboxConsume { object } => {
            shapes.insert(*object, CandidateShape::Never);
        }
        _ => {}
    }
    let result_shape = operation_result_shape(value, shapes);
    shapes.insert(binding, result_shape);
}

fn optimize_block(
    block: &mut Block,
    shapes: &mut BTreeMap<Local, CandidateShape>,
    outer_matches: &BTreeMap<Local, (Local, CandidateShape)>,
    outer_conds: &BTreeMap<Local, Local>,
    next_local: &mut Option<Local>,
) {
    let mut matches = outer_matches.clone();
    let mut match_conds = outer_conds.clone();

    for index in 0..block.instructions.len() {
        match &block.instructions[index] {
            Instruction::Let { binding, value } => {
                record_let_shapes(*binding, value, shapes, &mut matches, &mut match_conds);
            }
            Instruction::Drop(local) => {
                let local = *local;
                let shape = shapes
                    .get(&local)
                    .copied()
                    .unwrap_or(CandidateShape::Unknown);
                let eligible = !matches!(shape, CandidateShape::Never | CandidateShape::Unknown);
                if eligible
                    && has_compatible_alloc_slice(
                        &block.instructions[index + 1..],
                        &block.exit,
                        shape,
                    )
                    && let Some(token) = *next_local
                {
                    *next_local = token.checked_add(1);
                    block.instructions[index] = Instruction::Let {
                        binding: token,
                        value: Operation::DropReuse(local),
                    };
                    shapes.insert(token, CandidateShape::Never);
                    pair_reuse_tail(
                        &mut block.instructions[index + 1..],
                        &mut block.exit,
                        token,
                        shape,
                    );
                }
            }
            Instruction::Dup(_) => {}
        }
    }

    if let Exit::Branch {
        condition,
        then_,
        else_,
    } = &mut block.exit
    {
        let mut then_shapes = shapes.clone();
        if let Some(matched) = match_conds.get(condition)
            && let Some(&(scrutinee, pat_shape)) = matches.get(matched)
            && pat_shape != CandidateShape::Unknown
        {
            let current = then_shapes
                .get(&scrutinee)
                .copied()
                .unwrap_or(CandidateShape::Unknown);
            then_shapes.insert(scrutinee, CandidateShape::refine(current, pat_shape));
        }
        let mut else_shapes = shapes.clone();
        optimize_block(then_, &mut then_shapes, &matches, &match_conds, next_local);
        optimize_block(else_, &mut else_shapes, &matches, &match_conds, next_local);
    }
}
