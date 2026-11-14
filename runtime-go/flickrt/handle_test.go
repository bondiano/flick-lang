package flickrt

import "testing"

func handleConstant(n int32) Value {
	return Fn(func(*Fiber, Value) Value { return Eff(func(*Fiber) Value { return n }) })
}

// Unmentioned operations pass through a partial handler to the next one
// with an arm (reference/effects, Effects and handlers; formal/core §3.7).
func TestHandleForwardsUnmentionedOperation(t *testing.T) {
	key := "test.Partial"
	outer := []Install{{Key: key, Arms: Rec("left", handleConstant(10), "right", handleConstant(20))}}
	inner := []Install{{Key: key, Arms: Rec("left", handleConstant(1))}}
	got := Handle(rootFiber(), outer, Eff(func(fb *Fiber) Value {
		return Handle(fb, inner, Eff(func(fb *Fiber) Value {
			left := Run(fb, Apply(fb, Operation(key, "left", 1), Unit)).(int32)
			right := Run(fb, Apply(fb, Operation(key, "right", 1), Unit)).(int32)
			return left + right
		}), nil, nil)
	}), nil, nil)
	if got != int32(21) {
		t.Fatalf("got %v, want 21", got)
	}
}

func TestHandleForwardedArmUsesOuterEvidenceAndOwner(t *testing.T) {
	key, setting := "test.PartialEvidence", "test.setting"
	fb := rootFiber()
	fb.env = fb.env.with(setting, int32(7))
	owner := fb.owner
	arm := Fn(func(*Fiber, Value) Value {
		return Eff(func(at *Fiber) Value {
			if at.owner != owner {
				t.Fatal("forwarded arm used the body's owner")
			}
			v, _ := at.env.get(setting)
			return v
		})
	})
	got := Handle(fb, []Install{{Key: key, Arms: Rec("right", arm)}}, Eff(func(fb *Fiber) Value {
		inner := *fb
		inner.env = fb.env.with(setting, int32(99))
		inner.owner = newOwner()
		return Handle(&inner, []Install{{Key: key, Arms: Rec("left", handleConstant(1))}}, Eff(func(fb *Fiber) Value {
			return Run(fb, Apply(fb, Operation(key, "right", 1), Unit))
		}), nil, nil)
	}), nil, nil)
	if got != int32(7) {
		t.Fatalf("forwarded arm's evidence: %v, want 7", got)
	}
}

func TestHandleOperationLookupWaitsForTaskExecution(t *testing.T) {
	key := "test.PartialDeferred"
	fb := rootFiber()
	// Both pure argument stages and dictionary instantiation happen before
	// any handler is installed. Neither is the operation's execution place.
	partial := Apply(fb, Operation(key, "right", 2), int32(3))
	task := Apply(fb, partial, int32(4))
	constrained := Apply(fb, OperationGiven(key, "given", 1, int32(5)), int32(6))
	given := Poly(func(_ *Fiber, dicts ...Value) Value {
		return Fn(func(_ *Fiber, a Value) Value {
			return Eff(func(*Fiber) Value { return dicts[0].(int32) + a.(int32) })
		})
	})
	right := Fn2(func(_ *Fiber, a, b Value) Value {
		return Eff(func(*Fiber) Value { return a.(int32) + b.(int32) })
	})
	got := Handle(fb, []Install{{Key: key, Arms: Rec("right", right, "given", given)}}, Eff(func(fb *Fiber) Value {
		return Handle(fb, []Install{{Key: key, Arms: Rec("left", handleConstant(1))}}, Eff(func(fb *Fiber) Value {
			return Run(fb, task).(int32) + Run(fb, constrained).(int32)
		}), nil, nil)
	}), nil, nil)
	if got != int32(18) {
		t.Fatalf("deferred operations: %v, want 18", got)
	}
}

func TestHandleForwardsAbortAndControl(t *testing.T) {
	for _, class := range []string{"abort", "control"} {
		t.Run(class, func(t *testing.T) {
			key := "test.PartialTransfer." + class
			outer := Install{Key: key}
			if class == "abort" {
				outer.Arms = Rec("right", handleConstant(20))
				outer.Aborts = []string{"right"}
			} else {
				outer.Arms = Rec("right", Fn(func(*Fiber, Value) Value {
					return Fn(func(_ *Fiber, resume Value) Value {
						return Eff(func(fb *Fiber) Value { return Run(fb, Apply(fb, resume, int32(20))) })
					})
				}))
				outer.Controls = []Control{{Name: "right", Arity: 1}}
			}
			continued := false
			got := Handle(rootFiber(), []Install{outer}, Eff(func(fb *Fiber) Value {
				return Handle(fb, []Install{{Key: key, Arms: Rec("left", handleConstant(1))}}, Eff(func(fb *Fiber) Value {
					v := Run(fb, Apply(fb, Operation(key, "right", 1), Unit)).(int32)
					continued = true
					return v + 1
				}), nil, nil)
			}), nil, nil)
			want := int32(20)
			if class == "control" {
				want++
			}
			if got != want || continued != (class == "control") {
				t.Fatalf("got %v, continued %v, want %v", got, continued, want)
			}
		})
	}
}

func TestHandleMissingOperationDoesNotCrossCompleteProvider(t *testing.T) {
	key := "test.PartialBarrier"
	for _, complete := range []bool{false, true} {
		t.Run(map[bool]string{false: "no_provider", true: "complete_provider"}[complete], func(t *testing.T) {
			fb := rootFiber()
			if complete {
				// A raw service is complete, unlike a handler's partial arms.
				// Even a malformed raw implementation must not silently use an
				// outer service for the operation it is missing.
				fb.env = fb.env.withHandler(key, record(Rec("right", handleConstant(20))))
				fb = provided(fb, Services{key: Rec("left", handleConstant(10))})
			}
			_, cause := catch(func() Value {
				return Handle(fb, []Install{{Key: key, Arms: Rec("left", handleConstant(1))}}, Eff(func(fb *Fiber) Value {
					return Run(fb, Apply(fb, Operation(key, "right", 1), Unit))
				}), nil, nil)
			})
			if cause == nil || !cause.has(causeDie) {
				t.Fatalf("missing operation must be a defect, got %v", cause)
			}
		})
	}
}
