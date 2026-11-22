package flickrt

import (
	"reflect"
	"strings"
	"sync"
	"testing"
)

func TestArenaClosesBeforeContinuationAndPreservesReadAlias(t *testing.T) {
	var kept *arena
	alias := &struct{ text string }{"ordinary"}
	Run(rootFiber(), Eff(func(fb *Fiber) Value {
		read := Scope(fb, func(fb *Fiber, _, _ Value) Value {
			kept = ArenaMake(fb, Unit).(*arena)
			view := ArenaAlloc(kept, alias)
			if kept.liveSlots() != 1 {
				t.Fatal("arena owns its slot")
			}
			return ArenaGet(view)
		})
		if kept.liveSlots() != 0 || read != alias {
			t.Fatal("scope release precedes continuation; read alias survives")
		}
		return Unit
	}))
}

func TestArenaOutlivesEveryFinalizerAndClosesLateAllocations(t *testing.T) {
	var kept, late *arena
	var view Value
	var order []Value
	Run(rootFiber(), Eff(func(fb *Fiber) Value {
		return Scope(fb, func(fb *Fiber, owner, _ Value) Value {
			o := owner.(*Owner)
			o.add(fb, func(*Fiber, *Cause) Value { order = append(order, ArenaGet(view)); return Unit })
			kept = ArenaMake(fb, Unit).(*arena)
			view = ArenaAlloc(kept, "live")
			o.add(fb, func(fb *Fiber, _ *Cause) Value {
				order = append(order, ArenaGet(view))
				late = ArenaMake(fb, Unit).(*arena)
				order = append(order, ArenaGet(ArenaAlloc(late, "late")))
				o.add(fb, func(*Fiber, *Cause) Value { order = append(order, ArenaGet(view)); return Unit })
				return Unit
			})
			return Unit
		})
	}))
	if !reflect.DeepEqual(order, []Value{"live", "late", "live", "live"}) {
		t.Fatalf("all finalizers see live storage: %v", order)
	}
	if kept.liveSlots() != 0 || late.liveSlots() != 0 {
		t.Fatal("all arena slots are released")
	}
	if _, cause := catch(func() Value { return ArenaGet(view) }); cause == nil {
		t.Fatal("stale read must fail")
	}
}

func TestArenaUsesSelectedOuterOwner(t *testing.T) {
	var outerArena, innerArena *arena
	Run(rootFiber(), Eff(func(fb *Fiber) Value {
		return Scope(fb, func(fb *Fiber, owner, _ Value) Value {
			Scope(fb, func(fb *Fiber, _, _ Value) Value {
				outerArena = ArenaMake(Within(fb, owner), Unit).(*arena)
				innerArena = ArenaMake(fb, Unit).(*arena)
				ArenaAlloc(outerArena, "outer")
				ArenaAlloc(innerArena, "inner")
				return Unit
			})
			if outerArena.liveSlots() != 1 || innerArena.liveSlots() != 0 {
				t.Fatal("inner scope releases only its own storage")
			}
			return Unit
		})
	}))
	if outerArena.liveSlots() != 0 {
		t.Fatal("outer scope releases its arena")
	}
}

func TestRetiredOwnerSelectionIsInertButArenaAdmissionStaysClosed(t *testing.T) {
	var kept, outerArena *arena
	Run(rootFiber(), Eff(func(fb *Fiber) Value {
		return Scope(fb, func(fb *Fiber, _, _ Value) Value {
			outerArena = ArenaMake(fb, Unit).(*arena)
			outerView := ArenaAlloc(outerArena, "still live")
			var retired *Owner
			Scope(fb, func(fb *Fiber, owner, _ Value) Value {
				retired = owner.(*Owner)
				kept = ArenaMake(fb, Unit).(*arena)
				ArenaAlloc(kept, "retired")
				return Unit
			})
			if kept.liveSlots() != 0 {
				t.Fatal("inner storage must already be retired")
			}
			selected := Within(fb, retired)
			if value := Run(selected, Eff(func(*Fiber) Value { return 42 })); value != 42 {
				t.Fatalf("inert retired selection: %v", value)
			}
			if value := Run(selected, Eff(func(*Fiber) Value { return ArenaGet(outerView) })); value != "still live" {
				t.Fatalf("an unrelated live arena remains usable: %v", value)
			}
			_, cause := catch(func() Value { return ArenaMake(selected, Unit) })
			if cause == nil || cause.kind != causeDie || !strings.Contains(cause.defect, "arena owner is closed") {
				t.Fatalf("actual acquisition must report the closed-owner defect: %v", cause)
			}
			if kept.liveSlots() != 0 {
				t.Fatal("selection cannot reopen retired storage")
			}
			after := ArenaMake(fb, Unit)
			if value := ArenaGet(ArenaAlloc(after, "caller restored")); value != "caller restored" {
				t.Fatalf("caller default owner changed: %v", value)
			}
			return Unit
		})
	}))
	if outerArena.liveSlots() != 0 {
		t.Fatal("outer storage must retire at its own boundary")
	}
}

