//! `Ref` cells, `Arena s` storage, `Within`/`WithOwnerEvidence`, and `Lazy` cells for tier 1.

use std::mem;

use crate::tier0::{ArenaHandle, ArenaView, CellHandle, Data, DefectSpec, Error, Value};

use super::machine::{End, Machine, OwnerEvidence, RegVal};

impl Machine<'_, '_> {
    pub(super) fn exec_task(
        &mut self,
        dst: usize,
        code: usize,
        captures: &[usize],
    ) -> Result<(), Error> {
        self.stage_slots(captures)?;
        self.heap.admit_values(&self.scratch)?;
        let captures_owned = mem::take(&mut self.scratch);
        let val = self.heap.allocate(Data::Task {
            execution: self.execution,
            code: self.image.ir_codes[code],
            captures: captures_owned,
        });
        self.store_raw(dst, RegVal::Value(val));
        Ok(())
    }

    pub(super) fn exec_within(
        &mut self,
        dst: usize,
        owner_slot: usize,
        code: usize,
        args: &[usize],
    ) -> Result<(), Error> {
        let owner_val = self.take_value(owner_slot)?;
        self.scratch.push(owner_val);
        let selected = self.owner_index(&self.scratch[0])?;
        self.scratch.pop();
        let mut vals = Vec::with_capacity(args.len());
        for &slot in args {
            vals.push(self.take_reg(slot)?);
        }
        let previous = self.current_owner;
        self.current_owner = selected;
        self.push_call(code, vals, End::RestoreOwner { previous, dst })
    }

    pub(super) fn exec_with_owner_evidence(
        &mut self,
        dst: usize,
        owner_slot: usize,
        code: usize,
        args: &[usize],
    ) -> Result<(), Error> {
        let owner_val = self.take_value(owner_slot)?;
        self.scratch.push(owner_val);
        let candidate = match &self.scratch[0] {
            Value::Unit => None,
            Value::Owner(owner) => Some(OwnerEvidence::from_owner(owner)),
            _ => return Err(Error::ExpectedOwner),
        };
        self.scratch.pop();
        let mut vals = Vec::with_capacity(args.len());
        for &slot in args {
            vals.push(self.take_reg(slot)?);
        }
        self.current_evidence = candidate;
        self.push_call(code, vals, End::Store(dst))
    }

    pub(super) fn exec_ref_make(&mut self, dst: usize, src: usize) -> Result<(), Error> {
        self.stage_slots(&[src])?;
        let contents = self.scratch.pop().expect("cell initial contents");
        let root = self.heap.allocate(Data::Cell {
            execution: self.execution,
            contents: Some(contents),
        });
        let id = root.object_id().expect("allocated cell root");
        self.cells.push(root);
        self.store_reg(
            dst,
            RegVal::Value(Value::Cell(CellHandle {
                id,
                execution: self.execution,
            })),
        )
    }

    pub(super) fn exec_ref_get(&mut self, dst: usize, cell: usize) -> Result<(), Error> {
        self.stage_slots(&[cell])?;
        let Value::Cell(handle) = &self.scratch[0] else {
            return Err(Error::ExpectedCell);
        };
        let current = self
            .heap
            .cell_contents(self.execution, handle)?
            .representation();
        let val = self.heap.dup(&current)?;
        self.finish_primitive(dst, val)
    }

    pub(super) fn exec_ref_set(
        &mut self,
        dst: usize,
        cell: usize,
        value: usize,
    ) -> Result<(), Error> {
        self.stage_slots(&[cell, value])?;
        let Value::Cell(handle) = &self.scratch[0] else {
            return Err(Error::ExpectedCell);
        };
        let handle = CellHandle {
            id: handle.id,
            execution: handle.execution,
        };
        let contents = self.heap.cell_contents_mut(self.execution, &handle)?;
        let replacement = self.scratch.pop().expect("replacement contents");
        let old = mem::replace(contents, replacement);
        self.scratch.push(old);
        self.finish_primitive(dst, Value::Unit)
    }

    pub(super) fn exec_ref_same(
        &mut self,
        dst: usize,
        left: usize,
        right: usize,
    ) -> Result<(), Error> {
        self.stage_slots(&[left, right])?;
        let (Value::Cell(l), Value::Cell(r)) = (&self.scratch[0], &self.scratch[1]) else {
            return Err(Error::ExpectedCell);
        };
        self.heap.cell_contents(self.execution, l)?;
        self.heap.cell_contents(self.execution, r)?;
        let same = l == r;
        self.finish_primitive(dst, Value::Bool(same))
    }

