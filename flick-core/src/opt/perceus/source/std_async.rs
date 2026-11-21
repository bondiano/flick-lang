//! Audited `Fiber`, `Deferred`, `FiberSet`/`FiberMap`/`FiberHandle`, `Failure.inspect`,
//! `RawCause`, `TestClock`, `Random.with_seed`, and `Prelude` concurrency adapters (Stage 4A).

use flick_ir::memory::AsyncIntrinsic;
use flick_ir::{Foreign, ForeignMode};

use super::expr_builder::ExprBuilder;
use super::lazy_scope::has_standard_shape;
use super::{Code, Env, Expr, ExprKind, Intrinsic, Local, Lower, LowerError, Operation, Span};

impl Lower<'_> {
    pub(super) fn std_async_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if value.dicts != 0 || !has_standard_shape(value) {
            return None;
        }
        let from = value.binding.from.as_deref()?;
        let bind = value.binding.name.as_str();
        let stages = value.stages.len();

        if let Some(kind) = direct_async_intrinsic(from, bind, value.mode, stages) {
            return Some(self.intrinsic(Intrinsic::Async(kind), false, vec![], block, span));
        }
        if from == "flick:rt" && bind == "describeReport" && value.mode == ForeignMode::Pure {
            let b = ExprBuilder::new(span, "$dr");
            let expr = b.lam(&["r"], b.loc("r"));
            return Some(self.expr(&expr, &Env::new(), block));
        }
        if from == "flick:Deferred" && value.mode == ForeignMode::Effect {
            let b = ExprBuilder::new(span, "$def");
            let expr = deferred_template(&b, bind, stages)?;
            return Some(self.expr(&expr, &Env::new(), block));
        }
        if matches!(
            from,
            "flick:FiberSet" | "flick:FiberMap" | "flick:FiberHandle"
        ) && value.mode == ForeignMode::Effect
        {
            return self.fiber_group_foreign(from, bind, stages, span, block);
        }
        None
    }

    fn fiber_group_foreign(
        &mut self,
        from: &str,
        bind: &str,
        stages: usize,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        let tag_num: i32 = match from {
            "flick:FiberSet" => 0,
            "flick:FiberMap" => 1,
            "flick:FiberHandle" => 2,
            _ => return None,
        };
        if let Some(op) = direct_fiber_group_intrinsic(from, bind, stages) {
            return Some(self.intrinsic(Intrinsic::Async(op), false, vec![], block, span));
        }
        let b = ExprBuilder::new(span, "$fg");
        let (expr, op) = fiber_group_wrapper(&b, from, bind, stages, tag_num)?;
        Some(self.apply_async_intrinsic(&expr, op, block, span))
    }

    pub(super) fn apply_async_intrinsic(
        &mut self,
        expr: &Expr,
        op: AsyncIntrinsic,
        block: &mut Code,
        span: Span,
    ) -> Result<Local, LowerError> {
        let function = self.expr(expr, &Env::new(), block)?;
        let argument = self.intrinsic(Intrinsic::Async(op), false, vec![], block, span)?;
        block.emit(Operation::Apply { function, argument }, span)
    }
}

