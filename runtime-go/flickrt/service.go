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
	op, _ := operations.LoadOrStore(id, operation(key, name, arity, nil))
	return op
}

// OperationGiven is Operation of an operation with class constraints: its
// implementation takes dicts first. It is not cached, as a dictionary may be
// one the caller was given.
func OperationGiven(key, name string, arity int, dicts ...Value) Value {
	return operation(key, name, arity, dicts)
}

type operationId struct {
	key, name string
	arity     int
}

var operations sync.Map

func operation(key, name string, arity int, dicts []Value) Value {
	call := func(args []Value) Value {
		return Eff(func(fb *Fiber) Value {
			impl, ok := fb.env.get(key)
			if !ok {
				panic(Defect{"Service not found: " + key})
			}
			f := impl.(*Record).Get(name)
			if dicts != nil {
				f = Inst(fb, f, dicts...)
			}
			for _, a := range args {
				f = Apply(fb, f, a)
			}
			return Run(fb, f)
		})
	}
	var collect func(args []Value) Value
	collect = func(args []Value) Value {
		if len(args) == arity {
			return call(args)
		}
		return Fn(func(fb *Fiber, a Value) Value { return collect(append(args[:len(args):len(args)], a)) })
	}
	return collect(nil)
}