    pub(super) fn exec_ref_modify(
        &mut self,
        dst: usize,
        cell_slot: usize,
        func_slot: usize,
    ) -> Result<(), Error> {
        let cell_val = self.take_value(cell_slot)?;
        let func = self.take_reg(func_slot)?;
        let Value::Cell(cell) = cell_val else {
            let mut roots = vec![cell_val];
            Self::reg_into_roots(func, &mut roots);
            self.heap.drop_values(roots)?;
            return Err(Error::ExpectedCell);
        };
        let current = match self.heap.cell_contents(self.execution, &cell) {
            Ok(v) => v.representation(),
            Err(err) => {
                self.drop_reg(func)?;
                return Err(err);
            }
        };
        let old = match self.heap.dup(&current) {
            Ok(v) => v,
            Err(err) => {
                self.drop_reg(func)?;
                return Err(err);
            }
        };
        self.apply_reg(func, RegVal::Value(old), End::Modify { cell, dst }, false)
    }

    pub(super) fn complete_modify(&mut self, cell: &CellHandle, dst: usize) -> Result<(), Error> {
        let returned_reg = self.pending.take().expect("callback outcome");
        let returned = self.materialize_reg(returned_reg);
        self.pending = Some(RegVal::Value(returned));
        let Some(RegVal::Value(returned)) = self.pending.as_ref() else {
            unreachable!();
        };
        let Value::Object(token) = returned else {
            return Err(Error::ExpectedModifyPair);
        };
        let Data::Tagged {
            tag,
            ordinary_fields,
            fields,
            ..
        } = &self.heap.object(token.id)?.data
        else {
            return Err(Error::ExpectedModifyPair);
        };
        if tag != "$tuple" || *ordinary_fields != 2 || fields.len() != 2 {
            return Err(Error::ExpectedModifyPair);
        }
        let result = self.heap.project(returned, 0)?;
        self.scratch.push(result);
        let Some(RegVal::Value(pair_ref)) = self.pending.as_ref() else {
            unreachable!();
        };
        let replacement = self.heap.project(pair_ref, 1)?;
        self.scratch.push(replacement);
        let contents = self.heap.cell_contents_mut(self.execution, cell)?;
        let replacement = self.scratch.pop().expect("new contents");
        let old = mem::replace(contents, replacement);
        self.scratch.push(old);
        let Some(RegVal::Value(pair_val)) = self.pending.take() else {
            unreachable!();
        };
        self.scratch.push(pair_val);
        self.pending = Some(RegVal::Value(self.scratch.remove(0)));
        self.release_operands()?;
        let result = self.pending.take().expect("modify result");
        self.store_reg(dst, result)
    }

    pub(super) fn exec_arena_make(&mut self, dst: usize, unit: usize) -> Result<(), Error> {
        self.stage_slots(&[unit])?;
        if self.owners[self.current_owner].is_closed() {
            return self.raise_defect(DefectSpec::arena_owner_closed());
        }
        let root = self.heap.allocate(Data::Arena {
            execution: self.execution,
            contents: Vec::new(),
        });
        let id = root.object_id().expect("arena allocation");
        self.owners[self.current_owner].arenas.push(root);
        let value = Value::Arena(ArenaHandle {
            id,
            execution: self.execution,
        });
        self.finish_primitive(dst, value)
    }

    pub(super) fn exec_arena_alloc(
        &mut self,
        dst: usize,
        arena_slot: usize,
        value_slot: usize,
    ) -> Result<(), Error> {
        self.stage_slots(&[arena_slot, value_slot])?;
        let Value::Arena(arena) = &self.scratch[0] else {
            return Err(Error::ExpectedArena);
        };
        let handle = ArenaHandle {
            id: arena.id,
            execution: arena.execution,
        };
        let contents = match self.heap.arena_contents_mut(self.execution, &handle) {
            Ok(contents) => contents,
            Err(Error::ClosedArena) => {
                return self.raise_defect(DefectSpec::arena_closed());
            }
            Err(err) => return Err(err),
        };
        let index = contents.len();
        contents.push(self.scratch.pop().expect("arena slot contents"));
        let value = Value::ArenaView(ArenaView {
            arena: handle,
            index,
        });
        self.finish_primitive(dst, value)
    }

    pub(super) fn exec_arena_get(&mut self, dst: usize, view_slot: usize) -> Result<(), Error> {
        self.stage_slots(&[view_slot])?;
        let Value::ArenaView(view) = &self.scratch[0] else {
            return Err(Error::ExpectedArenaView);
        };
        let contents = match self.heap.arena_contents(self.execution, &view.arena) {
            Ok(contents) => contents,
            Err(Error::ClosedArena) => {
                return self.raise_defect(DefectSpec::arena_closed());
            }
            Err(err) => return Err(err),
        };
        let current = contents
            .get(view.index)
            .ok_or(Error::MissingArenaSlot(view.index))?
            .representation();
        let value = self.heap.dup(&current)?;
        self.finish_primitive(dst, value)
    }
}