func TestArenaClosesOnEveryOutcomeAndRefusesEndedOwner(t *testing.T) {
	for _, ending := range []causeKind{causeFail, causeDie, causeInterrupt} {
		t.Run(string(rune('0'+ending)), func(t *testing.T) {
			var kept *arena
			var owner *Owner
			_, cause := catch(func() Value {
				return Run(rootFiber(), Eff(func(fb *Fiber) Value {
					return Scope(fb, func(fb *Fiber, current, _ Value) Value {
						owner = current.(*Owner)
						kept = ArenaMake(fb, Unit).(*arena)
						ArenaAlloc(kept, "contents")
						panic(&Failure{&Cause{kind: ending, err: "typed", defect: "defect"}})
					})
				}))
			})
			if cause == nil || kept.liveSlots() != 0 {
				t.Fatal("failed scope still closes storage")
			}
			fb := rootFiber()
			fb.owner = owner
			owner.close(fb, nil)
			if _, cause = catch(func() Value { return ArenaMake(fb, Unit) }); cause == nil {
				t.Fatal("closed owner must refuse acquisition")
			}
			if _, cause = catch(func() Value { return ArenaAlloc(kept, "late") }); cause == nil {
				t.Fatal("closed arena must refuse allocation")
			}
		})
	}
}

func TestArenaConcurrentAccessAndClose(t *testing.T) {
	a := &arena{}
	view := ArenaAlloc(a, "contents")
	var workers sync.WaitGroup
	for range 8 {
		workers.Add(1)
		go func() {
			defer workers.Done()
			for range 50 {
				catch(func() Value { return ArenaGet(view) })
				catch(func() Value { return ArenaAlloc(a, "more") })
			}
		}()
	}
	a.close()
	workers.Wait()
	if a.liveSlots() != 0 {
		t.Fatal("closed storage stays empty")
	}
}

func TestArenaSignalsUnwindFinalizersAndRetireStorage(t *testing.T) {
	// 1. abort unwinds inner scope, runs LIFO finalizers while arena is live, then retires arena
	var abortArena *arena
	var abortOrder []Value
	res := Run(rootFiber(), Eff(func(fb *Fiber) Value {
		escapeTag := "Test.Escape"
		arms := newRecord(1)
		arms.put("escape", Fn(func(_ *Fiber, msg Value) Value {
			return Eff(func(*Fiber) Value {
				abortOrder = append(abortOrder, "arm:"+msg.(string))
				return "escaped:" + msg.(string)
			})
		}))
		return Handle(fb, []Install{{Key: escapeTag, Arms: arms, Aborts: []string{"escape"}}}, Eff(func(fb *Fiber) Value {
			return Scope(fb, func(fb *Fiber, owner, _ Value) Value {
				o := owner.(*Owner)
				abortArena = ArenaMake(fb, Unit).(*arena)
				v := ArenaAlloc(abortArena, "abort-live")
				o.add(fb, func(*Fiber, *Cause) Value { abortOrder = append(abortOrder, "fin1:"+ArenaGet(v).(string)); return Unit })
				o.add(fb, func(*Fiber, *Cause) Value { abortOrder = append(abortOrder, "fin2:"+ArenaGet(v).(string)); return Unit })
				return Run(fb, Apply(fb, Operation(escapeTag, "escape", 1), "now"))
			})
		}), nil, nil)
	}))
	if res != "escaped:now" || abortArena.liveSlots() != 0 {
		t.Fatalf("unexpected abort result=%v liveSlots=%d", res, abortArena.liveSlots())
	}
	if !reflect.DeepEqual(abortOrder, []Value{"fin2:abort-live", "fin1:abort-live", "arm:now"}) {
		t.Fatalf("unexpected abort order: %v", abortOrder)
	}

	// 2. control arm returning without resuming discontinues inner arena scope in LIFO order
	var ctrlArena *arena
	var ctrlOrder []Value
	ctrlRes := Run(rootFiber(), Eff(func(fb *Fiber) Value {
		yieldTag := "Test.Yield"
		arms := newRecord(1)
		arms.put("yield_", Fn(func(_ *Fiber, x Value) Value {
			return Fn(func(_ *Fiber, _ Value) Value {
				return Eff(func(*Fiber) Value {
					ctrlOrder = append(ctrlOrder, "ctrl-arm")
					return x
				})
			})
		}))
		return Handle(fb, []Install{{Key: yieldTag, Arms: arms, Controls: []Control{{Name: "yield_", Arity: 1}}}}, Eff(func(fb *Fiber) Value {
			return Scope(fb, func(fb *Fiber, owner, _ Value) Value {
				o := owner.(*Owner)
				ctrlArena = ArenaMake(fb, Unit).(*arena)
				v := ArenaAlloc(ctrlArena, "ctrl-live")
				o.add(fb, func(*Fiber, *Cause) Value { ctrlOrder = append(ctrlOrder, "fin1:"+ArenaGet(v).(string)); return Unit })
				o.add(fb, func(*Fiber, *Cause) Value { ctrlOrder = append(ctrlOrder, "fin2:"+ArenaGet(v).(string)); return Unit })
				return Run(fb, Apply(fb, Operation(yieldTag, "yield_", 1), "dropped"))
			})
		}), nil, nil)
	}))
	if ctrlRes != "dropped" || ctrlArena.liveSlots() != 0 {
		t.Fatalf("unexpected control result=%v liveSlots=%d", ctrlRes, ctrlArena.liveSlots())
	}
	if !reflect.DeepEqual(ctrlOrder, []Value{"ctrl-arm", "fin2:ctrl-live", "fin1:ctrl-live"}) {
		t.Fatalf("unexpected control order: %v", ctrlOrder)
	}

	// 3. failing tail arm unwinds inner arena scope and runs finalizers before outer recovery
	var tailArena *arena
	var tailOrder []Value
	tailOut := Run(rootFiber(), Eff(func(fb *Fiber) Value {
		return Try(fb, func(fb *Fiber) Value {
			askTag := "Test.Ask"
			arms := newRecord(1)
			arms.put("ask", Fn(func(_ *Fiber, _ Value) Value {
				return Eff(func(*Fiber) Value { panic(&Failure{&Cause{kind: causeFail, err: "tail-boom"}}) })
			}))
			return Handle(fb, []Install{{Key: askTag, Arms: arms}}, Eff(func(fb *Fiber) Value {
				return Scope(fb, func(fb *Fiber, owner, _ Value) Value {
					o := owner.(*Owner)
					tailArena = ArenaMake(fb, Unit).(*arena)
					v := ArenaAlloc(tailArena, "tail-live")
					o.add(fb, func(*Fiber, *Cause) Value { tailOrder = append(tailOrder, "fin1:"+ArenaGet(v).(string)); return Unit })
					o.add(fb, func(*Fiber, *Cause) Value { tailOrder = append(tailOrder, "fin2:"+ArenaGet(v).(string)); return Unit })
					return Run(fb, Apply(fb, Operation(askTag, "ask", 1), Unit))
				})
			}), nil, nil)
		}, func(_ *Fiber, err Value) Value {
			tailOrder = append(tailOrder, "caught:"+err.(string))
			return err
		})
	}))
	if tailOut != "tail-boom" || tailArena.liveSlots() != 0 {
		t.Fatalf("unexpected tail failure result=%v liveSlots=%d", tailOut, tailArena.liveSlots())
	}
	if !reflect.DeepEqual(tailOrder, []Value{"fin2:tail-live", "fin1:tail-live", "caught:tail-boom"}) {
		t.Fatalf("unexpected tail order: %v", tailOrder)
	}
}

