package flickrt

import (
	"strings"
	"testing"
)

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

func TestHandleAbortFromChildFiberPanicsDefect(t *testing.T) {
	key := "test.CrossFiberAbort"
	armRan := false
	arm := Fn(func(*Fiber, Value) Value {
		return Eff(func(*Fiber) Value {
			armRan = true
			return "wrong"
		})
	})
	_, cause := catch(func() Value {
		return Handle(rootFiber(), []Install{{Key: key, Arms: Rec("stop", arm), Aborts: []string{"stop"}}}, Eff(func(fb *Fiber) Value {
			return ParallelPair(
				fb,
				Eff(func(child *Fiber) Value { return Run(child, Apply(child, Operation(key, "stop", 1), Unit)) }),
				Succeed(int32(1)),
			)
		}), nil, nil)
	})
	if armRan {
		t.Fatal("abort arm must not run for a child fiber")
	}
	if cause == nil || !cause.has(causeDie) || !strings.Contains(pretty(cause), "other than its handler") {
		t.Fatalf("want cross-fiber abort defect, got %v", cause)
	}
}

func TestHandleEnsuringDieAndAbortPropagatesCleanDefect(t *testing.T) {
	key := "test.EnsuringDieAbort"
	for _, tc := range []struct {
		name      string
		body      func(*Fiber) Value
		finalizer func(*Fiber) Value
	}{
		{
			name:      "body_die_cleanup_abort",
			body:      func(*Fiber) Value { return Die("boom") },
			finalizer: func(fb *Fiber) Value { return Run(fb, Apply(fb, Operation(key, "stop", 1), Unit)) },
		},
		{
			name:      "body_abort_cleanup_die",
			body:      func(fb *Fiber) Value { return Run(fb, Apply(fb, Operation(key, "stop", 1), Unit)) },
			finalizer: func(*Fiber) Value { return Die("boom") },
		},
	} {
		t.Run(tc.name, func(t *testing.T) {
			armRan := false
			arm := Fn(func(*Fiber, Value) Value {
				return Eff(func(*Fiber) Value {
					armRan = true
					return "swallowed"
				})
			})
			_, cause := catch(func() Value {
				return Handle(rootFiber(), []Install{{Key: key, Arms: Rec("stop", arm), Aborts: []string{"stop"}}}, Eff(func(fb *Fiber) Value {
					return EffectEnsuring(fb, Eff(tc.finalizer), Eff(tc.body))
				}), nil, nil)
			})
			if armRan {
				t.Fatal("abort arm must not swallow defect")
			}
			if cause == nil || len(cause.reasons()) != 1 || cause.reasons()[0].kind != causeDie || cause.reasons()[0].message() != "boom" {
				t.Fatalf("want single Die(boom), got %v (%d reasons)", cause, len(cause.reasons()))
			}
		})
	}
}

func TestHandleFirstSuccessOfAndEventuallyPropagateAbort(t *testing.T) {
	key := "test.SequentialAbort"
	arm := Fn(func(*Fiber, Value) Value { return Eff(func(*Fiber) Value { return "aborted" }) })

	secondRan := false
	got := Handle(rootFiber(), []Install{{Key: key, Arms: Rec("stop", arm), Aborts: []string{"stop"}}}, Eff(func(fb *Fiber) Value {
		return EffectFirstSuccessOf(fb, []Value{
			Eff(func(fb *Fiber) Value { return Run(fb, Apply(fb, Operation(key, "stop", 1), Unit)) }),
			Eff(func(*Fiber) Value { secondRan = true; return "second" }),
		})
	}), nil, nil)
	if got != "aborted" || secondRan {
		t.Fatalf("firstSuccessOf: got %v, secondRan=%v, want aborted/false", got, secondRan)
	}

	attempts := 0
	got = Handle(rootFiber(), []Install{{Key: key, Arms: Rec("stop", arm), Aborts: []string{"stop"}}}, Eff(func(fb *Fiber) Value {
		return EffectEventually(fb, Eff(func(fb *Fiber) Value {
			attempts++
			if attempts == 1 {
				return Fail("retry")
			}
			return Run(fb, Apply(fb, Operation(key, "stop", 1), Unit))
		}))
	}), nil, nil)
	if got != "aborted" || attempts != 2 {
		t.Fatalf("eventually: got %v, attempts=%d, want aborted/2", got, attempts)
	}
}

