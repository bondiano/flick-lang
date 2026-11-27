//! `AsyncIntrinsic` dispatch: fibers, masks, sleep, yield, deferreds,
//! cached tasks, test clock, and seeded random for tier 1.

use flick_core::ir::Lit;
use flick_core::ir::memory::AsyncIntrinsic;

use crate::stdlib::{self, primitives};
use crate::tier0::{Data, DefectSpec, Error, Value};

use super::machine::{End, Machine, OwnerEvidence, RegVal};
use super::machine_coords::{AllMode, CachedCellState, CachedEntry};
use super::machine_fibers::{
    FiberOutcome, FiberWaitKind, FiberWaiter, ParkReason, SupervisorHook, TimerAction,
};

impl Machine<'_, '_> {
    pub(super) fn exec_async_op(
        &mut self,
        dst: usize,
        op: AsyncIntrinsic,
        args: &[usize],
    ) -> Result<(), Error> {
        self.stage_slots(args)?;
        match op {
            AsyncIntrinsic::FiberSpawn
            | AsyncIntrinsic::FiberJoin
            | AsyncIntrinsic::FiberAwait
            | AsyncIntrinsic::FiberInterrupt
            | AsyncIntrinsic::FiberInterruptAs
            | AsyncIntrinsic::FiberJoinAll
            | AsyncIntrinsic::FiberAwaitAll
            | AsyncIntrinsic::FiberInterruptAll
            | AsyncIntrinsic::FiberInterruptAllAs => self.fiber_intrinsic(op, dst),
            AsyncIntrinsic::EffectFiberId
            | AsyncIntrinsic::EffectYieldNow
            | AsyncIntrinsic::EffectYieldNowWith
            | AsyncIntrinsic::EffectSleep
            | AsyncIntrinsic::EffectNever
            | AsyncIntrinsic::EffectInterrupt
            | AsyncIntrinsic::EffectInterruptible
            | AsyncIntrinsic::EffectUninterruptibleMask
            | AsyncIntrinsic::EffectInterruptibleMask
            | AsyncIntrinsic::EffectEnsuring
            | AsyncIntrinsic::EffectOnExitPrimitive
            | AsyncIntrinsic::EffectCatchCause
            | AsyncIntrinsic::EffectOrDie
            | AsyncIntrinsic::EffectTimed
            | AsyncIntrinsic::EffectCached
            | AsyncIntrinsic::EffectCachedWithTtl
            | AsyncIntrinsic::EffectCachedInvalidateWithTtl => self.effect_async_intrinsic(op, dst),
            AsyncIntrinsic::Parallel
            | AsyncIntrinsic::ParallelAll
            | AsyncIntrinsic::ParallelResults
            | AsyncIntrinsic::ParallelExits
            | AsyncIntrinsic::EffectAll
            | AsyncIntrinsic::RaceSuccess
            | AsyncIntrinsic::RaceFirst
            | AsyncIntrinsic::RaceAll
            | AsyncIntrinsic::RaceAllFirst
            | AsyncIntrinsic::Timeout
            | AsyncIntrinsic::TimeoutOption
            | AsyncIntrinsic::TimeoutOrElse => self.parallel_race_intrinsic(op, dst),
            AsyncIntrinsic::DeferredMake
            | AsyncIntrinsic::DeferredAwait
            | AsyncIntrinsic::DeferredDone
            | AsyncIntrinsic::DeferredCompleteWith
            | AsyncIntrinsic::DeferredIsDone
            | AsyncIntrinsic::DeferredPoll
            | AsyncIntrinsic::DeferredInterruptWith => self.deferred_intrinsic(op, dst),
            AsyncIntrinsic::FiberGroupMake
            | AsyncIntrinsic::FiberGroupRun
            | AsyncIntrinsic::FiberGroupAdd
            | AsyncIntrinsic::FiberGroupGet
            | AsyncIntrinsic::FiberGroupHas
            | AsyncIntrinsic::FiberGroupRemove
            | AsyncIntrinsic::FiberGroupClear
            | AsyncIntrinsic::FiberGroupSize
            | AsyncIntrinsic::FiberGroupJoin
            | AsyncIntrinsic::FiberGroupAwaitEmpty => self.fiber_group_intrinsic(op, dst),
            AsyncIntrinsic::FailureInspect => {
                let text = primitives::describe_cause_report(self.heap, &self.scratch[0])?;
                let val = self.heap.literal(&Lit::String(text))?;
                self.finish_primitive(dst, val)
            }
            AsyncIntrinsic::TestClockAdjust => self.exec_test_clock_adjust(dst),
            AsyncIntrinsic::TestClockSetTime => self.exec_test_clock_set_time(dst),
            AsyncIntrinsic::RandomWithSeed => self.exec_random_with_seed(dst),
        }
    }

