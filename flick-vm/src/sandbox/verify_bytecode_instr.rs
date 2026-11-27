//! Per-instruction verification checks for [`super::verify_bytecode`].

use std::collections::BTreeSet;

use super::verify_bytecode::BytecodeVerifyError;
use super::verify_bytecode_ctrl::verify_control_and_call_instr;
use crate::tier1::opcode::{CompiledFunction, Instr, Prepared, ValueSource};

pub(super) fn check_slot(
    func_idx: usize,
    pc: usize,
    slots: usize,
    slot: usize,
    role: &str,
) -> Result<(), BytecodeVerifyError> {
    if slot >= slots {
        return Err(BytecodeVerifyError::at(
            func_idx,
            pc,
            format!("{role} slot {slot} out of bounds (slots: {slots})"),
        ));
    }
    Ok(())
}

pub(super) fn check_source(
    func_idx: usize,
    pc: usize,
    slots: usize,
    source: ValueSource,
    role: &str,
) -> Result<(), BytecodeVerifyError> {
    match source {
        ValueSource::Slot(slot) | ValueSource::Cell(slot) => {
            check_slot(func_idx, pc, slots, slot, role)
        }
        ValueSource::Immediate(_) => Ok(()),
    }
}

fn check_tag(
    prepared: &Prepared,
    func_idx: usize,
    pc: usize,
    tag: usize,
) -> Result<(), BytecodeVerifyError> {
    if tag >= prepared.tags.len() {
        return Err(BytecodeVerifyError::at(
            func_idx,
            pc,
            format!(
                "tag index {tag} out of bounds (tags: {})",
                prepared.tags.len()
            ),
        ));
    }
    Ok(())
}

fn check_unique_record_fields(
    func_idx: usize,
    pc: usize,
    slots: usize,
    fields: &[(String, usize)],
) -> Result<(), BytecodeVerifyError> {
    let mut seen = BTreeSet::new();
    for (name, slot) in fields {
        if !seen.insert(name.as_str()) {
            return Err(BytecodeVerifyError::at(
                func_idx,
                pc,
                format!("duplicate record field `{name}`"),
            ));
        }
        check_slot(func_idx, pc, slots, *slot, "record field")?;
    }
    Ok(())
}

pub(super) fn verify_instr(
    prepared: &Prepared,
    func_idx: usize,
    func: &CompiledFunction,
    pc: usize,
    instr: &Instr,
) -> Result<(), BytecodeVerifyError> {
    let slots = func.slots;
    let len = func.instructions.len();
    if let Some(res) = try_verify_slot_instr(func_idx, pc, slots, instr) {
        return res;
    }
    if let Some(res) = try_verify_misc_slot_instr(func_idx, pc, slots, instr) {
        return res;
    }
    if let Some(res) = try_verify_data_instr(prepared, func_idx, pc, slots, instr) {
        return res;
    }
    verify_control_and_call_instr(prepared, func_idx, pc, slots, len, instr)
}

fn try_verify_slot_instr(
    func_idx: usize,
    pc: usize,
    slots: usize,
    instr: &Instr,
) -> Option<Result<(), BytecodeVerifyError>> {
    try_verify_unary_slot_instr(func_idx, pc, slots, instr)
        .or_else(|| try_verify_binary_slot_instr(func_idx, pc, slots, instr))
}

fn try_verify_unary_slot_instr(
    func_idx: usize,
    pc: usize,
    slots: usize,
    instr: &Instr,
) -> Option<Result<(), BytecodeVerifyError>> {
    match instr {
        Instr::Literal { dst, .. }
        | Instr::String { dst, .. }
        | Instr::BigInt { dst, .. }
        | Instr::Label { dst, .. }
        | Instr::ServiceTag { dst, .. }
        | Instr::ReadOwnerEvidence { dst } => Some(check_slot(func_idx, pc, slots, *dst, "dst")),

        Instr::Dup { src, dst }
        | Instr::Not { dst, src }
        | Instr::Neg { dst, src, .. }
        | Instr::HostIntrinsic { dst, src, .. }
        | Instr::DropReuse { dst, object: src }
        | Instr::BoxAggregate {
            dst, value: src, ..
        }
        | Instr::UnboxBorrow { dst, object: src }
        | Instr::UnboxConsume { dst, object: src }
        | Instr::Project {
            dst, object: src, ..
        }
        | Instr::ProjectOwned {
            dst, object: src, ..
        }
        | Instr::Field {
            dst, object: src, ..
        }
        | Instr::Match {
            dst, object: src, ..
        }
        | Instr::MatchFlag {
            dst, object: src, ..
        }
        | Instr::Structure {
            dst, object: src, ..
        }
        | Instr::VarMake { dst, src }
        | Instr::VarGet { dst, var: src }
        | Instr::ReadCell { dst, var: src }
        | Instr::UnwrapOption { dst, option: src }
        | Instr::ScopeRun { dst, body: src }
        | Instr::ScopeCurrent { dst, unit: src }
        | Instr::ScopeAddFinalizer { dst, task: src }
        | Instr::ScopeAddFinalizerExit { dst, func: src }
        | Instr::ScopeAddClose { dst, func: src }
        | Instr::Uninterruptible { dst, task: src }
        | Instr::Await { dst, task: src }
        | Instr::Yield { dst, value: src }
        | Instr::SeqOpen { dst, source: src }
        | Instr::SeqNext { dst, iterator: src }
        | Instr::SeqOfList { dst, list: src }
        | Instr::SeqToList { dst, seq: src }
        | Instr::RefMake { dst, src }
        | Instr::RefGet { dst, cell: src }
        | Instr::ArenaMake { dst, unit: src }
        | Instr::ArenaGet { dst, view: src }
        | Instr::LazyMake { dst, task: src, .. }
        | Instr::LazyForce { dst, lazy: src }
        | Instr::EffectExit { dst, task: src }
        | Instr::FromResult { dst, src } => Some(
            check_slot(func_idx, pc, slots, *dst, "dst")
                .and_then(|()| check_slot(func_idx, pc, slots, *src, "src")),
        ),

        Instr::Drop { src }
        | Instr::Raise { src }
        | Instr::Die { src }
        | Instr::FailureRethrow { src }
        | Instr::Return { value: src } => Some(check_slot(func_idx, pc, slots, *src, "src")),

        _ => None,
    }
}

