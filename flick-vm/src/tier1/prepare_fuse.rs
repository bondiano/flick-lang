//! Intra-block bytecode peephole fusion for tier 1.
//!
//! Preserves branch targets and exact evaluation order while fusing:
//! - immediate literals and frame-cell reads feeding binary operations (`Compute`)
//! - binary comparisons immediately feeding a conditional branch (`CompareBranch`)
//! - `Dup` + `VarGet` (`ReadCell`), `VarSet` + `Drop` (`WriteCell`), and
//!   in-place arithmetic cell updates (`UpdateCell`)
//! - `Project` + `Drop(object)` (`ProjectOwned`)
//! - binding-free `Match` + `ProjectOwned(_, 0)` (`MatchFlag`) and + `Branch` (`MatchBranch`)
//! - `Literal(Unit)` + `Return` (`ReturnUnit`)
//! - `Call` (<= `MAX_TAIL_ARGS`) / `Apply` immediately followed by `Return` (`TailCall` / `TailApply`)

use std::mem;

use flick_core::ir::BinOp;
use flick_core::ir::memory::{self, MemorySpan};

use super::opcode::{Immediate, Instr, MAX_TAIL_ARGS, ValueSource};

pub(super) fn fuse(
    instructions: Vec<Instr>,
    spans: Vec<Option<MemorySpan>>,
) -> (Vec<Instr>, Vec<Option<MemorySpan>>) {
    debug_assert_eq!(instructions.len(), spans.len());
    let mut targets = vec![None; instructions.len()];
    for instr in &instructions {
        if let Instr::Branch {
            then_pc, else_pc, ..
        }
        | Instr::CompareBranch {
            then_pc, else_pc, ..
        }
        | Instr::MatchBranch {
            then_pc, else_pc, ..
        } = instr
        {
            targets[*then_pc] = Some(0);
            targets[*else_pc] = Some(0);
        }
    }

    let mut out = Vec::with_capacity(instructions.len());
    let mut out_spans = Vec::with_capacity(spans.len());
    let mut block_start = 0;
    for (pc, (instr, span)) in instructions.into_iter().zip(spans).enumerate() {
        if let Some(target) = &mut targets[pc] {
            // A jump target starts a new fusion window and keeps its entry PC.
            *target = out.len();
            block_start = out.len();
        }
        out.push(instr);
        out_spans.push(span);
        loop {
            if let [.., first, second] = &out[block_start..]
                && let Some(fused) = fuse_pair(first, second)
            {
                let second_span = out_spans.pop().expect("second span");
                let first_span = out_spans.pop().expect("first span");
                out.truncate(out.len() - 2);
                out.push(fused);
                out_spans.push(second_span.or(first_span));
                continue;
            }
            if let [.., first, middle, third] = &out[block_start..]
                && let Some((a, b)) = fuse_triple(first, middle, third)
            {
                let third_span = out_spans.pop().expect("third span");
                let middle_span = out_spans.pop().expect("middle span");
                let first_span = out_spans.pop().expect("first span");
                out.truncate(out.len() - 3);
                out.push(a);
                out.push(b);
                out_spans.push(first_span.or(third_span));
                out_spans.push(middle_span);
                continue;
            }
            break;
        }
    }

    for instr in &mut out {
        if let Instr::Branch {
            then_pc, else_pc, ..
        }
        | Instr::CompareBranch {
            then_pc, else_pc, ..
        }
        | Instr::MatchBranch {
            then_pc, else_pc, ..
        } = instr
        {
            *then_pc = targets[*then_pc].expect("recorded then branch target");
            *else_pc = targets[*else_pc].expect("recorded else branch target");
        }
    }

    mark_tail_calls(&mut out);
    (out, out_spans)
}

fn mark_tail_calls(out: &mut [Instr]) {
    let mut rest = out;
    while let [call, next @ ..] = rest {
        if let [Instr::Return { value }, ..] = &*next {
            match call {
                Instr::Call { dst, code, args } if dst == value && args.len() <= MAX_TAIL_ARGS => {
                    *call = Instr::TailCall {
                        dst: *dst,
                        code: *code,
                        args: mem::take(args),
                    };
                }
                Instr::Apply {
                    dst,
                    function,
                    argument,
                } if dst == value => {
                    *call = Instr::TailApply {
                        dst: *dst,
                        function: *function,
                        argument: *argument,
                    };
                }
                _ => {}
            }
        }
        rest = next;
    }
}