func TestArenaTrackedAsyncWorkAndLateCallbackAfterClose(t *testing.T) {
	var kept *arena
	var view Value
	var seen []Value
	lateDone := make(chan bool, 1)
	Run(rootFiber(), Eff(func(fb *Fiber) Value {
		return Scope(fb, func(fb *Fiber, owner, _ Value) Value {
			o := owner.(*Owner)
			kept = ArenaMake(fb, Unit).(*arena)
			view = ArenaAlloc(kept, "tracked-live")
			o.add(fb, func(*Fiber, *Cause) Value {
				seen = append(seen, "c5:"+ArenaGet(view).(string))
				return Unit
			})
			done := make(chan struct{})
			o.track(done)
			go func() {
				defer close(done)
				seen = append(seen, "c4:"+ArenaGet(view).(string))
			}()
			return Unit
		})
	}))
	if kept.liveSlots() != 0 {
		t.Fatal("arena must be retired after tracked C4 and C5 finalizers finish")
	}
	if !reflect.DeepEqual(seen, []Value{"c4:tracked-live", "c5:tracked-live"}) {
		t.Fatalf("C4 must settle before C5 finalizers: %v", seen)
	}
	// Late untracked async callback after scope close cleanly raises closed-arena defect
	go func() {
		_, cause := catch(func() Value { return ArenaGet(view) })
		lateDone <- (cause != nil && cause.kind == causeDie && strings.Contains(cause.defect, "arena is closed"))
	}()
	if !<-lateDone {
		t.Fatal("late callback reading closed arena must fail cleanly with closed-arena defect")
	}
}

func TestArenaPersistentContextLayerPreservesSelectedOuterOwner(t *testing.T) {
	var outerArena *arena
	var got Value
	Run(rootFiber(), Eff(func(fb *Fiber) Value {
		return Scope(fb, func(fb *Fiber, outer, _ Value) Value {
			fb = WithOwnerEvidence(fb, outer)
			ctx := ContextMake("Test.Svc", "val")
			layer := LayerSucceedContext(ctx)
			v := Provide(fb, layer, Eff(func(inner *Fiber) Value {
				outerArena = ArenaMake(Within(inner, ReadOwnerEvidence(inner)), Unit).(*arena)
				return ArenaAlloc(outerArena, ContextGet("Test.Svc", ctx))
			}))
			if outerArena.liveSlots() != 1 {
				t.Fatal("outer arena must outlive persistent context layer provide")
			}
			got = ArenaGet(v)
			return Unit
		})
	}))
	if got != "val" || outerArena.liveSlots() != 0 {
		t.Fatalf("unexpected persistent context result=%v liveSlots=%d", got, outerArena.liveSlots())
	}
}
