package flickrt

// Services and layers. A service is its key, `Module.S`; an operation looks
// its service up in the environment of the fiber it runs in.

import (
	"sync"
	"sync/atomic"
)

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

// operation finds the innermost implementation with an arm for name. Only
// partial handlers forward a missing operation: a complete service still
// shadows the outer one, including when a malformed record lacks the field.
func (e *env) operation(key, name string) Value {
	for ; e != nil; e = e.next {
		if e.key != key {
			continue
		}
		impl := e.v.(*Record)
		if e.partial && impl.index(name) < 0 {
			continue
		}
		return impl.Get(name)
	}
	panic(Defect{"Service not found: " + key})
}

// An Op is `S.op` as Perform runs it: a package variable of the code
// that applies it to all its arguments up to its first effectful stage.
type Op struct {
	key, name string
	slot      uint32 // its entry in an opCache
}

var opSlots atomic.Uint32

// NewOp is the Op of the operation `name` of the service `key`.
func NewOp(key, name string) *Op {
	return &Op{key: key, name: name, slot: (opSlots.Add(1) - 1) % uint32(len(opCache{}))}
}

// Perform is `S.op` applied to all its arguments up to its first effectful
// stage and run, as Run(fb, Apply(fb, Operation(…), args…)) is, without the
// function and the task Operation makes for each call.
func Perform(fb *Fiber, op *Op, args ...Value) Value {
	Check(fb)
	f := fb.state.ops.find(fb.env, op)
	for _, a := range args {
		f = Apply(fb, f, a)
	}
	return Run(fb, f)
}

// An opCache holds, by Op, the implementation an operation of the fiber
// found last and the environment it was found in. An environment is never
// changed once made, and the entry keeps it alive, so no other one is made
// at its address: the same environment finds the same implementation, and
// a `provide` or a handler, which make a new one, look it up again. Only
// the goroutine that runs the fiber touches it, as `mask`.
type opCache [4]struct {
	op   *Op
	env  *env
	impl Value
}

func (c *opCache) find(e *env, op *Op) Value {
	entry := &c[op.slot]
	if entry.op == op && entry.env == e {
		return entry.impl
	}
	impl := e.operation(op.key, op.name)
	entry.op, entry.env, entry.impl = op, e, impl
	return impl
}

func operation(key, name string, arity int, dicts []Value) Value {
	call := func(args []Value) Value {
		return Eff(func(fb *Fiber) Value {
			f := fb.env.operation(key, name)
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