    fn fiber_intrinsic(&mut self, op: AsyncIntrinsic, dst: usize) -> Result<(), Error> {
        match op {
            AsyncIntrinsic::FiberSpawn => self.exec_fiber_spawn(dst),
            AsyncIntrinsic::FiberJoin => self.exec_fiber_wait(FiberWaitKind::Join(dst)),
            AsyncIntrinsic::FiberAwait => self.exec_fiber_wait(FiberWaitKind::Await(dst)),
            AsyncIntrinsic::FiberInterrupt => self.exec_fiber_interrupt(None, dst),
            AsyncIntrinsic::FiberInterruptAs => {
                let fib = self.scratch.pop().expect("fiber");
                let opt = self.scratch.pop().expect("opt id");
                let by = stdlib::runtime::coop::decode_opt_fiber_id(self.heap, &opt)?;
                self.heap.drop_value(opt)?;
                self.scratch.push(fib);
                self.exec_fiber_interrupt(by, dst)
            }
            AsyncIntrinsic::FiberJoinAll => self.exec_fiber_all(AllMode::JoinAll, dst),
            AsyncIntrinsic::FiberAwaitAll => self.exec_fiber_all(AllMode::AwaitAll, dst),
            AsyncIntrinsic::FiberInterruptAll => self.exec_fiber_interrupt_all(None, dst),
            AsyncIntrinsic::FiberInterruptAllAs => {
                let list = self.scratch.pop().expect("fibers");
                let id_val = self.scratch.pop().expect("id");
                let by = stdlib::runtime::coop::decode_fiber_float(&id_val)?;
                self.heap.drop_value(id_val)?;
                self.scratch.push(list);
                self.exec_fiber_interrupt_all(Some(by), dst)
            }
            _ => unreachable!("non-fiber async intrinsic"),
        }
    }

    fn effect_async_intrinsic(&mut self, op: AsyncIntrinsic, dst: usize) -> Result<(), Error> {
        match op {
            AsyncIntrinsic::EffectFiberId => {
                self.release_operands()?;
                #[expect(clippy::cast_precision_loss, reason = "fiber id fits exact f64")]
                let id = self.active_fiber as f64;
                self.store_reg(dst, RegVal::Value(Value::Float(id)))
            }
            AsyncIntrinsic::EffectYieldNow | AsyncIntrinsic::EffectYieldNowWith => {
                self.release_operands()?;
                if self.check_active_interrupt()? {
                    return Ok(());
                }
                self.store_reg(dst, RegVal::Value(Value::Unit))?;
                self.yield_current(None)
            }
            AsyncIntrinsic::EffectSleep => self.exec_effect_sleep(dst),
            AsyncIntrinsic::EffectNever => {
                self.release_operands()?;
                if self.check_active_interrupt()? {
                    return Ok(());
                }
                self.park_current(ParkReason::Never)
            }
            AsyncIntrinsic::EffectInterrupt => {
                self.release_operands()?;
                let cause = self.interrupt_cause(self.active_fiber as u64)?;
                self.failure = Some(cause);
                self.failure_elected = true;
                Ok(())
            }
            AsyncIntrinsic::EffectInterruptible => {
                let task = self.scratch.pop().expect("interruptible task");
                let saved = self.mask_depth;
                self.mask_depth = 0;
                self.await_task_reg(
                    RegVal::Value(task),
                    End::RestoreMask {
                        saved_mask: saved,
                        check_on_exit: false,
                        next: Box::new(End::Store(dst)),
                    },
                )
            }
            AsyncIntrinsic::EffectUninterruptibleMask => self.exec_mask_region(true, dst),
            AsyncIntrinsic::EffectInterruptibleMask => self.exec_mask_region(false, dst),
            AsyncIntrinsic::EffectEnsuring => {
                let task = self.scratch.pop().expect("ensuring body");
                let fin = self.scratch.pop().expect("ensuring finalizer");
                self.await_task_reg(
                    RegVal::Value(task),
                    End::EnsuringFinish {
                        finalizer: RegVal::Value(fin),
                        dst,
                    },
                )
            }
            AsyncIntrinsic::EffectOnExitPrimitive => {
                let task = self.scratch.pop().expect("on_exit body");
                let decide = self.scratch.pop().expect("on_exit decide");
                let Value::Bool(interruptible) = self.scratch.pop().expect("interruptible flag")
                else {
                    self.heap.drop_values(vec![decide, task])?;
                    return Err(Error::ExpectedBoolean);
                };
                self.await_task_reg(
                    RegVal::Value(task),
                    End::OnExitDecide {
                        interruptible,
                        decide: RegVal::Value(decide),
                        dst,
                    },
                )
            }
            AsyncIntrinsic::EffectCatchCause => {
                let task = self.scratch.pop().expect("catch_cause body");
                let handler = self.scratch.pop().expect("catch_cause handler");
                self.await_task_reg(
                    RegVal::Value(task),
                    End::CatchCause {
                        handler: RegVal::Value(handler),
                        dst,
                    },
                )
            }
            AsyncIntrinsic::EffectOrDie => {
                let task = self.scratch.pop().expect("or_die task");
                self.await_task_reg(RegVal::Value(task), End::OrDie { dst })
            }
            AsyncIntrinsic::EffectTimed => {
                let task = self.scratch.pop().expect("timed task");
                let start_ns = self.virtual_now_ns;
                self.await_task_reg(RegVal::Value(task), End::TimedFinish { start_ns, dst })
            }
            AsyncIntrinsic::EffectCached => self.exec_cached(None, false, dst),
            AsyncIntrinsic::EffectCachedWithTtl => self.exec_cached_with_ttl(false, dst),
            AsyncIntrinsic::EffectCachedInvalidateWithTtl => self.exec_cached_with_ttl(true, dst),
            _ => unreachable!("non-effect async intrinsic"),
        }
    }

