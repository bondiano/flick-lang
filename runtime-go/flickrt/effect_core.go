package flickrt

import "math/big"

// The rest of the part `Effect.Core` of `Effect`: one task timed, delayed,
// repeated or zipped.

func EffectFailCauseSync(fb *Fiber, f Value) Value {
	return failCause(causeOfFailure(Apply(fb, f, Unit)))
}

// EffectYieldNowWith yields; Go's scheduler has no priorities.
func EffectYieldNowWith(fb *Fiber, _ Value) Value { return EffectYieldNow(fb) }

// EffectZipWith runs `self`, then `that`: `f` of both values.
func EffectZipWith(fb *Fiber, that, f, self Value) Value {
	a := Run(fb, self)
	return Apply2(fb, f, a, Run(fb, that))
}

func EffectDelay(fb *Fiber, d, task Value) Value {
	sleepFor(fb, dur(d))
	return Run(fb, task)
}

// EffectTimed is how long the task took by the clock's monotonic time,
// and its value.
func EffectTimed(fb *Fiber, task Value) Value {
	clock := clockOf(fb)
	start := clock.monotonic()
	v := Run(fb, task)
	return []Value{fromNanos(new(big.Int).Sub(clock.monotonic(), start)), v}
}

// EffectTimeoutOrElse is the task's value, or `orElse`'s when the duration
// passes first, the task interrupted.
func EffectTimeoutOrElse(fb *Fiber, d, orElse, task Value) Value {
	if dur(d).infinite() {
		return Run(fb, task)
	}
	return timeoutAfter(fb, millisOf(dur(d)), task, Identity, func() Value { return Run(fb, orElse) })
}

// EffectForever runs the task again and again, yielding after each run,
// until it fails.
func EffectForever(fb *Fiber, task Value) Value {
	for {
		Run(fb, task)
		EffectYieldNow(fb)
	}
}

// EffectEventually runs the task until it succeeds, yielding after each
// typed failure; a cause without one fails as it is.
func EffectEventually(fb *Fiber, task Value) Value {
	for {
		v, c := caught(fb, func() Value { return Run(fb, task) })
		if c == nil {
			return v
		}
		if !c.has(causeFail) {
			return failCause(c)
		}
		EffectYieldNow(fb)
	}
}

func EffectReplicate(n, task Value) Value {
	out := make([]Value, max(0, int(n.(int32))))
	for i := range out {
		out[i] = task
	}
	return out
}

func EffectReplicateEffect(fb *Fiber, concurrency, n, task Value) Value {
	return byConcurrency(fb, concurrency, EffectReplicate(n, task).([]Value))
}

func EffectWhen(fb *Fiber, condition, task Value) Value {
	if !Bool(Run(fb, condition)) {
		return None
	}
	return Some(Run(fb, task))
}

// EffectResourceAuto is a manual resource that a fiber of its region
// refreshes by the schedule.
func EffectResourceAuto(fb *Fiber, schedule, acquire Value) Value {
	r := ResourceManual(fb, acquire)
	refresh := Eff(func(fb *Fiber) Value { return ResourceRefresh(fb, r) })
	forkIn(fb, fb.owner, func(fb *Fiber) Value { return EffectRepeat(fb, schedule, refresh) })
	return r
}
