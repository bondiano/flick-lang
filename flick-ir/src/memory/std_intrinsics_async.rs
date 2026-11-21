/// Cooperative concurrency, fiber, interruption, deferred, fiber-group,
/// virtual test-clock, and seeded-random intrinsics (Stage 4A).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AsyncIntrinsic {
    // Fibers & supervision
    FiberSpawn,
    FiberJoin,
    FiberAwait,
    FiberAwaitAll,
    FiberJoinAll,
    FiberInterrupt,
    FiberInterruptAs,
    FiberInterruptAll,
    FiberInterruptAllAs,
    EffectFiberId,
    // Cooperative scheduling, time, and interruption masks
    EffectYieldNow,
    EffectYieldNowWith,
    EffectSleep,
    EffectNever,
    EffectInterrupt,
    EffectInterruptible,
    EffectUninterruptibleMask,
    EffectInterruptibleMask,
    EffectEnsuring,
    EffectOnExitPrimitive,
    EffectCatchCause,
    EffectOrDie,
    EffectTimed,
    EffectCached,
    EffectCachedWithTtl,
    EffectCachedInvalidateWithTtl,
    // Concurrency combinators
    Parallel,
    ParallelAll,
    ParallelResults,
    ParallelExits,
    RaceSuccess,
    RaceFirst,
    RaceAll,
    RaceAllFirst,
    Timeout,
    TimeoutOption,
    TimeoutOrElse,
    EffectAll,
    // Deferred one-shot latch
    DeferredMake,
    DeferredAwait,
    DeferredDone,
    DeferredCompleteWith,
    DeferredIsDone,
    DeferredPoll,
    DeferredInterruptWith,
    // FiberSet / FiberMap / FiberHandle (owner-hooked C1 groups)
    FiberGroupMake,
    FiberGroupRun,
    FiberGroupAdd,
    FiberGroupGet,
    FiberGroupHas,
    FiberGroupRemove,
    FiberGroupClear,
    FiberGroupSize,
    FiberGroupJoin,
    FiberGroupAwaitEmpty,
    // Failure inspection, virtual test clock, and seeded host random
    FailureInspect,
    TestClockAdjust,
    TestClockSetTime,
    RandomWithSeed,
}

impl AsyncIntrinsic {
    pub fn arity(self) -> usize {
        match self {
            Self::FiberJoin
            | Self::FiberAwait
            | Self::FiberAwaitAll
            | Self::FiberJoinAll
            | Self::FiberInterrupt
            | Self::FiberInterruptAll
            | Self::EffectFiberId
            | Self::EffectYieldNow
            | Self::EffectYieldNowWith
            | Self::EffectSleep
            | Self::EffectNever
            | Self::EffectInterrupt
            | Self::EffectInterruptible
            | Self::EffectUninterruptibleMask
            | Self::EffectInterruptibleMask
            | Self::EffectOrDie
            | Self::EffectTimed
            | Self::EffectCached
            | Self::ParallelResults
            | Self::ParallelExits
            | Self::RaceAll
            | Self::RaceAllFirst
            | Self::DeferredMake
            | Self::DeferredAwait
            | Self::DeferredIsDone
            | Self::DeferredPoll
            | Self::FiberGroupMake
            | Self::FiberGroupClear
            | Self::FiberGroupSize
            | Self::FiberGroupJoin
            | Self::FiberGroupAwaitEmpty
            | Self::FailureInspect
            | Self::TestClockAdjust
            | Self::TestClockSetTime => 1,
            Self::FiberInterruptAs
            | Self::FiberInterruptAllAs
            | Self::EffectEnsuring
            | Self::EffectCatchCause
            | Self::EffectCachedWithTtl
            | Self::EffectCachedInvalidateWithTtl
            | Self::Parallel
            | Self::ParallelAll
            | Self::RaceSuccess
            | Self::RaceFirst
            | Self::Timeout
            | Self::TimeoutOption
            | Self::EffectAll
            | Self::DeferredDone
            | Self::DeferredCompleteWith
            | Self::DeferredInterruptWith
            | Self::FiberGroupGet
            | Self::FiberGroupHas
            | Self::FiberGroupRemove
            | Self::RandomWithSeed => 2,
            Self::FiberSpawn
            | Self::EffectOnExitPrimitive
            | Self::TimeoutOrElse => 3,
            Self::FiberGroupRun | Self::FiberGroupAdd => 4,
        }
    }
}

/// Dynamic `Layer`, `MemoMap`, and `Context` / `Context.Reference` intrinsics (Stage 4A).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LayerIntrinsic {
    // Context & fiber-local References
    ContextEmpty,
    ContextMake,
    ContextAdd,
    ContextAddOrOmit,
    ContextGet,
    ContextGetOption,
    ContextMerge,
    ContextMergeAll,
    ContextPick,
    ContextOmit,
    ContextReference,
    ContextRead,
    ContextWithReference,
    // Dynamic Layer & MemoMap
    LayerEmpty,
    LayerSucceedContext,
    LayerFromBuild,
    LayerFresh,
    LayerMergeAll,
    LayerOrDie,
    LayerBuild,
    LayerBuildWithMemoMap,
    LayerMakeMemoMap,
    LayerForkMemoMap,
    LayerCurrentMemoMap,
    LayerProvideContext,
    LayerLookupService,
    // Manual scope ownership and captured environment execution (Stage 4B)
    OwnerNew,
    OwnerRun,
    OwnerClose,
    EnvCapture,
    EnvRun,
    // Stage 4D fiber-ref mutation & runtime fiber metric counter
    ContextSetFiberRef,
    MetricRuntimeFibersStarted,
}

impl LayerIntrinsic {
    pub fn arity(self) -> usize {
        match self {
            Self::LayerEmpty => 0,
            Self::ContextEmpty
            | Self::ContextRead
            | Self::LayerSucceedContext
            | Self::LayerFresh
            | Self::LayerOrDie
            | Self::LayerBuild
            | Self::LayerMakeMemoMap
            | Self::LayerForkMemoMap
            | Self::LayerCurrentMemoMap
            | Self::LayerLookupService
            | Self::OwnerNew
            | Self::EnvCapture
            | Self::MetricRuntimeFibersStarted => 1,
            Self::ContextMake
            | Self::ContextGet
            | Self::ContextGetOption
            | Self::ContextMerge
            | Self::ContextMergeAll
            | Self::ContextPick
            | Self::ContextOmit
            | Self::ContextReference
            | Self::LayerFromBuild
            | Self::LayerMergeAll
            | Self::LayerBuildWithMemoMap
            | Self::LayerProvideContext
            | Self::OwnerRun
            | Self::OwnerClose
            | Self::EnvRun
            | Self::ContextSetFiberRef => 2,
            Self::ContextAdd | Self::ContextAddOrOmit | Self::ContextWithReference => 3,
        }
    }
}