fn try_verify_binary_slot_instr(
    func_idx: usize,
    pc: usize,
    slots: usize,
    instr: &Instr,
) -> Option<Result<(), BytecodeVerifyError>> {
    match instr {
        Instr::Binary {
            dst, left, right, ..
        }
        | Instr::StringPredicate {
            dst,
            needle: left,
            subject: right,
            ..
        }
        | Instr::Apply {
            dst,
            function: left,
            argument: right,
        }
        | Instr::TailApply {
            dst,
            function: left,
            argument: right,
        }
        | Instr::VarSet {
            dst,
            var: left,
            value: right,
        }
        | Instr::Try {
            dst,
            body: left,
            selector: right,
        }
        | Instr::AcquireRelease {
            dst,
            acquire: left,
            release: right,
        }
        | Instr::RefSet {
            dst,
            cell: left,
            value: right,
        }
        | Instr::RefModify {
            dst,
            cell: left,
            func: right,
        }
        | Instr::RefSame { dst, left, right }
        | Instr::ArenaAlloc {
            dst,
            arena: left,
            value: right,
        }
        | Instr::EffectCatchDefect {
            dst,
            handler: left,
            task: right,
        }
        | Instr::LayerSucceed {
            dst,
            tag: left,
            implementation: right,
        }
        | Instr::LayerMerge { dst, left, right }
        | Instr::LayerRun {
            dst,
            layer: left,
            body: right,
        } => Some(
            check_slot(func_idx, pc, slots, *dst, "dst")
                .and_then(|()| check_slot(func_idx, pc, slots, *left, "left"))
                .and_then(|()| check_slot(func_idx, pc, slots, *right, "right")),
        ),

        _ => None,
    }
}

fn try_verify_misc_slot_instr(
    func_idx: usize,
    pc: usize,
    slots: usize,
    instr: &Instr,
) -> Option<Result<(), BytecodeVerifyError>> {
    match instr {
        Instr::WriteCell { var, value } => Some(
            check_slot(func_idx, pc, slots, *var, "var")
                .and_then(|()| check_slot(func_idx, pc, slots, *value, "value")),
        ),
        Instr::Compute {
            dst, left, right, ..
        } => Some(
            check_slot(func_idx, pc, slots, *dst, "dst")
                .and_then(|()| check_source(func_idx, pc, slots, *left, "left"))
                .and_then(|()| check_source(func_idx, pc, slots, *right, "right")),
        ),
        Instr::UpdateCell { var, right, .. } => Some(
            check_slot(func_idx, pc, slots, *var, "var")
                .and_then(|()| check_source(func_idx, pc, slots, *right, "right")),
        ),
        Instr::Primitive { dst, args, .. } => {
            Some(check_slot(func_idx, pc, slots, *dst, "dst").and_then(|()| {
                args.iter()
                    .try_for_each(|&arg| check_slot(func_idx, pc, slots, arg, "primitive arg"))
            }))
        }
        Instr::Bracket {
            dst,
            acquire,
            use_fn,
            release,
        } => Some(
            check_slot(func_idx, pc, slots, *dst, "dst")
                .and_then(|()| check_slot(func_idx, pc, slots, *acquire, "bracket acquire"))
                .and_then(|()| check_slot(func_idx, pc, slots, *use_fn, "bracket use_fn"))
                .and_then(|()| check_slot(func_idx, pc, slots, *release, "bracket release")),
        ),
        Instr::Recovery { dst, selected } => {
            Some(check_slot(func_idx, pc, slots, *dst, "dst").and_then(|()| {
                selected.map_or(Ok(()), |slot| {
                    check_slot(func_idx, pc, slots, slot, "recovery selected")
                })
            }))
        }
        Instr::MatchFail | Instr::ReturnUnit => Some(Ok(())),
        _ => None,
    }
}