fn direct_async_intrinsic(
    from: &str,
    bind: &str,
    mode: ForeignMode,
    stages: usize,
) -> Option<AsyncIntrinsic> {
    if mode != ForeignMode::Effect {
        return None;
    }
    let op = match (from, bind) {
        ("flick:rt", "fiberSpawn") => AsyncIntrinsic::FiberSpawn,
        ("flick:rt", "fiberJoin") => AsyncIntrinsic::FiberJoin,
        ("flick:Fiber", "flickAwait") => AsyncIntrinsic::FiberAwait,
        ("flick:Fiber", "flickAwaitAll") => AsyncIntrinsic::FiberAwaitAll,
        ("flick:Fiber", "flickJoinAll") => AsyncIntrinsic::FiberJoinAll,
        ("flick:Fiber", "flickInterrupt") => AsyncIntrinsic::FiberInterrupt,
        ("flick:Fiber", "flickInterruptAs") => AsyncIntrinsic::FiberInterruptAs,
        ("flick:Fiber", "flickInterruptAll") => AsyncIntrinsic::FiberInterruptAll,
        ("flick:Fiber", "flickInterruptAllAs") => AsyncIntrinsic::FiberInterruptAllAs,
        ("flick:rt", "parallel") => AsyncIntrinsic::Parallel,
        ("flick:rt", "parallelAll") => AsyncIntrinsic::ParallelAll,
        ("flick:rt", "parallelResults") => AsyncIntrinsic::ParallelResults,
        ("flick:rt", "parallelExits") => AsyncIntrinsic::ParallelExits,
        ("flick:rt", "raceSuccess") => AsyncIntrinsic::RaceSuccess,
        ("flick:rt", "raceFirst") => AsyncIntrinsic::RaceFirst,
        ("flick:rt", "timeout") => AsyncIntrinsic::Timeout,
        ("flick:rt", "timeoutOption") => AsyncIntrinsic::TimeoutOption,
        ("flick:rt", "failureInspect" | "rawCauseInspect") => AsyncIntrinsic::FailureInspect,
        ("flick:rt", "testAdjust") => AsyncIntrinsic::TestClockAdjust,
        ("flick:rt", "testSetTime") => AsyncIntrinsic::TestClockSetTime,
        ("flick:Random", "withSeed") => AsyncIntrinsic::RandomWithSeed,
        ("flick:Deferred", "make") => AsyncIntrinsic::DeferredMake,
        ("flick:Deferred", "await") => AsyncIntrinsic::DeferredAwait,
        ("flick:Deferred", "flickDone") => AsyncIntrinsic::DeferredDone,
        ("flick:Deferred", "completeWith") => AsyncIntrinsic::DeferredCompleteWith,
        ("flick:Deferred", "isDone") => AsyncIntrinsic::DeferredIsDone,
        ("flick:Deferred", "poll") => AsyncIntrinsic::DeferredPoll,
        _ => return None,
    };
    (op.arity() == stages).then_some(op)
}

fn direct_fiber_group_intrinsic(from: &str, bind: &str, stages: usize) -> Option<AsyncIntrinsic> {
    match (from, bind, stages) {
        ("flick:FiberMap", "has", 2) => Some(AsyncIntrinsic::FiberGroupHas),
        ("flick:FiberMap", "remove", 2) => Some(AsyncIntrinsic::FiberGroupRemove),
        (_, "clear", 1) => Some(AsyncIntrinsic::FiberGroupClear),
        (_, "size", 1) => Some(AsyncIntrinsic::FiberGroupSize),
        (_, "join", 1) => Some(AsyncIntrinsic::FiberGroupJoin),
        (_, "awaitEmpty", 1) => Some(AsyncIntrinsic::FiberGroupAwaitEmpty),
        _ => None,
    }
}

fn fg_app4(b: &ExprBuilder, op: &str, opts: Expr, key: Expr, item: &str) -> Expr {
    b.app(b.app3(b.loc(op), opts, key, b.loc(item)), b.loc("grp"))
}

