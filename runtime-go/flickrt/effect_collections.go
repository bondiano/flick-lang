package flickrt

// The part `Effect.Collections` of `Effect`: a task per element of a list,
// run by an `Effect.Concurrency`, and filters of a task's value.

// byConcurrency runs the tasks one after another for `Sequential`, else
// concurrently, at most n at a time for `Bounded n`: their results in
// order, or the first failure.
func byConcurrency(fb *Fiber, concurrency Value, tasks []Value) []Value {
	c := concurrency.(*Data)
	switch ConstructorName(c.Tag) {
	case "Sequential":
		out := make([]Value, len(tasks))
		for i, t := range tasks {
			out[i] = Run(fb, t)
		}
		return out
	case "Bounded":
		return all(fb, tasks, max(1, int(c.Fields[0].(int32))))
	}
	return all(fb, tasks, 0)
}

// each is the task of `f` of each element.
func each(fb *Fiber, f, xs Value) []Value {
	in := List(xs)
	tasks := make([]Value, len(in))
	for i, x := range in {
		tasks[i] = Apply(fb, f, x)
	}
	return tasks
}

func EffectForEach(fb *Fiber, concurrency, f, xs Value) Value {
	return byConcurrency(fb, concurrency, each(fb, f, xs))
}

var validationError = Tag("Effect.Collections.ValidationError.ValidationError")

// partition runs the task of `f` of each element: the values, and the
// typed errors of those that failed with one.
func partition(fb *Fiber, concurrency, f, xs Value) ([]Value, []Value) {
	tasks := each(fb, f, xs)
	for i, t := range tasks {
		tasks[i] = Eff(func(fb *Fiber) Value {
			v, c := caught(fb, func() Value { return Run(fb, t) })
			if c == nil {
				return Ok(v)
			}
			if r := c.firstOf(causeFail); r != nil {
				return Err(r.err)
			}
			return failCause(c)
		})
	}
	passes, fails := []Value{}, []Value{}
	for _, r := range byConcurrency(fb, concurrency, tasks) {
		if isOk(r) {
			passes = append(passes, value(r))
		} else {
			fails = append(fails, value(r))
		}
	}
	return passes, fails
}

func EffectPartition(fb *Fiber, concurrency, f, xs Value) Value {
	passes, fails := partition(fb, concurrency, f, xs)
	return []Value{passes, fails}
}

// EffectValidate fails with a ValidationError of every typed error.
func EffectValidate(fb *Fiber, concurrency, f, xs Value) Value {
	passes, fails := partition(fb, concurrency, f, xs)
	if len(fails) > 0 {
		return Fail(NewData(validationError, fails))
	}
	return passes
}

// keeping runs the task of `f` of each element: what `keep` makes of
// its value goes out.
//
// shortcut: in the order of the elements, where the JS target's concurrent forms keep the order the tasks end in; the same for tasks that do not wait
func keeping(fb *Fiber, concurrency, f, xs Value, keep func(x, v Value) (Value, bool)) Value {
	in := List(xs)
	out := []Value{}
	for i, v := range byConcurrency(fb, concurrency, each(fb, f, xs)) {
		if kept, ok := keep(in[i], v); ok {
			out = append(out, kept)
		}
	}
	return out
}

func EffectFilter(fb *Fiber, concurrency, f, xs Value) Value {
	return keeping(fb, concurrency, f, xs, func(x, v Value) (Value, bool) { return x, v.(bool) })
}

func EffectFilterMapEffect(fb *Fiber, concurrency, f, xs Value) Value {
	return keeping(fb, concurrency, f, xs, func(_, r Value) (Value, bool) { return okValue(r) })
}

// okValue is the value of an `Ok`.
func okValue(r Value) (Value, bool) {
	if isOk(r) {
		return value(r), true
	}
	return nil, false
}

func EffectFilterMap(fb *Fiber, f, xs Value) Value {
	out := []Value{}
	for _, x := range List(xs) {
		if v, ok := okValue(Apply(fb, f, x)); ok {
			out = append(out, v)
		}
	}
	return out
}

func EffectFindFirst(fb *Fiber, f, xs Value) Value {
	for _, x := range List(xs) {
		if Bool(Run(fb, Apply(fb, f, x))) {
			return Some(x)
		}
	}
	return None
}

func EffectFindFirstFilter(fb *Fiber, f, xs Value) Value {
	for _, x := range List(xs) {
		if v, ok := okValue(Run(fb, Apply(fb, f, x))); ok {
			return Some(v)
		}
	}
	return None
}

// EffectFirstSuccessOf runs the tasks in turn until one succeeds; the
// last one ends as it does. None is a defect.
func EffectFirstSuccessOf(fb *Fiber, tasks Value) Value {
	in := List(tasks)
	if len(in) == 0 {
		return Die("Error: Received an empty collection of effects")
	}
	for _, t := range in[:len(in)-1] {
		v, c := caught(fb, func() Value { return Run(fb, t) })
		if c == nil {
			return v
		}
		if !c.has(causeFail) {
			return failCause(c)
		}
	}
	return Run(fb, in[len(in)-1])
}

var noSuchElementError = Tag("Cause.NoSuchElementError.NoSuchElementError")

func EffectHead(fb *Fiber, task Value) Value {
	xs := List(Run(fb, task))
	if len(xs) == 0 {
		return Fail(NewData(noSuchElementError, ""))
	}
	return xs[0]
}

func EffectReduce(fb *Fiber, zero, f, xs Value) Value {
	acc := zero
	for _, x := range List(xs) {
		acc = Run(fb, apply2(fb, f, acc, x))
	}
	return acc
}

func EffectFilterOrElse(fb *Fiber, p, orElse, task Value) Value {
	v := Run(fb, task)
	if Bool(Apply(fb, p, v)) {
		return v
	}
	return Run(fb, Apply(fb, orElse, v))
}

func EffectFilterMapOrElse(fb *Fiber, filter, orElse, task Value) Value {
	r := Apply(fb, filter, Run(fb, task))
	if v, ok := okValue(r); ok {
		return v
	}
	return Run(fb, Apply(fb, orElse, value(r)))
}

func EffectFilterOrFail(fb *Fiber, p, orFail, task Value) Value {
	v := Run(fb, task)
	if Bool(Apply(fb, p, v)) {
		return v
	}
	return Fail(Apply(fb, orFail, v))
}

func EffectFilterMapOrFail(fb *Fiber, filter, orFail, task Value) Value {
	r := Apply(fb, filter, Run(fb, task))
	if v, ok := okValue(r); ok {
		return v
	}
	return Fail(Apply(fb, orFail, value(r)))
}
