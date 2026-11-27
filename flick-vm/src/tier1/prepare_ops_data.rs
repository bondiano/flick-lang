//! Data, reuse, aggregate, dictionary, variable, and loop translators for tier 1.

use flick_core::ir::Pat;
use flick_core::ir::memory::{CellLayout, CodeId, DictionaryEntry, Local, MemorySpan, ValueRep};

use super::opcode::{Instr, PrepareError};
use super::prepare::{Lowering, Tokens};
use super::prepare_ops::unsupported;

impl Lowering<'_> {
    pub(super) fn translate_construct(
        &mut self,
        tag: &str,
        fields: &[Local],
        rep: ValueRep,
        layout: Option<&CellLayout>,
        dst: usize,
        tokens: &mut Tokens,
    ) -> Instr {
        Instr::Construct {
            dst,
            tag: self.tags.intern(tag.to_owned()),
            fields: tokens.take_all(fields),
            dicts: Vec::new(),
            rep,
            layout: layout.cloned(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn translate_construct_with_dicts(
        &mut self,
        tag: &str,
        fields: &[Local],
        dicts: &[Local],
        rep: ValueRep,
        layout: Option<&CellLayout>,
        dst: usize,
        tokens: &mut Tokens,
    ) -> Instr {
        Instr::Construct {
            dst,
            tag: self.tags.intern(tag.to_owned()),
            fields: tokens.take_all(fields),
            dicts: tokens.take_all(dicts),
            rep,
            layout: layout.cloned(),
        }
    }

    pub(super) fn translate_project(
        object: Local,
        field: usize,
        dst: usize,
        tokens: &Tokens,
    ) -> Instr {
        Instr::Project {
            dst,
            object: tokens.peek(object),
            field,
        }
    }

    pub(super) fn translate_record(
        fields: &[(String, Local)],
        dst: usize,
        tokens: &mut Tokens,
    ) -> Instr {
        Instr::Record {
            dst,
            fields: tokens.take_fields(fields),
        }
    }

    pub(super) fn translate_field(
        object: Local,
        field: &str,
        dst: usize,
        tokens: &Tokens,
    ) -> Instr {
        Instr::Field {
            dst,
            object: tokens.peek(object),
            field: field.to_owned(),
        }
    }

    pub(super) fn translate_match(
        object: Local,
        pattern: &Pat,
        dst: usize,
        tokens: &Tokens,
    ) -> Instr {
        Instr::Match {
            dst,
            object: tokens.peek(object),
            pattern: Box::new(pattern.clone()),
        }
    }

    pub(super) fn translate_update(
        object: Local,
        fields: &[(String, Local)],
        dst: usize,
        tokens: &mut Tokens,
    ) -> Instr {
        let object = tokens.take(object);
        let mut labels = Vec::with_capacity(fields.len());
        let mut values = Vec::with_capacity(fields.len());
        for (name, local) in fields {
            labels.push(name.clone());
            values.push(tokens.take(*local));
        }
        Instr::Update {
            dst,
            object,
            labels,
            values,
        }
    }

    pub(super) fn translate_drop_reuse(object: Local, dst: usize, tokens: &mut Tokens) -> Instr {
        Instr::DropReuse {
            dst,
            object: tokens.take(object),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn translate_reuse(
        &mut self,
        code: CodeId,
        object: Local,
        tag: &str,
        fields: &[Local],
        rep: ValueRep,
        layout: Option<&CellLayout>,
        dst: usize,
        span: Option<MemorySpan>,
        tokens: &mut Tokens,
    ) -> Result<Instr, PrepareError> {
        if rep.is_aggregate() {
            return Err(unsupported(code, "Reuse(Aggregate)", span));
        }
        Ok(Instr::Reuse {
            dst,
            object: tokens.take(object),
            tag: self.tags.intern(tag.to_owned()),
            fields: tokens.take_all(fields),
            dicts: Vec::new(),
            layout: layout.cloned(),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn translate_reuse_with_dicts(
        &mut self,
        code: CodeId,
        object: Local,
        tag: &str,
        fields: &[Local],
        dicts: &[Local],
        rep: ValueRep,
        layout: Option<&CellLayout>,
        dst: usize,
        span: Option<MemorySpan>,
        tokens: &mut Tokens,
    ) -> Result<Instr, PrepareError> {
        if rep.is_aggregate() {
            return Err(unsupported(code, "ReuseWithDicts(Aggregate)", span));
        }
        Ok(Instr::Reuse {
            dst,
            object: tokens.take(object),
            tag: self.tags.intern(tag.to_owned()),
            fields: tokens.take_all(fields),
            dicts: tokens.take_all(dicts),
            layout: layout.cloned(),
        })
    }

    pub(super) fn translate_reuse_record(
        object: Local,
        fields: &[(String, Local)],
        dst: usize,
        tokens: &mut Tokens,
    ) -> Instr {
        Instr::ReuseRecord {
            dst,
            object: tokens.take(object),
            fields: tokens.take_fields(fields),
        }
    }

    pub(super) fn translate_box_aggregate(
        value: Local,
        layout: Option<&CellLayout>,
        dst: usize,
        tokens: &mut Tokens,
    ) -> Instr {
        Instr::BoxAggregate {
            dst,
            value: tokens.take(value),
            layout: layout.cloned(),
        }
    }

    pub(super) fn translate_unbox_borrow(object: Local, dst: usize, tokens: &Tokens) -> Instr {
        Instr::UnboxBorrow {
            dst,
            object: tokens.peek(object),
        }
    }

    pub(super) fn translate_unbox_consume(object: Local, dst: usize, tokens: &mut Tokens) -> Instr {
        Instr::UnboxConsume {
            dst,
            object: tokens.take(object),
        }
    }

    pub(super) fn translate_dictionary(
        &mut self,
        entries: &[(String, DictionaryEntry)],
        dst: usize,
        tokens: &mut Tokens,
    ) -> Instr {
        Instr::Dictionary {
            dst,
            entries: entries
                .iter()
                .map(|(field, entry)| {
                    (
                        self.dictionary_fields.intern(field.clone()),
                        tokens.take(entry.local()),
                        matches!(entry, DictionaryEntry::Getter(_)),
                    )
                })
                .collect(),
        }
    }

    pub(super) fn translate_method(
        &mut self,
        dictionary: Local,
        field: &str,
        dst: usize,
        tokens: &Tokens,
    ) -> Instr {
        Instr::Method {
            dst,
            dictionary: tokens.peek(dictionary),
            field: self.dictionary_fields.intern(field.to_owned()),
        }
    }

    pub(super) fn translate_structure(
        object: Local,
        expected: usize,
        dst: usize,
        tokens: &Tokens,
    ) -> Instr {
        Instr::Structure {
            dst,
            object: tokens.peek(object),
            expected,
        }
    }

    pub(super) fn translate_var_make(value: Local, dst: usize, tokens: &mut Tokens) -> Instr {
        Instr::VarMake {
            dst,
            src: tokens.take(value),
        }
    }

    pub(super) fn translate_var_get(var: Local, dst: usize, tokens: &mut Tokens) -> Instr {
        Instr::VarGet {
            dst,
            var: tokens.take(var),
        }
    }

    pub(super) fn translate_var_set(
        var: Local,
        value: Local,
        dst: usize,
        tokens: &mut Tokens,
    ) -> Instr {
        Instr::VarSet {
            dst,
            var: tokens.take(var),
            value: tokens.take(value),
        }
    }

    pub(super) fn translate_loop(
        &mut self,
        id: &str,
        body_code: CodeId,
        args: &[Local],
        dst: usize,
        tokens: &mut Tokens,
    ) -> Instr {
        Instr::Loop {
            dst,
            id: self.loops.intern(id.to_owned()),
            code: self.codes.intern(body_code),
            args: tokens.take_all(args),
        }
    }

    pub(super) fn translate_loop_exit(
        &mut self,
        id: &str,
        value: Option<Local>,
        tokens: &mut Tokens,
    ) -> Instr {
        Instr::LoopExit {
            id: self.loops.intern(id.to_owned()),
            value: value.map(|local| tokens.take(local)),
        }
    }

    pub(super) fn translate_function_boundary(
        &mut self,
        fn_code: CodeId,
        args: &[Local],
        dst: usize,
        tokens: &mut Tokens,
    ) -> Instr {
        Instr::FunctionBoundary {
            dst,
            code: self.codes.intern(fn_code),
            args: tokens.take_all(args),
        }
    }

    pub(super) fn translate_unwrap_option(option: Local, dst: usize, tokens: &mut Tokens) -> Instr {
        Instr::UnwrapOption {
            dst,
            option: tokens.take(option),
        }
    }

    pub(super) fn translate_label(value: &str, dst: usize) -> Instr {
        Instr::Label {
            dst,
            value: value.to_owned(),
        }
    }

    pub(super) fn translate_service_tag(key: &str, dst: usize) -> Instr {
        Instr::ServiceTag {
            dst,
            key: key.to_owned(),
        }
    }

    pub(super) fn translate_read_owner_evidence(dst: usize) -> Instr {
        Instr::ReadOwnerEvidence { dst }
    }

    pub(super) fn translate_recovery_result(
        selected: Option<Local>,
        dst: usize,
        tokens: &mut Tokens,
    ) -> Instr {
        Instr::Recovery {
            dst,
            selected: selected.map(|local| tokens.take(local)),
        }
    }
}