fn verify_construct_fields(
    prepared: &Prepared,
    loc: (usize, usize, usize),
    dst: usize,
    tag: usize,
    fields: &[usize],
    dicts: &[usize],
    kind: &'static str,
) -> Result<(), BytecodeVerifyError> {
    let (func_idx, pc, slots) = loc;
    check_slot(func_idx, pc, slots, dst, "dst")?;
    check_tag(prepared, func_idx, pc, tag)?;
    if fields.len().checked_add(dicts.len()).is_none() {
        return Err(BytecodeVerifyError::at(
            func_idx,
            pc,
            format!("{kind} field count overflow"),
        ));
    }
    for &slot in fields.iter().chain(dicts.iter()) {
        check_slot(func_idx, pc, slots, slot, kind)?;
    }
    Ok(())
}

fn try_verify_data_instr(
    prepared: &Prepared,
    func_idx: usize,
    pc: usize,
    slots: usize,
    instr: &Instr,
) -> Option<Result<(), BytecodeVerifyError>> {
    match instr {
        Instr::Construct {
            dst,
            tag,
            fields,
            dicts,
            ..
        } => Some(verify_construct_fields(
            prepared,
            (func_idx, pc, slots),
            *dst,
            *tag,
            fields,
            dicts,
            "construct operand",
        )),
        Instr::Reuse {
            dst,
            object,
            tag,
            fields,
            dicts,
            ..
        } => Some(
            check_slot(func_idx, pc, slots, *object, "reuse object").and_then(|()| {
                verify_construct_fields(
                    prepared,
                    (func_idx, pc, slots),
                    *dst,
                    *tag,
                    fields,
                    dicts,
                    "reuse operand",
                )
            }),
        ),
        Instr::ReuseRecord {
            dst,
            object,
            fields,
        } => Some(
            check_slot(func_idx, pc, slots, *dst, "dst")
                .and_then(|()| check_slot(func_idx, pc, slots, *object, "reuse record object"))
                .and_then(|()| check_unique_record_fields(func_idx, pc, slots, fields)),
        ),
        Instr::Record { dst, fields } => Some(
            check_slot(func_idx, pc, slots, *dst, "dst")
                .and_then(|()| check_unique_record_fields(func_idx, pc, slots, fields)),
        ),
        Instr::Update {
            dst,
            object,
            labels,
            values,
        } => Some(
            check_slot(func_idx, pc, slots, *dst, "dst")
                .and_then(|()| check_slot(func_idx, pc, slots, *object, "update object"))
                .and_then(|()| {
                    if labels.len() != values.len() {
                        return Err(BytecodeVerifyError::at(
                            func_idx,
                            pc,
                            format!(
                                "update labels length ({}) does not match values length ({})",
                                labels.len(),
                                values.len()
                            ),
                        ));
                    }
                    values
                        .iter()
                        .try_for_each(|&slot| check_slot(func_idx, pc, slots, slot, "update value"))
                }),
        ),
        _ => try_verify_table_instr(prepared, func_idx, pc, slots, instr),
    }
}

fn check_dict_field(
    prepared: &Prepared,
    func_idx: usize,
    pc: usize,
    field_idx: usize,
    kind: &'static str,
) -> Result<(), BytecodeVerifyError> {
    if field_idx >= prepared.dictionary_fields.len() {
        return Err(BytecodeVerifyError::at(
            func_idx,
            pc,
            format!(
                "{kind} field index {field_idx} out of bounds (fields: {})",
                prepared.dictionary_fields.len()
            ),
        ));
    }
    Ok(())
}

fn try_verify_table_instr(
    prepared: &Prepared,
    func_idx: usize,
    pc: usize,
    slots: usize,
    instr: &Instr,
) -> Option<Result<(), BytecodeVerifyError>> {
    match instr {
        Instr::Global { dst, global } => {
            Some(check_slot(func_idx, pc, slots, *dst, "dst").and_then(|()| {
                if *global >= prepared.globals.len() {
                    return Err(BytecodeVerifyError::at(
                        func_idx,
                        pc,
                        format!(
                            "global index {global} out of bounds (globals: {})",
                            prepared.globals.len()
                        ),
                    ));
                }
                Ok(())
            }))
        }
        Instr::Dictionary { dst, entries } => {
            Some(check_slot(func_idx, pc, slots, *dst, "dst").and_then(|()| {
                entries.iter().try_for_each(|&(field_idx, slot, _)| {
                    check_dict_field(prepared, func_idx, pc, field_idx, "dictionary")
                        .and_then(|()| check_slot(func_idx, pc, slots, slot, "dictionary entry"))
                })
            }))
        }
        Instr::Method {
            dst,
            dictionary,
            field,
        } => Some(
            check_slot(func_idx, pc, slots, *dst, "dst")
                .and_then(|()| check_slot(func_idx, pc, slots, *dictionary, "dictionary"))
                .and_then(|()| check_dict_field(prepared, func_idx, pc, *field, "method")),
        ),
        _ => None,
    }
}
