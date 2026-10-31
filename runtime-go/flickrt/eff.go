package flickrt

// Tasks made from values and other tasks: the `Functor`, `Applicative` and
// `Monad` instances of `Eff`, and Effect's combinators of tasks.

func Succeed(x Value) Value { return Eff(func(*Fiber) Value { return x }) }

func MapEff(f, task Value) Value {
	return Eff(func(fb *Fiber) Value { return Apply(fb, f, Run(fb, task)) })
}

func FlatMapEff(f, task Value) Value {
	return Eff(func(fb *Fiber) Value { return Run(fb, Apply(fb, f, Run(fb, task))) })
}

func BindEff(task, f Value) Value { return FlatMapEff(f, task) }

// Map2Eff runs the first task, then the second.
func Map2Eff(f, a, b Value) Value {
	return Eff(func(fb *Fiber) Value {
		x := Run(fb, a)
		return apply2(fb, f, x, Run(fb, b))
	})
}

// ListMapEffect runs the task of `f` of each element, in order.
func ListMapEffect(fb *Fiber, f, xs Value) Value {
	in := List(xs)
	out := make([]Value, len(in))
	for i, x := range in {
		out[i] = Run(fb, Apply(fb, f, x))
	}
	return out
}

var EffectVoid Value = Succeed(Unit)

// EffectZip runs `self`, then `that`.
func EffectZip(fb *Fiber, that, self Value) Value {
	a := Run(fb, self)
	return []Value{a, Run(fb, that)}
}

// EffectAll runs the tasks by the concurrency given, the results in order.
func EffectAll(fb *Fiber, concurrency, tasks Value) Value {
	return byConcurrency(fb, concurrency, List(tasks))
}