fn fiber_group_wrapper(
    b: &ExprBuilder,
    from: &str,
    bind: &str,
    stages: usize,
    tag_num: i32,
) -> Option<(Expr, AsyncIntrinsic)> {
    match (from, bind, stages) {
        (_, "flickMake", 1) => Some((
            b.lam(&["mk", "_u"], b.app(b.loc("mk"), b.int(tag_num))),
            AsyncIntrinsic::FiberGroupMake,
        )),
        ("flick:FiberSet" | "flick:FiberHandle", "flickRun", 3) => Some((
            b.lam(
                &["run", "opts", "task", "grp"],
                fg_app4(b, "run", b.loc("opts"), b.none(), "task"),
            ),
            AsyncIntrinsic::FiberGroupRun,
        )),
        ("flick:FiberMap", "flickRun", 4) => Some((
            b.lam(
                &["run", "opts", "k", "task", "grp"],
                fg_app4(b, "run", b.loc("opts"), b.some(b.loc("k")), "task"),
            ),
            AsyncIntrinsic::FiberGroupRun,
        )),
        ("flick:FiberSet", "flickAdd", 3) => {
            let opts = b.at(ExprKind::Record(vec![
                ("onlyIfMissing".into(), b.bool_(false)),
                ("propagateInterruption".into(), b.loc("prop")),
            ]));
            Some((
                b.lam(
                    &["add", "prop", "fib", "grp"],
                    fg_app4(b, "add", opts, b.none(), "fib"),
                ),
                AsyncIntrinsic::FiberGroupAdd,
            ))
        }
        ("flick:FiberHandle", "flickSet", 3) => Some((
            b.lam(
                &["add", "opts", "fib", "grp"],
                fg_app4(b, "add", b.loc("opts"), b.none(), "fib"),
            ),
            AsyncIntrinsic::FiberGroupAdd,
        )),
        ("flick:FiberMap", "flickSet", 4) => Some((
            b.lam(
                &["add", "opts", "k", "fib", "grp"],
                fg_app4(b, "add", b.loc("opts"), b.some(b.loc("k")), "fib"),
            ),
            AsyncIntrinsic::FiberGroupAdd,
        )),
        ("flick:FiberHandle", "flickGet", 1) => Some((
            b.lam(
                &["get", "grp"],
                b.app2(b.loc("get"), b.none(), b.loc("grp")),
            ),
            AsyncIntrinsic::FiberGroupGet,
        )),
        ("flick:FiberMap", "flickGet", 2) => Some((
            b.lam(
                &["get", "k", "grp"],
                b.app2(b.loc("get"), b.some(b.loc("k")), b.loc("grp")),
            ),
            AsyncIntrinsic::FiberGroupGet,
        )),
        _ => None,
    }
}

fn deferred_done(b: &ExprBuilder, exit_expr: Expr) -> Expr {
    b.app2(b.global("Deferred", "done"), exit_expr, b.loc("d"))
}

fn deferred_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("succeed", 2) => Some(b.lam(
            &["v", "d"],
            deferred_done(b, b.app(b.global("Exit", "succeed"), b.loc("v"))),
        )),
        ("sync", 2) => Some(b.lam(
            &["f", "d"],
            deferred_done(
                b,
                b.app(
                    b.global("Effect.Core", "exit"),
                    b.at(ExprKind::Task(Box::new(b.app(b.loc("f"), b.unit())))),
                ),
            ),
        )),
        ("fail", 2) => Some(b.lam(
            &["e", "d"],
            deferred_done(b, b.app(b.global("Exit", "fail"), b.loc("e"))),
        )),
        ("failSync", 2) => Some(b.lam(
            &["f", "d"],
            deferred_done(
                b,
                b.app(b.global("Exit", "fail"), b.app(b.loc("f"), b.unit())),
            ),
        )),
        ("flickFailCause", 2) => Some(b.lam(
            &["c", "d"],
            deferred_done(b, b.app(b.global("Exit", "fail_cause"), b.loc("c"))),
        )),
        ("flickFailCauseSync", 2) => Some(b.lam(
            &["f", "d"],
            deferred_done(
                b,
                b.app(b.global("Exit", "fail_cause"), b.app(b.loc("f"), b.unit())),
            ),
        )),
        ("complete", 2) => Some(b.lam(
            &["t", "d"],
            deferred_done(b, b.app(b.global("Effect.Core", "exit"), b.loc("t"))),
        )),
        ("into", 2) => Some(b.lam(
            &["d", "t"],
            b.app2(b.global("Deferred", "complete"), b.loc("t"), b.loc("d")),
        )),
        ("interrupt", 1) => Some(b.lam(
            &["d"],
            deferred_done(
                b,
                b.app(
                    b.global("Exit", "interrupt"),
                    b.some(b.app(b.global("Effect.Concurrency", "fiber_id"), b.unit())),
                ),
            ),
        )),
        ("interruptWith", 2) => Some(b.lam(
            &["fid", "d"],
            deferred_done(
                b,
                b.app(b.global("Exit", "interrupt"), b.some(b.loc("fid"))),
            ),
        )),
        _ => None,
    }
}