    fn exec_cached_with_ttl(&mut self, with_invalidate: bool, dst: usize) -> Result<(), Error> {
        let task = self.scratch.pop().expect("task");
        let dur = self.scratch.pop().expect("ttl");
        let ttl = primitives::duration_sleep_nanos(self.heap, &dur)?;
        self.heap.drop_value(dur)?;
        self.scratch.push(task);
        self.exec_cached(ttl, with_invalidate, dst)
    }

    fn exec_fiber_spawn(&mut self, dst: usize) -> Result<(), Error> {
        let task = self.scratch.pop().expect("spawn task");
        let supervisor = self.scratch.pop().expect("spawn supervisor");
        let owner_val = self.scratch.pop().expect("spawn owner");
        let owner = self.owner_index(&owner_val)?;
        if !self.owners[owner].is_open() {
            self.heap.drop_values(vec![supervisor, task])?;
            return self.raise_defect(DefectSpec::custom(
                "Error: the owner is closing: it admits no new children",
            ));
        }
        let services = self.snapshot_active_services()?;
        let sup_services = self.snapshot_active_services()?;
        let refs = self.snapshot_fiber_refs()?;
        let ev = Some(OwnerEvidence::from_owner(&self.owner_handle(owner)));
        let child_id = self.spawn_raw_fiber(owner, ev, task, services, refs, 0)?;
        self.owners[owner].children.insert(child_id);
        self.owners[owner].watchers += 1;
        if let Some(rec) = self.fiber_table.get_mut(&child_id) {
            rec.owner = Some(owner);
            rec.supervisor = Some(SupervisorHook {
                owner,
                callback: supervisor,
                services: sup_services,
            });
        }
        let handle = self.heap.allocate(Data::FiberHandle {
            execution: self.execution,
            id: child_id,
        });
        self.store_reg(dst, RegVal::Value(handle))
    }

    pub(super) fn fiber_id_of(&self, val: &Value) -> Result<usize, Error> {
        let Value::Object(tok) = val else {
            return Err(Error::ExpectedFiber);
        };
        let Data::FiberHandle { execution, id } = &self.heap.object(tok.id)?.data else {
            return Err(Error::ExpectedFiber);
        };
        if *execution != self.execution {
            return Err(Error::ExpectedFiber);
        }
        Ok(*id)
    }

