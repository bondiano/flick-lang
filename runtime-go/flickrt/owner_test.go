package flickrt

import "testing"

// A hook a supervisor adds while its owner closes still runs before the
// finalizers (docs/formal/runtime.md §3.3, C1).
func TestAHookAddedWhileClosingRuns(t *testing.T) {
	var order []string
	Run(rootFiber(), Eff(func(fb *Fiber) Value {
		return Scope(fb, func(fb *Fiber, owner, _ Value) Value {
			o := owner.(*Owner)
			o.add(fb, func(*Fiber, *Cause) Value { order = append(order, "finalizer"); return Unit })
			supervisor := Fn(func(fb *Fiber, _ Value) Value {
				o.onClose(fb, func(*Fiber) { order = append(order, "hook") })
				return Unit
			})
			return FiberSpawn(fb, owner, supervisor, Eff(func(fb *Fiber) Value { wait(fb, make(chan struct{})); return Unit }))
		})
	}))
	if len(order) != 2 || order[0] != "hook" || order[1] != "finalizer" {
		t.Errorf("the hook runs, before the finalizers: %v", order)
	}
}

// A failing supervisor of an event listener reaches the owner's root.
func TestAFailingListenerSupervisorIsReported(t *testing.T) {
	broken := Fn(func(*Fiber, Value) Value { panic(Defect{"Error: broken"}) })
	owner := newOwner()
	owner.defects = &defects{}
	fb := &Fiber{state: newState(), owner: owner}
	l := EventsListen(fb, owner, 1, broken, Fn(func(*Fiber, Value) Value { return Succeed(Unit) })).(*listener)
	l.fail(&Cause{kind: causeDie, defect: "Error: handler"})
	owner.close(fb, nil)
	if owner.defects.all() == nil {
		t.Error("the supervisor's failure is reported")
	}
}
