//! Nonowning lexical evidence, independent from the default owner.
use super::{CodeId, Destination, Error, Local, Machine, Operation, OwnerEvidence, Value};

impl Machine<'_, '_> {
    pub(super) fn evidence_operation(
        &mut self,
        binding: Local,
        operation: &Operation,
    ) -> Result<bool, Error> {
        match operation {
            Operation::ReadOwnerEvidence => {
                let value = self
                    .current_evidence
                    .map_or(Value::Unit, OwnerEvidence::value);
                self.store(binding, value)?;
            }
            Operation::WithOwnerEvidence { owner, code, args } => {
                self.with_evidence(binding, *owner, *code, args)?;
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn with_evidence(
        &mut self,
        binding: Local,
        owner: Local,
        code: CodeId,
        args: &[Local],
    ) -> Result<(), Error> {
        self.stage([owner].into_iter().chain(args.iter().copied()))?;
        // Copy only inert identity metadata. Within checks identity/provenance;
        // resource admission checks retirement at the actual operation.
        let candidate = match &self.scratch[0] {
            Value::Unit => None,
            Value::Owner(owner) => Some(OwnerEvidence::from_owner(owner)),
            _ => return Err(Error::ExpectedOwner),
        };
        self.scratch.remove(0);
        self.current_evidence = candidate;
        self.call(code, Destination::Bind(binding))
    }
}
