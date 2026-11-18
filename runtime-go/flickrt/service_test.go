package flickrt

import "testing"

func performed(who string) Value {
	return Rec("name", Fn(func(*Fiber, Value) Value { return Eff(func(*Fiber) Value { return who }) }))
}

// Each Perform finds the innermost provider where it runs: one provided after
// the last call, and the outer one again once the fiber is back in it.
func TestPerformFindsTheProviderOfEachEnvironment(t *testing.T) {
	key := "test.PerformAgain"
	op := NewOp(key, "name")
	fb := rootFiber()
	fb.env = fb.env.with(key, performed("outer"))
	got := []Value{Perform(fb, op, Unit)}
	for _, who := range []string{"first", "second"} {
		inner := *fb
		inner.env = fb.env.with(key, performed(who))
		got = append(got, Perform(&inner, op, Unit), Perform(fb, op, Unit))
	}
	want := []Value{"outer", "first", "outer", "second", "outer"}
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("got %v, want %v", got, want)
		}
	}
}

// Operations whose entries in the cache are the same one take it in turns.
func TestPerformOfOperationsSharingAnEntry(t *testing.T) {
	ops := make([]*Op, len(opCache{})+1)
	fb := rootFiber()
	for i := range ops {
		key := "test.PerformShared" + string(rune('A'+i))
		ops[i] = NewOp(key, "name")
		fb.env = fb.env.with(key, performed(key))
	}
	first, last := ops[0], ops[len(ops)-1]
	if first.slot != last.slot {
		t.Fatalf("slots %d and %d: the test needs a shared one", first.slot, last.slot)
	}
	for range 3 {
		for _, op := range []*Op{first, last} {
			if got := Perform(fb, op, Unit); got != op.key {
				t.Fatalf("%s found %v", op.key, got)
			}
		}
	}
}

// The arm of a handler runs where the handler is: an operation there finds
// the provider around the handler, not the one the body found just before.
func TestPerformInAnArmFindsTheProviderOfTheHandler(t *testing.T) {
	svc := "test.PerformArmService"
	op := NewOp(svc, "name")
	for _, class := range []string{"tail", "abort", "control"} {
		t.Run(class, func(t *testing.T) {
			eff := "test.PerformArm." + class
			ask := NewOp(eff, "ask")
			in := Install{Key: eff}
			arm := func(fb *Fiber) Value { return Perform(fb, op, Unit).(string) }
			switch class {
			case "tail", "abort":
				in.Arms = Rec("ask", Fn(func(*Fiber, Value) Value { return Eff(arm) }))
				if class == "abort" {
					in.Aborts = []string{"ask"}
				}
			case "control":
				in.Arms = Rec("ask", Fn(func(*Fiber, Value) Value {
					return Fn(func(_ *Fiber, resume Value) Value {
						return Eff(func(fb *Fiber) Value { return Run(fb, Apply(fb, resume, arm(fb))) })
					})
				}))
				in.Controls = []Control{{Name: "ask", Arity: 1}}
			}
			fb := rootFiber()
			fb.env = fb.env.with(svc, performed("outer"))
			got := Handle(fb, []Install{in}, Eff(func(fb *Fiber) Value {
				inner := *fb
				inner.env = fb.env.with(svc, performed("inner"))
				before := Perform(&inner, op, Unit).(string)
				asked := Perform(&inner, ask, Unit).(string)
				return before + " " + asked + " " + Perform(&inner, op, Unit).(string)
			}), nil, nil)
			want := "inner outer inner"
			if class == "abort" {
				want = "outer"
			}
			if got != want {
				t.Fatalf("got %q, want %q", got, want)
			}
		})
	}
}

// A fiber's cache is its own: two fibers that run the same operation, each
// in its own environment, find each its own provider, in turn and at once.
func TestPerformInFibersOfTheirOwn(t *testing.T) {
	key := "test.PerformFibers"
	op := NewOp(key, "name")
	fibers := make([]*Fiber, 2)
	for i, who := range []string{"left", "right"} {
		fibers[i] = &Fiber{state: newState(), env: (*env)(nil).with(key, performed(who))}
	}
	done := make(chan bool)
	for i, fb := range fibers {
		go func() {
			ok := true
			for range 1000 {
				ok = ok && Perform(fb, op, Unit) == []string{"left", "right"}[i]
			}
			done <- ok
		}()
	}
	if !<-done || !<-done {
		t.Fatal("a fiber found the other's provider")
	}
}

// Perform makes nothing of its own: what an implementation makes is all a
// call allocates.
func TestPerformAllocatesNothingOfItsOwn(t *testing.T) {
	key := "test.PerformAllocs"
	op := NewOp(key, "name")
	task := Eff(func(*Fiber) Value { return Unit })
	fb := rootFiber()
	fb.env = fb.env.with(key, Rec("name", Fn(func(*Fiber, Value) Value { return task })))
	if n := testing.AllocsPerRun(100, func() { Perform(fb, op, Unit) }); n != 0 {
		t.Fatalf("Perform allocates %v times", n)
	}
}
