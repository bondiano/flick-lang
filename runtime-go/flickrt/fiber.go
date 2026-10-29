package flickrt

// A Fiber is what effectful code runs in: the generated code threads it
// through every effectful call. Its derivations (another environment of
// services, another owner) share the fiber's state.
type Fiber struct {
	state *fiberState
	env   map[string]Value // the services provided, by key; never mutated
}

type fiberState struct {
	id int64
}

// Eff is a task: a computation run each time it is awaited, in the fiber
// that awaits it.
type Eff func(*Fiber) Value

// Run runs a task in a fiber.
func Run(fb *Fiber, task Value) Value { return task.(Eff)(fb) }

// rootFiber is the fiber `main` runs in.
func rootFiber() *Fiber { return &Fiber{state: &fiberState{}} }

// LiftStage adapts a function of one stage where one of another is
// expected: effectful stages (Sync and Runtime) give a task, pure ones a
// value; `result`, when given, adapts what the function returns.
func LiftStage(fromEff, toEff bool, result func(Value) Value) func(Value) Value {
	lifted := func(v Value) Value {
		if result != nil {
			return result(v)
		}
		return v
	}
	return func(f Value) Value {
		return Fn(func(x Value) Value {
			switch {
			case fromEff && toEff && result != nil:
				return Eff(func(fb *Fiber) Value { return result(Run(fb, Apply(f, x))) })
			case fromEff && toEff:
				return Apply(f, x)
			case toEff:
				return Eff(func(*Fiber) Value { return lifted(Apply(f, x)) })
			case fromEff:
				// shortcut: a task where a pure value is expected runs in a fiber of its own, without the caller's services; thread the caller's when a program needs them
				return lifted(Run(rootFiber(), Apply(f, x)))
			}
			return lifted(Apply(f, x))
		})
	}
}

// LiftEff adapts the result of a task.
func LiftEff(inner func(Value) Value) func(Value) Value {
	return func(task Value) Value {
		return Eff(func(fb *Fiber) Value { return inner(Run(fb, task)) })
	}
}
