package flickrt

import "runtime"

// Effect's `Effect` beyond the language: the parts `Effect.Core` and
// `Effect.Collections`, as the JS runtime's adapters of them.

// EffectDie dies with the text as the defect, a string as Effect's is.
func EffectDie(_ *Fiber, message Value) Value {
	panic(&Failure{&Cause{kind: causeDie, defect: message.(string), plain: true}})
}

// failWith fails with a whole cause; the empty cause too.
func failWith(c *Cause) Value {
	if c == nil {
		c = emptyCause
	}
	panic(&Failure{c})
}

func EffectFailCauseSync(_ *Fiber, f Value) Value { return failWith(causeOfFailure(Apply(f, Unit))) }

func EffectYieldNowWith(fb *Fiber, _ Value) Value { return EffectYieldNow(fb) }

func EffectZipWith(fb *Fiber, that, f, self Value) Value {
	a := Run(fb, self)
	return apply2(f, a, Run(fb, that))
}

func EffectDelay(fb *Fiber, d, task Value) Value {
	EffectSleep(fb, d)
	return Run(fb, task)
}

// EffectTimed is the task's value with how long it ran on the fiber's
// monotonic clock.
func EffectTimed(fb *Fiber, task Value) Value {
	clock := clockOf(fb)
	start := clock.monotonic()
	v := Run(fb, task)
	return []Value{elapsed(clock, start), v}
}

func EffectTimeoutOrElse(fb *Fiber, d, orElse, task Value) Value {
	return within(fb, toGoDuration(dur(d)), task, Identity, func() Value { return Run(fb, orElse) })
}

func EffectForever(fb *Fiber, task Value) Value {
	for {
		Run(fb, task)
		runtime.Gosched()
	}
}

