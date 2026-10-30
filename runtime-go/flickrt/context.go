package flickrt

import "sync"

// The `Context` module: built services as values, and references, which
// are fiber-local values with a default.

func ctxOf(v Value) Services { return v.(Services) }

func ContextEmpty() Value { return Services{} }

func ContextMake(key, impl Value) Value { return Services{key.(string): impl} }

func ContextAdd(key, impl, ctx Value) Value {
	return ctxOf(ctx).with(Services{key.(string): impl})
}

// ContextAddOrOmit is the context with the service of `key` when `impl`
// is one, without it otherwise.
func ContextAddOrOmit(key, impl, ctx Value) Value {
	if v, ok := some(impl); ok {
		return ContextAdd(key, v, ctx)
	}
	return ContextOmit(key, ctx)
}

func ContextGet(key, ctx Value) Value {
	if v, ok := ctxOf(ctx)[key.(string)]; ok {
		return v
	}
	panic(Defect{"Service not found: " + key.(string)})
}

func ContextGetOption(key, ctx Value) Value {
	if v, ok := ctxOf(ctx)[key.(string)]; ok {
		return Some(v)
	}
	return None
}

func ContextGetOrElse(fb *Fiber, key, orElse, ctx Value) Value {
	if v, ok := ctxOf(ctx)[key.(string)]; ok {
		return v
	}
	return Apply(fb, orElse, Unit)
}

// ContextMerge is `Context.merge that self`: the services of `that`
// replace those of `self`.
func ContextMerge(that, self Value) Value { return ctxOf(self).with(ctxOf(that)) }

func ContextMergeAll(first, rest Value) Value {
	out := ctxOf(first)
	for _, c := range List(rest) {
		out = out.with(ctxOf(c))
	}
	return out
}

func ContextPick(key, ctx Value) Value {
	if v, ok := ctxOf(ctx)[key.(string)]; ok {
		return Services{key.(string): v}
	}
	return Services{}
}

func ContextOmit(key, ctx Value) Value {
	out := Services{}
	for k, v := range ctxOf(ctx) {
		if k != key.(string) {
			out[k] = v
		}
	}
	return out
}

// A reference is read from the fiber's environment under its key, or is
// its default, computed once.
type reference struct {
	key   string
	once  sync.Once
	make  Value
	value Value
}

func ContextReference(key, make Value) Value { return &reference{key: key.(string), make: make} }

func (r *reference) fallback(fb *Fiber) Value {
	r.once.Do(func() { r.value = Apply(fb, r.make, Unit) })
	return r.value
}

func ContextRead(fb *Fiber, ref Value) Value {
	r := ref.(*reference)
	if v, ok := fb.env.get(r.key); ok {
		return v
	}
	return r.fallback(fb)
}

func ContextWithReference(fb *Fiber, ref, value, task Value) Value {
	return withEnv(fb, ref.(*reference).key, value, task)
}