func TestHandleSignalNotWrappedByPartitionOrParallelResults(t *testing.T) {
	sig := &handlerSignal{token: &handlerToken{}, arm: Succeed("arm")}
	sigCause := &Cause{kind: causeFail, err: sig}
	if got := FailurePrimary(&Supervised{cause: sigCause}); got != None {
		t.Fatalf("FailurePrimary(signal) = %v, want None", got)
	}

	_, cause := catch(func() Value {
		return ParallelResults(rootFiber(), []Value{Eff(func(*Fiber) Value { return unwind(sig) })})
	})
	if cause == nil || !unwinds(cause) {
		t.Fatalf("ParallelResults must propagate handler signal, got %v", cause)
	}

	seq := NewData(Tag("Effect.Core.Concurrency.Sequential"))
	_, cause = catch(func() Value {
		return EffectPartition(rootFiber(), seq, Fn(func(*Fiber, Value) Value {
			return Eff(func(*Fiber) Value { return unwind(sig) })
		}), []Value{int32(1)})
	})
	if cause == nil || !unwinds(cause) {
		t.Fatalf("EffectPartition must propagate handler signal, got %v", cause)
	}
}

func TestCacheRunUnlocksMutexOnPanic(t *testing.T) {
	fb := rootFiber()
	c := &cache{
		task: Eff(func(*Fiber) Value {
			return unwind(&handlerSignal{token: &handlerToken{}, arm: Succeed("arm")})
		}),
		ttl: 1e9,
	}
	_, cause := catch(func() Value { return c.run(fb) })
	if cause == nil || !unwinds(cause) {
		t.Fatalf("want unwind from cached task, got %v", cause)
	}
	// If cache.run leaked c.mu, invalidate() would deadlock immediately.
	c.invalidate()
}

func TestHandleWithErrorArmsElectsDieAndFailAndRunsSkipHandler(t *testing.T) {
	key := "test.MixedHandlerErrors"
	armRan := false
	errRan := false
	arm := Fn(func(*Fiber, Value) Value {
		return Eff(func(*Fiber) Value {
			armRan = true
			return "wrong-op"
		})
	})
	errHandler := func(_ *Fiber, _ Value) Value {
		errRan = true
		return "wrong-err"
	}

	// Die + Fail(TypedErr) under a handler with both op and error arms elects Fail -> Die (["D", "D"]).
	mixed := Sequential(&Cause{kind: causeDie, defect: "boom"}, &Cause{kind: causeFail, err: "my-err"})
	_, cause := catch(func() Value {
		return Handle(rootFiber(), []Install{{Key: key, Arms: Rec("stop", arm), Aborts: []string{"stop"}}}, Eff(func(*Fiber) Value {
			panic(&Failure{mixed})
		}), nil, errHandler)
	})
	if armRan || errRan {
		t.Fatalf("neither op nor error arm may run on Die+Fail: armRan=%v errRan=%v", armRan, errRan)
	}
	rs := cause.reasons()
	if len(rs) != 2 || rs[0].kind != causeDie || rs[1].kind != causeDie {
		t.Fatalf("want [Die, Die], got %v", rs)
	}

	// Die + Fail(handlerSignal) under a handler with error arms strips the signal cleanly.
	_, cause = catch(func() Value {
		return Handle(rootFiber(), []Install{{Key: key, Arms: Rec("stop", arm), Aborts: []string{"stop"}}}, Eff(func(fb *Fiber) Value {
			return EffectEnsuring(fb, Eff(func(fb *Fiber) Value {
				return Run(fb, Apply(fb, Operation(key, "stop", 1), Unit))
			}), Eff(func(*Fiber) Value { return Die("boom") }))
		}), nil, errHandler)
	})
	if armRan || errRan {
		t.Fatalf("neither op nor error arm may run on Die+Signal: armRan=%v errRan=%v", armRan, errRan)
	}
	rs = cause.reasons()
	if len(rs) != 1 || rs[0].kind != causeDie || rs[0].message() != "boom" {
		t.Fatalf("want single Die(boom), got %v", rs)
	}

	// Interrupted fiber with Die + Fail runs skipHandler (drops Fail, appends Interrupt).
	fb := rootFiber()
	_, cause = catch(func() Value {
		return Handle(fb, []Install{{Key: key, Arms: Rec("stop", arm), Aborts: []string{"stop"}}}, Eff(func(fb *Fiber) Value {
			fb.state.interrupt(99)
			panic(&Failure{mixed})
		}), nil, errHandler)
	})
	rs = cause.reasons()
	if len(rs) != 2 || rs[0].kind != causeDie || rs[1].kind != causeInterrupt {
		t.Fatalf("want [Die, Interrupt] from skipHandler, got %v", rs)
	}
}