fn fuse_triple(first: &Instr, middle: &Instr, third: &Instr) -> Option<(Instr, Instr)> {
    match (first, middle, third) {
        (
            Instr::Match {
                dst: match_dst,
                object,
                pattern,
            },
            Instr::Drop { src: dropped },
            Instr::ProjectOwned {
                dst: flag_dst,
                object: proj_obj,
                field: 0,
            },
        ) if match_dst == proj_obj
            && dropped != match_dst
            && dropped != flag_dst
            && memory::pattern_bindings(pattern).is_ok_and(|names| names.is_empty()) =>
        {
            Some((
                Instr::MatchFlag {
                    dst: *flag_dst,
                    object: *object,
                    pattern: pattern.clone(),
                },
                Instr::Drop { src: *dropped },
            ))
        }
        _ => None,
    }
}

#[allow(clippy::too_many_lines)]
fn fuse_pair(first: &Instr, second: &Instr) -> Option<Instr> {
    if let Some((dst, op, mut left, mut right)) = binary(second) {
        match *first {
            Instr::Literal {
                dst: lit_dst,
                value,
            } => match (uses_slot(left, lit_dst), uses_slot(right, lit_dst)) {
                (true, false) => left = ValueSource::Immediate(value),
                (false, true) => right = ValueSource::Immediate(value),
                _ => return None,
            },
            Instr::ReadCell { dst: cell_dst, var } => {
                match (uses_slot(left, cell_dst), uses_slot(right, cell_dst)) {
                    (true, false) => left = ValueSource::Cell(var),
                    (false, true) => right = ValueSource::Cell(var),
                    _ => return None,
                }
            }
            _ => return None,
        }
        return Some(Instr::Compute {
            dst,
            op,
            left,
            right,
        });
    }

    match (first, second) {
        (Instr::Project { dst, object, field }, Instr::Drop { src })
            if object == src && dst != object =>
        {
            Some(Instr::ProjectOwned {
                dst: *dst,
                object: *object,
                field: *field,
            })
        }
        (
            Instr::Match {
                dst: match_dst,
                object,
                pattern,
            },
            Instr::ProjectOwned {
                dst: flag_dst,
                object: proj_obj,
                field: 0,
            },
        ) if match_dst == proj_obj
            && memory::pattern_bindings(pattern).is_ok_and(|names| names.is_empty()) =>
        {
            Some(Instr::MatchFlag {
                dst: *flag_dst,
                object: *object,
                pattern: pattern.clone(),
            })
        }
        (
            Instr::MatchFlag {
                dst: flag_dst,
                object,
                pattern,
            },
            Instr::Branch {
                condition,
                then_pc,
                else_pc,
            },
        ) if flag_dst == condition => Some(Instr::MatchBranch {
            object: *object,
            pattern: pattern.clone(),
            then_pc: *then_pc,
            else_pc: *else_pc,
        }),
        (Instr::Dup { src, dst: copied }, Instr::VarGet { dst, var }) if copied == var => {
            Some(Instr::ReadCell {
                dst: *dst,
                var: *src,
            })
        }
        (Instr::VarSet { dst, var, value }, Instr::Drop { src }) if src == dst => {
            Some(Instr::WriteCell {
                var: *var,
                value: *value,
            })
        }
        (
            Instr::Compute {
                dst,
                op,
                left: ValueSource::Cell(source),
                right,
            },
            Instr::WriteCell { var, value },
        ) if dst == value && source == var && !uses_slot(*right, *var) => Some(Instr::UpdateCell {
            var: *var,
            op: *op,
            right: *right,
        }),
        (
            first,
            Instr::Branch {
                condition,
                then_pc,
                else_pc,
            },
        ) => {
            let (dst, op, left, right) = binary(first)?;
            (dst == *condition).then_some(Instr::CompareBranch {
                op,
                left,
                right,
                then_pc: *then_pc,
                else_pc: *else_pc,
            })
        }
        (
            Instr::Literal {
                dst,
                value: Immediate::Unit,
            },
            Instr::Return { value },
        ) if dst == value => Some(Instr::ReturnUnit),
        _ => None,
    }
}

fn binary(instr: &Instr) -> Option<(usize, BinOp, ValueSource, ValueSource)> {
    match *instr {
        Instr::Binary {
            dst,
            op,
            left,
            right,
        } => Some((dst, op, ValueSource::Slot(left), ValueSource::Slot(right))),
        Instr::Compute {
            dst,
            op,
            left,
            right,
        } => Some((dst, op, left, right)),
        _ => None,
    }
}

fn uses_slot(source: ValueSource, slot: usize) -> bool {
    matches!(source, ValueSource::Slot(src) if src == slot)
}
