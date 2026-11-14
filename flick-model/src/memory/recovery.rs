//! One executing selector owns the whole cause; result markers never become values.
use super::{
    CauseToken, Data, Destination, Error, Local, Machine, OwnerEvidence, Value, charge, mem,
};

pub(super) struct RecoveryState {
    original: CauseToken,
    binding: Local,
    owner: usize,
    evidence: Option<OwnerEvidence>,
}

impl Machine<'_, '_> {
    pub(super) fn prepare_failure(&mut self) -> Result<(), Error> {
        if self.failure_elected {
            return Ok(());
        }
        let cause = self.failure.as_ref().expect("failure root");
        // The original stays machine-owned through every partial allocation/duplicate.
        if let Some(elected) = self.heap.elect_failure(cause, &mut self.fuel)? {
            let original = self.failure.replace(elected).expect("original failure");
            self.heap.drop_cause(original)?;
        }
        self.failure_elected = true;
        Ok(())
    }

    pub(super) fn begin_recovery(&mut self, binding: Local) -> Result<(), Error> {
        self.prepare_failure()?;
        let original = self.failure.as_ref().expect("failure root");
        let Some(payload) = self.heap.first_fail(original, &mut self.fuel)? else {
            self.heap
                .drop_value(self.pending_handler.take().expect("unused selector"))?;
            return Ok(());
        };
        charge(&mut self.fuel, 1)?;
        let id = self.next_recovery;
        self.next_recovery = id.checked_add(1).ok_or(Error::RecoveryIdentityOverflow)?;
        self.recoveries.insert(
            id,
            RecoveryState {
                original: self.failure.take().expect("eligible whole cause"),
                binding,
                owner: self.current_owner,
                evidence: self.current_evidence,
            },
        );
        // C and selector are machine roots BEFORE duplication/apply/frame installation.
        charge(&mut self.fuel, 1)?;
        let argument = self.heap.dup(&payload)?;
        self.scratch
            .push(self.pending_handler.take().expect("rooted selector"));
        self.scratch.push(argument);
        self.apply(Destination::RecoverySelection { id })
    }

    fn executing_recovery(&self) -> Result<usize, Error> {
        let id = self
            .frames
            .last()
            .and_then(|frame| frame.recovery_id)
            .ok_or(Error::RecoveryOutsideSelection)?;
        if !self.recoveries.contains_key(&id) {
            return Err(Error::ForeignRecoveryResult);
        }
        Ok(id)
    }

    fn check_selected(&self, value: &Value) -> Result<(), Error> {
        self.heap.admit_ordinary(value)?;
        let Value::Object(token) = value else {
            return Err(Error::ExpectedClosure);
        };
        let Data::Closure {
            execution, code, ..
        } = &self.heap.object(token.id)?.data
        else {
            return Err(Error::ExpectedClosure);
        };
        if *execution != self.execution {
            return Err(Error::ForeignClosure);
        }
        let function = self
            .program
            .functions
            .get(code)
            .ok_or(Error::InvalidProgram(super::memory::Error::UnknownCode(
                *code,
            )))?;
        if function.params.len() != 2 {
            return Err(Error::ArgumentCount {
                expected: 2,
                actual: function.params.len(),
            });
        }
        Ok(())
    }

    pub(super) fn recovery_result(
        &mut self,
        binding: Local,
        selected: Option<Local>,
    ) -> Result<(), Error> {
        self.stage(selected)?;
        let id = self.executing_recovery()?;
        if selected.is_some() {
            self.check_selected(self.scratch.last().expect("selected closure root"))?;
        }
        charge(&mut self.fuel, 1)?;
        let selected = selected.map(|_| self.scratch.pop().expect("selected closure root"));
        let value = self.heap.allocate(Data::RecoveryResult {
            execution: self.execution,
            recovery_id: id,
            selected,
        });
        self.store_transport(binding, value, id)
    }

    fn marker(&self, value: &Value, id: usize) -> Result<Option<Value>, Error> {
        let Value::Object(token) = value else {
            return Err(Error::ExpectedRecoveryResult);
        };
        let Data::RecoveryResult {
            execution,
            recovery_id,
            selected,
        } = &self.heap.object(token.id)?.data
        else {
            return Err(Error::ExpectedRecoveryResult);
        };
        if *execution != self.execution || *recovery_id != id || !self.recoveries.contains_key(&id)
        {
            return Err(Error::ForeignRecoveryResult);
        }
        Ok(selected.as_ref().map(Value::representation))
    }

    pub(super) fn store_transport(
        &mut self,
        binding: Local,
        value: Value,
        id: usize,
    ) -> Result<(), Error> {
        self.scratch.push(value);
        if self.executing_recovery()? != id {
            return Err(Error::ForeignRecoveryResult);
        }
        let value = self.scratch.last().expect("transport root");
        if let Value::Object(token) = value
            && matches!(
                self.heap.object(token.id)?.data,
                Data::RecoveryResult { .. }
            )
        {
            self.marker(value, id)?;
        } else {
            self.heap.admit_ordinary(value)?;
        }
        let value = self.scratch.pop().expect("validated transport root");
        self.store_raw(binding, value);
        Ok(())
    }

    pub(super) fn complete_recovery(&mut self, id: usize) -> Result<(), Error> {
        let selected = self.marker(self.pending.as_ref().expect("rooted selector result"), id)?;
        let hit = selected.is_some();
        if let Some(selected) = &selected {
            self.check_selected(selected)?;
        }
        // Root the body independently BEFORE releasing wrapper/original/obsolete state.
        if let Some(selected) = selected {
            charge(&mut self.fuel, 1)?;
            self.scratch.push(self.heap.dup(&selected)?);
        }
        self.heap
            .drop_value(self.pending.take().expect("selector result"))?;
        let state = self
            .recoveries
            .remove(&id)
            .ok_or(Error::ForeignRecoveryResult)?;
        self.current_owner = state.owner;
        self.current_evidence = state.evidence;
        if hit {
            self.heap.drop_cause(state.original)?;
            self.scratch.push(Value::Unit);
            self.apply(Destination::RecoveryBody {
                binding: state.binding,
                owner: state.owner,
                evidence: state.evidence,
            })?;
        } else {
            self.failure = Some(state.original);
            self.failure_elected = true;
        }
        Ok(())
    }

    pub(super) fn abandon_recovery(&mut self, id: usize) -> Result<(), Error> {
        let state = self
            .recoveries
            .remove(&id)
            .ok_or(Error::ForeignRecoveryResult)?;
        self.current_owner = state.owner;
        self.current_evidence = state.evidence;
        self.heap.drop_cause(state.original)
    }

    pub(super) fn cleanup_recoveries(&mut self, roots: &mut Vec<Value>) {
        roots.extend(
            mem::take(&mut self.recoveries)
                .into_values()
                .map(|state| state.original.into_value()),
        );
    }
}
