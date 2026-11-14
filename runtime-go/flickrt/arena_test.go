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
