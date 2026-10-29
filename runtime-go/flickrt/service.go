package flickrt

// Services and layers. A service is its key, `Module.S`; an operation looks
// its service up in the environment of the fiber it runs in.

import "sync"

// Operation is `S.op`: a function of the operation's arguments up to its
// first effectful stage, whose task applies the implementation to them.
// Code reads it where it calls it, so each one is built once.
func Operation(key, name string, arity int) Value {
	id := operationId{key, name, arity}
	if op, ok := operations.Load(id); ok {
		return op
	}
	op, _ := operations.LoadOrStore(id, operation(key, name, arity))
	return op
}

type operationId struct {
	key, name string
	arity     int
}

var operations sync.Map

func operation(key, name string, arity int) Value {
	call := func(args []Value) Value {
		return Eff(func(fb *Fiber) Value {
			impl, ok := fb.env.get(key)
			if !ok {
				panic(Defect{"Service not found: " + key})
			}
			f := impl.(*Record).Get(name)
			for _, a := range args {
				f = Apply(f, a)
			}
			return Run(fb, f)
		})
	}
	var collect func(args []Value) Value
	collect = func(args []Value) Value {
		if len(args) == arity {
			return call(args)
		}
		return Fn(func(a Value) Value { return collect(append(args[:len(args):len(args)], a)) })
	}
	return collect(nil)
}

// A Layer builds services in a fiber.
type Layer func(*Fiber) map[string]Value

func LayerSucceed(key, impl Value) Value {
	return Layer(func(*Fiber) map[string]Value { return map[string]Value{key.(string): impl} })
}

// LayerMerge builds both layers, the second's services overriding the first's.
func LayerMerge(a, b Value) Value {
	return Layer(func(fb *Fiber) map[string]Value {
		out := a.(Layer)(fb)
		for k, v := range b.(Layer)(fb) {
			out[k] = v
		}
		return out
	})
}

// Provide runs `body` with the services `layer` builds added to the fiber's.
//
// shortcut: the layer is built in the caller's owner, so what it acquires is not released after the body; give provide its own owner with scopes
func Provide(fb *Fiber, layer, body Value) Value {
	inner := *fb
	for k, v := range layer.(Layer)(fb) {
		inner.env = inner.env.with(k, v)
	}
	return Run(&inner, body)
}
