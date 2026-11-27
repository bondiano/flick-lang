//! Structured concurrency coordinator completion, stepping, and failure aggregation for tier 1.

use std::collections::VecDeque;

use crate::stdlib::{self, FuelCounter, primitives};
use crate::tier0::{CauseToken, Error, Reason, Value};

use super::machine::Machine;
use super::machine_coords::{AllMode, CoordKind, CoordState};
use super::machine_fibers::FiberOutcome;
use super::machine_layers::LayerBuildTarget;
use super::machine_scopes::Finalizer;

impl Machine<'_, '_> {
    pub(super) fn on_coord_child_done(
        &mut self,
        coord_id: usize,
        index: usize,
        child_id: usize,
    ) -> Result<(), Error> {
        let Some(mut coord) = self.coords.remove(&coord_id) else {
            return Ok(());
        };
        match &mut coord.kind {
            CoordKind::All { .. } => self.on_coord_all_done(coord_id, coord, index, child_id),
            CoordKind::Race { .. } => self.on_coord_race_done(coord_id, coord, child_id),
            CoordKind::Timeout { .. } => {
                let LayerBuildTarget::Local(dst) = coord.target else {
                    unreachable!("timeout target is local");
                };
                let CoordKind::Timeout {
                    child,
                    timer_id,
                    kind,
                    timed_out,
                    interrupted_by,
                } = coord.kind
                else {
                    unreachable!()
                };
                self.finish_coord_timeout(
                    coord.parent_fiber,
                    dst,
                    child,
                    timer_id,
                    kind,
                    timed_out,
                    interrupted_by,
                )
            }
            CoordKind::WaitAll { pending, .. } => {
                *pending = pending.saturating_sub(1);
                if *pending == 0 {
                    let CoordKind::WaitAll { done_value, .. } = coord.kind else {
                        unreachable!()
                    };
                    self.wake_fiber_build_value(coord.parent_fiber, coord.target, done_value)
                } else {
                    self.coords.insert(coord_id, coord);
                    Ok(())
                }
            }
            CoordKind::OwnerHookWait { owner, pending } => {
                *pending = pending.saturating_sub(1);
                let owner = *owner;
                if *pending == 0 {
                    self.wake_owner_closer(owner);
                } else {
                    self.coords.insert(coord_id, coord);
                }
                Ok(())
            }
        }
    }

    fn on_coord_all_done(
        &mut self,
        coord_id: usize,
        mut coord: CoordState,
        index: usize,
        child_id: usize,
    ) -> Result<(), Error> {
        let done = match self.step_coord_all_mut(coord_id, &mut coord, index, child_id) {
            Ok(done) => done,
            Err(err) => {
                self.coords.insert(coord_id, coord);
                return Err(err);
            }
        };
        if !done {
            self.coords.insert(coord_id, coord);
            return Ok(());
        }
        let CoordKind::All {
            mode,
            results,
            first_failure,
            ..
        } = coord.kind
        else {
            unreachable!()
        };
        self.finish_coord_all(
            coord.parent_fiber,
            coord.target,
            mode,
            results,
            first_failure,
        )
    }

    fn step_coord_all_mut(
        &mut self,
        coord_id: usize,
        coord: &mut CoordState,
        index: usize,
        child_id: usize,
    ) -> Result<bool, Error> {
        let parent = coord.parent_fiber;
        let CoordKind::All {
            mode,
            queued,
            children,
            results,
            first_failure,
            stopping,
            pending,
            owner,
        } = &mut coord.kind
        else {
            unreachable!()
        };
        *pending = pending.saturating_sub(1);
        match mode {
            AllMode::Exits | AllMode::AwaitAll => {
                let exit = self.fiber_outcome_to_exit(child_id)?;
                results[index] = Some(exit);
            }
            AllMode::Results => match &self.fiber_table[&child_id].outcome {
                FiberOutcome::Success(v) => {
                    let rep = v.representation();
                    let dup = self.heap.dup(&rep)?;
                    results[index] = Some(self.heap.construct("Prelude.Result.Ok", vec![dup])?);
                }
                FiberOutcome::Failure(c) => {
                    let has_die = self
                        .heap
                        .cause_reasons(c)?
                        .any(|r| matches!(r, Reason::Die(_)));
                    let first_err = if has_die {
                        None
                    } else {
                        self.heap.first_fail(c, &mut self.fuel)?
                    };
                    if let Some(err_rep) = first_err {
                        let dup = self.heap.dup(&err_rep)?;
                        results[index] =
                            Some(self.heap.construct("Prelude.Result.Err", vec![dup])?);
                    } else {
                        self.record_coord_failure(
                            parent,
                            child_id,
                            first_failure,
                            stopping,
                            queued,
                            children,
                        )?;
                    }
                }
                FiberOutcome::Running => {}
            },
            _ => match &self.fiber_table[&child_id].outcome {
                FiberOutcome::Success(v) => {
                    if !*stopping {
                        let rep = v.representation();
                        results[index] = Some(self.heap.dup(&rep)?);
                    }
                }
                FiberOutcome::Failure(_) => {
                    self.record_coord_failure(
                        parent,
                        child_id,
                        first_failure,
                        stopping,
                        queued,
                        children,
                    )?;
                }
                FiberOutcome::Running => {}
            },
        }
        if !*stopping && let Some((next_idx, next_task)) = queued.pop_front() {
            let services = self.fiber_services_for_spawn(parent)?;
            let refs = self.fiber_refs_for_spawn(parent)?;
            let ev = self.fiber_evidence(parent);
            let next_child = self.spawn_raw_fiber(*owner, ev, next_task, services, refs, 0)?;
            if let Some(rec) = self.fiber_table.get_mut(&next_child) {
                rec.coord_parent = Some((coord_id, next_idx));
            }
            children.push(next_child);
            *pending += 1;
        }
        Ok(*pending == 0)
    }