    fn exec_fiber_wait(&mut self, kind: FiberWaitKind) -> Result<(), Error> {
        let fib_val = self.scratch.pop().expect("fiber");
        let target = self.fiber_id_of(&fib_val)?;
        self.heap.drop_value(fib_val)?;
        if self.check_active_interrupt()? {
            return Ok(());
        }
        let done = self
            .fiber_table
            .get(&target)
            .is_some_and(|r| !matches!(r.outcome, FiberOutcome::Running));
        if done {
            return match kind {
                FiberWaitKind::Interrupt(d) => self.store_reg(d, RegVal::Value(Value::Unit)),
                FiberWaitKind::Join(d) => match &self.fiber_table[&target].outcome {
                    FiberOutcome::Success(v) => {
                        let rep = v.representation();
                        let dup = self.heap.dup(&rep)?;
                        self.store_reg(d, RegVal::Value(dup))
                    }
                    FiberOutcome::Failure(c) => {
                        self.failure = Some(self.heap.dup_cause(c)?);
                        self.failure_elected = true;
                        Ok(())
                    }
                    FiberOutcome::Running => Ok(()),
                },
                FiberWaitKind::Await(d) => {
                    let exit = self.fiber_outcome_to_exit(target)?;
                    self.store_reg(d, RegVal::Value(exit))
                }
                FiberWaitKind::CoordDone { .. } => Ok(()),
            };
        }
        let me = self.active_fiber;
        if let Some(rec) = self.fiber_table.get_mut(&target) {
            rec.waiters.push(FiberWaiter { fiber: me, kind });
        }
        self.park_current(ParkReason::Fiber { target })
    }

    fn exec_fiber_interrupt(&mut self, by: Option<u64>, dst: usize) -> Result<(), Error> {
        let fib_val = self.scratch.pop().expect("fiber");
        let target = self.fiber_id_of(&fib_val)?;
        self.heap.drop_value(fib_val)?;
        let interruptor = by.unwrap_or(self.active_fiber as u64);
        self.request_interrupt(target, interruptor)?;
        let done = self
            .fiber_table
            .get(&target)
            .is_some_and(|r| !matches!(r.outcome, FiberOutcome::Running));
        if done {
            return self.store_reg(dst, RegVal::Value(Value::Unit));
        }
        let me = self.active_fiber;
        let kind = FiberWaitKind::Interrupt(dst);
        if let Some(rec) = self.fiber_table.get_mut(&target) {
            rec.waiters.push(FiberWaiter { fiber: me, kind });
        }
        self.park_current(ParkReason::Fiber { target })
    }

    fn exec_effect_sleep(&mut self, dst: usize) -> Result<(), Error> {
        let dur_val = self.scratch.pop().expect("sleep duration");
        let ns = primitives::duration_sleep_nanos(self.heap, &dur_val)?;
        self.heap.drop_value(dur_val)?;
        if self.check_active_interrupt()? {
            return Ok(());
        }
        match ns {
            Some(0) => {
                self.store_reg(dst, RegVal::Value(Value::Unit))?;
                self.yield_current(None)
            }
            None => self.park_current(ParkReason::Never),
            Some(delay_ns) => {
                let fiber = self.active_fiber;
                let timer_id = self.register_timer(delay_ns, TimerAction::WakeSleep { fiber, dst });
                self.park_current(ParkReason::Sleep { timer_id })
            }
        }
    }

    fn exec_mask_region(&mut self, uninterruptible: bool, dst: usize) -> Result<(), Error> {
        let f = self.scratch.pop().expect("mask callback");
        let outer = self.mask_depth;
        self.mask_depth = if uninterruptible { outer + 1 } else { 0 };
        let restore = self.heap.allocate(Data::MaskRestore {
            execution: self.execution,
            outer_mask: outer,
        });
        self.apply_reg(
            RegVal::Value(f),
            RegVal::Value(restore),
            End::RestoreMask {
                saved_mask: outer,
                check_on_exit: true,
                next: Box::new(End::Store(dst)),
            },
            false,
        )
    }

    fn exec_cached(
        &mut self,
        ttl_ns: Option<u64>,
        with_invalidate: bool,
        dst: usize,
    ) -> Result<(), Error> {
        let task = self.scratch.pop().expect("cached task");
        let id = self.next_cached;
        self.next_cached = self.next_cached.checked_add(1).expect("cached id overflow");
        self.cached_cells.insert(
            id,
            CachedCellState {
                task,
                ttl_ns,
                entry: CachedEntry::Empty,
            },
        );
        let run_task = self.heap.allocate(Data::CachedRunTask {
            execution: self.execution,
            id,
        });
        if with_invalidate {
            let inv_task = self.heap.allocate(Data::CachedInvalidateTask {
                execution: self.execution,
                id,
            });
            let pair = self.heap.construct("$tuple", vec![run_task, inv_task])?;
            self.store_reg(dst, RegVal::Value(pair))
        } else {
            self.store_reg(dst, RegVal::Value(run_task))
        }
    }
}
