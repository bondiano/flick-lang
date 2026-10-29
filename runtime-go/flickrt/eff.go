package flickrt

// Tasks made from values and other tasks: the `Functor`, `Applicative` and
// `Monad` instances of `Eff`, and Effect's combinators of tasks.

func Succeed(x Value) Value { return Eff(func(*Fiber) Value { return x }) }

func MapEff(f, task Value) Value {
	return Eff(func(fb *Fiber) Value { return Apply(f, Run(fb, task)) })
}

func FlatMapEff(f, task Value) Value {
	return Eff(func(fb *Fiber) Value { return Run(fb, Apply(f, Run(fb, task))) })
}

func BindEff(task, f Value) Value { return FlatMapEff(f, task) }

// Map2Eff runs the first task, then the second.
func Map2Eff(f, a, b Value) Value {
	return Eff(func(fb *Fiber) Value {
		x := Run(fb, a)
		return apply2(f, x, Run(fb, b))
	})
}

// ListMapEffect runs the task of `f` of each element, in order.
func ListMapEffect(fb *Fiber, f, xs Value) Value {
	in := List(xs)
	out := make([]Value, len(in))
	for i, x := range in {
		out[i] = Run(fb, Apply(f, x))
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
	c := concurrency.(*Data)
	switch ConstructorName(c.Tag) {
	case "Sequential":
		return ListMapEffect(fb, Fn(Identity), tasks)
	case "Bounded":
		return all(fb, List(tasks), max(1, int(c.Fields[0].(int32))))
	}
	return all(fb, List(tasks), 0)
}