func TestLazyForceInterruptDieAndSignalDefect(t *testing.T) {
	// R9: Interrupt + Die must NOT be cached; a second force retries.
	runs := 0
	cell := LazyMake(Eff(func(*Fiber) Value {
		runs++
		if runs == 1 {
			panic(&Failure{Sequential(&Cause{kind: causeInterrupt}, &Cause{kind: causeDie, defect: "int-die"})})
		}
		return int32(42)
	}))
	_, c1 := catch(func() Value { return LazyForce(rootFiber(), cell) })
	if c1 == nil || !c1.has(causeInterrupt) || !c1.hasRealDie() {
		t.Fatalf("first force want Interrupt+Die, got %v", c1)
	}
	v2 := LazyForce(rootFiber(), cell)
	if runs != 2 || v2 != int32(42) {
		t.Fatalf("second force want runs=2 v=42, got runs=%d v=%v", runs, v2)
	}

	// Die + Fail(Signal) without Interrupt MUST strip the signal and cache the clean defect.
	key := "test.LazyDieSignal"
	arm := Fn(func(*Fiber, Value) Value { return Eff(func(*Fiber) Value { return "swallowed" }) })
	runs2 := 0
	cell2 := LazyMake(Eff(func(fb *Fiber) Value {
		runs2++
		return EffectEnsuring(fb, Eff(func(*Fiber) Value { return Die("fin-boom") }), Eff(func(fb *Fiber) Value {
			return Run(fb, Apply(fb, Operation(key, "stop", 1), Unit))
		}))
	}))
	for attempt := 1; attempt <= 2; attempt++ {
		_, c := catch(func() Value {
			return Handle(rootFiber(), []Install{{Key: key, Arms: Rec("stop", arm), Aborts: []string{"stop"}}}, Eff(func(fb *Fiber) Value {
				return LazyForce(fb, cell2)
			}), nil, nil)
		})
		if c == nil || len(c.reasons()) != 1 || c.reasons()[0].kind != causeDie || c.reasons()[0].message() != "fin-boom" {
			t.Fatalf("attempt %d: want single Die(fin-boom), got %v", attempt, c)
		}
	}
	if runs2 != 1 {
		t.Fatalf("want defect cached (runs=1), got runs=%d", runs2)
	}
}

func TestEffectErrorsOrDieIgnoreOnExitAndTapWithAbort(t *testing.T) {
	key := "test.EffectErrorsAbort"
	arm := Fn(func(_ *Fiber, v Value) Value { return Eff(func(*Fiber) Value { return v }) })
	stop := func(fb *Fiber, msg string) Value {
		return Run(fb, Apply(fb, Operation(key, "stop", 1), msg))
	}

	// EffectOrDie lets pure abort unwind to handler.
	got := Handle(rootFiber(), []Install{{Key: key, Arms: Rec("stop", arm), Aborts: []string{"stop"}}}, Eff(func(fb *Fiber) Value {
		return EffectOrDie(fb, Eff(func(fb *Fiber) Value { return stop(fb, "halt") }))
	}), nil, nil)
	if got != "halt" {
		t.Fatalf("EffectOrDie on abort: got %v, want halt", got)
	}

	// EffectIgnore and EffectIgnoreCause let pure abort unwind to handler.
	got = Handle(rootFiber(), []Install{{Key: key, Arms: Rec("stop", arm), Aborts: []string{"stop"}}}, Eff(func(fb *Fiber) Value {
		return EffectIgnore(fb, None, Eff(func(fb *Fiber) Value { return stop(fb, "ignored-no") }))
	}), nil, nil)
	if got != "ignored-no" {
		t.Fatalf("EffectIgnore on abort: got %v, want ignored-no", got)
	}
	got = Handle(rootFiber(), []Install{{Key: key, Arms: Rec("stop", arm), Aborts: []string{"stop"}}}, Eff(func(fb *Fiber) Value {
		return EffectIgnoreCause(fb, None, Eff(func(fb *Fiber) Value { return stop(fb, "ignored-cause-no") }))
	}), nil, nil)
	if got != "ignored-cause-no" {
		t.Fatalf("EffectIgnoreCause on abort: got %v, want ignored-cause-no", got)
	}

	// EffectOnExit receives clean Exit (signals stripped when a real defect is present).
	var seenExit *Exit
	_, cause := catch(func() Value {
		return Handle(rootFiber(), []Install{{Key: key, Arms: Rec("stop", arm), Aborts: []string{"stop"}}}, Eff(func(fb *Fiber) Value {
			return EffectOnExit(fb, Fn(func(_ *Fiber, e Value) Value {
				return Eff(func(*Fiber) Value {
					seenExit = e.(*Exit)
					return Unit
				})
			}), Eff(func(fb *Fiber) Value {
				return EffectEnsuring(fb, Eff(func(*Fiber) Value { return Die("fin-boom") }), Eff(func(fb *Fiber) Value {
					return stop(fb, "body-abort")
				}))
			}))
		}), nil, nil)
	})
	if seenExit == nil || seenExit.cause == nil || len(seenExit.cause.reasons()) != 1 || seenExit.cause.reasons()[0].kind != causeDie {
		t.Fatalf("EffectOnExit callback saw %v, want single Die", seenExit)
	}
	if cause == nil || len(cause.reasons()) != 1 || cause.reasons()[0].kind != causeDie {
		t.Fatalf("EffectOnExit outer cause = %v, want single Die", cause)
	}
}
