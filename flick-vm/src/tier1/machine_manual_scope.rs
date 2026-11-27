//! Manual scope ownership (`OwnerNew`, `OwnerRun`, `OwnerClose`) and captured
//! service/fiber-ref environment execution (`EnvCapture`, `EnvRun`) for tier 1.

use std::collections::BTreeMap;
use std::mem;

use flick_core::ir::memory::LayerIntrinsic;

use crate::stdlib;
use crate::tier0::{CauseToken, Data, Error, Value};

use super::machine::{End, Machine, OwnerEvidence, RegVal};
use super::machine_scopes::{OwnerState, ScopeOutcome};

impl Machine<'_, '_> {
    pub(super) fn manual_scope_intrinsic(
        &mut self,
        op: LayerIntrinsic,
        dst: usize,
    ) -> Result<(), Error> {
        match op {
            LayerIntrinsic::OwnerNew => {
                self.release_operands()?;
                let id = self.owners.len();
                self.owners.push(OwnerState::default());
                let handle = Value::Owner(self.owner_handle(id));
                self.store_reg(dst, RegVal::Value(handle))
            }
            LayerIntrinsic::OwnerRun => {
                let task = self.scratch.pop().expect("owner_run task");
                let owner_val = self.scratch.pop().expect("owner_run owner");
                let owner = match self.owner_index(&owner_val) {
                    Ok(o) => o,
                    Err(err) => {
                        self.heap.drop_values(vec![owner_val, task])?;
                        return Err(err);
                    }
                };
                self.heap.drop_value(owner_val)?;
                let previous = self.current_owner;
                let evidence = self.current_evidence;
                self.current_owner = owner;
                self.current_evidence = Some(OwnerEvidence::from_owner(&self.owner_handle(owner)));
                self.await_task_reg(
                    RegVal::Value(task),
                    End::RestoreOwnerEvidence {
                        previous,
                        evidence,
                        dst,
                    },
                )
            }
            LayerIntrinsic::OwnerClose => self.exec_owner_close(dst),
            LayerIntrinsic::EnvCapture => {
                self.release_operands()?;
                let services = self.snapshot_active_services()?;
                let refs = match self.snapshot_fiber_refs() {
                    Ok(r) => r,
                    Err(err) => {
                        self.heap.drop_values(services)?;
                        return Err(err);
                    }
                };
                let env_val = self.heap.allocate(Data::CapturedEnv { services, refs });
                self.store_reg(dst, RegVal::Value(env_val))
            }
            LayerIntrinsic::EnvRun => self.exec_env_run(dst),
            _ => unreachable!("non-manual-scope intrinsic"),
        }
    }

    fn exec_owner_close(&mut self, dst: usize) -> Result<(), Error> {
        let opt_cause = self.scratch.pop().expect("owner_close opt cause");
        let owner_val = self.scratch.pop().expect("owner_close owner");
        let owner = match self.owner_index(&owner_val) {
            Ok(o) => o,
            Err(err) => {
                self.heap.drop_values(vec![owner_val, opt_cause])?;
                return Err(err);
            }
        };
        self.heap.drop_value(owner_val)?;
        let exit_override = match stdlib::runtime::coop::extract_opt_value(self.heap, opt_cause)? {
            Some(cval) => Some(CauseToken::from_cause_value(self.heap, cval)?),
            None => None,
        };
        if self.owners[owner].is_closing_or_closed() {
            if let Some(c) = exit_override {
                self.heap.drop_cause(c)?;
            }
            return self.store_reg(dst, RegVal::Value(Value::Unit));
        }
        let previous = self.current_owner;
        self.begin_close_scope_with_override(
            owner,
            previous,
            ScopeOutcome::Normal {
                dst: Some(dst),
                value: RegVal::Value(Value::Unit),
            },
            exit_override,
            false,
        )
    }

    fn exec_env_run(&mut self, dst: usize) -> Result<(), Error> {
        let task = self.scratch.pop().expect("env_run task");
        let env_val = self.scratch.pop().expect("env_run env");
        let (svc_reps, ref_reps) = {
            let Value::Object(tok) = &env_val else {
                self.heap.drop_values(vec![env_val, task])?;
                return Err(Error::ExpectedContext);
            };
            let Data::CapturedEnv { services, refs } = &self.heap.object(tok.id)?.data else {
                self.heap.drop_values(vec![env_val, task])?;
                return Err(Error::ExpectedContext);
            };
            (
                services
                    .iter()
                    .map(Value::representation)
                    .collect::<Vec<_>>(),
                refs.iter()
                    .map(|(k, v)| (k.clone(), v.representation()))
                    .collect::<Vec<_>>(),
            )
        };
        let mut services = Vec::with_capacity(svc_reps.len());
        for rep in svc_reps {
            match self.heap.dup(&rep) {
                Ok(v) => services.push(v),
                Err(err) => {
                    self.heap.drop_values(services)?;
                    self.heap.drop_values(vec![env_val, task])?;
                    return Err(err);
                }
            }
        }
        let mut refs = BTreeMap::new();
        for (k, rep) in ref_reps {
            match self.heap.dup(&rep) {
                Ok(v) => {
                    refs.insert(k, v);
                }
                Err(err) => {
                    self.heap.drop_values(services)?;
                    self.heap.drop_values(refs.into_values().collect())?;
                    self.heap.drop_values(vec![env_val, task])?;
                    return Err(err);
                }
            }
        }
        self.heap.drop_value(env_val)?;
        let previous = mem::replace(&mut self.fiber_refs, refs);
        let inner = End::RestoreEnvRefs { previous, dst };
        let end = if services.is_empty() {
            inner
        } else {
            self.bump_handler_epoch();
            End::WithServices {
                services,
                next: Box::new(inner),
            }
        };
        self.await_task_reg(RegVal::Value(task), end)
    }
}