    fn record_coord_failure(
        &mut self,
        parent: usize,
        child_id: usize,
        first_failure: &mut Option<CauseToken>,
        stopping: &mut bool,
        queued: &mut VecDeque<(usize, Value)>,
        children: &[usize],
    ) -> Result<(), Error> {
        let FiberOutcome::Failure(c) = &self.fiber_table[&child_id].outcome else {
            return Ok(());
        };
        let dup = self.heap.dup_cause(c)?;
        if *stopping {
            let without = primitives::without_interrupts_cause(
                self.heap,
                &dup,
                &mut FuelCounter(&mut self.fuel),
            );
            self.heap.drop_cause(dup)?;
            if let Some(rest) = without? {
                if let Some(prev) = first_failure.take() {
                    let joined = primitives::join_causes(
                        self.heap,
                        &prev,
                        &rest,
                        &mut FuelCounter(&mut self.fuel),
                    );
                    self.heap.drop_cause(prev)?;
                    self.heap.drop_cause(rest)?;
                    let joined = joined?;
                    let elected = match self.heap.elect_failure(&joined, &mut self.fuel) {
                        Ok(Some(e)) => {
                            self.heap.drop_cause(joined)?;
                            e
                        }
                        Ok(None) => joined,
                        Err(err) => {
                            self.heap.drop_cause(joined)?;
                            return Err(err);
                        }
                    };
                    *first_failure = Some(elected);
                } else {
                    *first_failure = Some(rest);
                }
            }
        } else {
            *stopping = true;
            *first_failure = Some(dup);
            let dropped: Vec<Value> = queued.drain(..).map(|(_, t)| t).collect();
            self.heap.drop_values(dropped)?;
            for &other in children {
                if other != child_id {
                    self.request_interrupt(other, parent as u64)?;
                }
            }
        }
        Ok(())
    }

    fn finish_coord_all(
        &mut self,
        parent: usize,
        target: LayerBuildTarget,
        mode: AllMode,
        results: Vec<Option<Value>>,
        first_failure: Option<CauseToken>,
    ) -> Result<(), Error> {
        if let Some(mut cause) = first_failure {
            let vals: Vec<Value> = results.into_iter().flatten().collect();
            self.heap.drop_values(vals)?;
            if let AllMode::MergeLayers {
                scopes,
                parent_owner,
                or_die,
            } = mode
            {
                for sub_owner in scopes {
                    self.owners[parent_owner]
                        .finalizers
                        .push(Finalizer::CloseChildOwner(sub_owner));
                }
                if or_die {
                    let converted = primitives::fails_to_dies_cause(
                        self.heap,
                        &cause,
                        &mut FuelCounter(&mut self.fuel),
                    );
                    self.heap.drop_cause(cause)?;
                    cause = converted?;
                }
            }
            return self.wake_fiber_build_failure(parent, target, cause);
        }
        let items: Vec<Value> = results
            .into_iter()
            .map(|o| o.unwrap_or(Value::Unit))
            .collect();
        let value = match mode {
            AllMode::Pair => self.heap.construct("$tuple", items)?,
            AllMode::List
            | AllMode::Results
            | AllMode::Exits
            | AllMode::JoinAll
            | AllMode::AwaitAll => self.heap.construct("$list", items)?,
            AllMode::MergeLayers {
                scopes,
                parent_owner,
                ..
            } => {
                for sub_owner in scopes {
                    self.owners[parent_owner]
                        .finalizers
                        .push(Finalizer::CloseChildOwner(sub_owner));
                }
                stdlib::runtime::context::merge_context_values(self.heap, items)?
            }
        };
        self.wake_fiber_build_value(parent, target, value)
    }
}