// EffectEventually runs the task again, after letting others run, while
// its cause holds a typed failure, as Effect's catch takes one.
func EffectEventually(fb *Fiber, task Value) Value {
	for {
		v, c := catch(func() Value { return Run(fb, task) })
		if c == nil {
			return v
		}
		if c.firstOf(causeFail) == nil {
			panic(&Failure{c})
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

func EffectReplicateEffect(fb *Fiber, c, n, task Value) Value {
	return EffectAll(fb, c, EffectReplicate(n, task))
}

func EffectWhen(fb *Fiber, condition, task Value) Value {
	if Bool(Run(fb, condition)) {
		return Some(Run(fb, task))
	}
	return None
}

// The part `Effect.Collections`.

// tasksOf is the task of `f` of each element.
func tasksOf(f, xs Value) []Value {
	in := List(xs)
	out := make([]Value, len(in))
	for i, x := range in {
		out[i] = Apply(f, x)
	}
	return out
}

func EffectForEach(fb *Fiber, c, f, xs Value) Value { return EffectAll(fb, c, tasksOf(f, xs)) }

// resultOf is Effect's `result`: the value as `Ok`, the first typed error
// of the cause as `Err`; another cause goes on.
func resultOf(task Value) Value {
	return Eff(func(fb *Fiber) Value {
		v, c := catch(func() Value { return Run(fb, task) })
		if c == nil {
			return Ok(v)
		}
		if r := c.firstOf(causeFail); r != nil {
			return Err(r.err)
		}
		panic(&Failure{c})
	})
}

// partition is the values and the errors of the tasks of `f`.
func partition(fb *Fiber, c, f, xs Value) ([]Value, []Value) {
	tasks := tasksOf(f, xs)
	for i, t := range tasks {
		tasks[i] = resultOf(t)
	}
	passes, fails := []Value{}, []Value{}
	for _, r := range List(EffectAll(fb, c, tasks)) {
		if isOk(r) {
			passes = append(passes, value(r))
		} else {
			fails = append(fails, value(r))
		}
	}
	return passes, fails
}

func EffectPartition(fb *Fiber, c, f, xs Value) Value {
	passes, fails := partition(fb, c, f, xs)
	return []Value{passes, fails}
}

var validationError = Tag("Effect.Collections.ValidationError.ValidationError")

func EffectValidate(fb *Fiber, c, f, xs Value) Value {
	passes, fails := partition(fb, c, f, xs)
	if len(fails) > 0 {
		return Fail(NewData(validationError, Value(fails)))
	}
	return passes
}

func EffectFilter(fb *Fiber, c, f, xs Value) Value {
	in := List(xs)
	keep := List(EffectAll(fb, c, tasksOf(f, xs)))
	out := []Value{}
	for i, x := range in {
		if Bool(keep[i]) {
			out = append(out, x)
		}
	}
	return out
}

// passed is what the `Ok`s of filter results hold.
func passed(results []Value) Value {
	out := []Value{}
	for _, r := range results {
		if isOk(r) {
			out = append(out, value(r))
		}
	}
	return out
}

func EffectFilterMap(_ *Fiber, f, xs Value) Value {
	in := List(xs)
	results := make([]Value, len(in))
	for i, x := range in {
		results[i] = Apply(f, x)
	}
	return passed(results)
}

func EffectFilterMapEffect(fb *Fiber, c, f, xs Value) Value {
	return passed(List(EffectAll(fb, c, tasksOf(f, xs))))
}

func EffectFindFirst(fb *Fiber, f, xs Value) Value {
	for _, x := range List(xs) {
		if Bool(Run(fb, Apply(f, x))) {
			return Some(x)
		}
	}
	return None
}

func EffectFindFirstFilter(fb *Fiber, f, xs Value) Value {
	for _, x := range List(xs) {
		if r := Run(fb, Apply(f, x)); isOk(r) {
			return Some(value(r))
		}
	}
	return None
}

// EffectFirstSuccessOf runs the tasks in turn until one ends without a
// typed failure: how that one ends, or how the last one does.
func EffectFirstSuccessOf(fb *Fiber, tasks Value) Value {
	in := List(tasks)
	if len(in) == 0 {
		panic(Defect{"Error: Received an empty collection of effects"})
	}
	for _, t := range in[:len(in)-1] {
		v, c := catch(func() Value { return Run(fb, t) })
		if c == nil {
			return v
		}
		if c.firstOf(causeFail) == nil {
			panic(&Failure{c})
		}
	}
	return Run(fb, in[len(in)-1])
}

var noSuchElementTag = Tag("Cause.NoSuchElementError.NoSuchElementError")

// noSuchElement is Effect's NoSuchElementError as Flick's; Effect's own
// carries no message.
func noSuchElement(message string) Value { return NewData(noSuchElementTag, message) }

func EffectHead(fb *Fiber, task Value) Value {
	xs := List(Run(fb, task))
	if len(xs) == 0 {
		return Fail(noSuchElement(""))
	}
	return xs[0]
}

func EffectReduce(fb *Fiber, zero, f, xs Value) Value {
	acc := zero
	for _, x := range List(xs) {
		acc = Run(fb, apply2(f, acc, x))
	}
	return acc
}

func EffectFilterOrElse(fb *Fiber, predicate, orElse, task Value) Value {
	v := Run(fb, task)
	if Bool(Apply(predicate, v)) {
		return v
	}
	return Run(fb, Apply(orElse, v))
}

func EffectFilterMapOrElse(fb *Fiber, filter, orElse, task Value) Value {
	r := Apply(filter, Run(fb, task))
	if isOk(r) {
		return value(r)
	}
	return Run(fb, Apply(orElse, value(r)))
}

func EffectFilterOrFail(fb *Fiber, predicate, orFail, task Value) Value {
	v := Run(fb, task)
	if Bool(Apply(predicate, v)) {
		return v
	}
	return Fail(Apply(orFail, v))
}

func EffectFilterMapOrFail(fb *Fiber, filter, orFail, task Value) Value {
	r := Apply(filter, Run(fb, task))
	if isOk(r) {
		return value(r)
	}
	return Fail(Apply(orFail, value(r)))
}
