package flickrt

import (
	"reflect"
	"testing"
)

func TestOwnerEvidenceHolderAcquisition(t *testing.T) {
	fb := rootFiber()
	var released []string
	acquire := func(name string) Value {
		return Eff(func(at *Fiber) Value {
			return AcquireRelease(Within(at, ReadOwnerEvidence(at)), Eff(func(*Fiber) Value { return name }),
				Fn(func(*Fiber, Value) Value {
					return Eff(func(*Fiber) Value { released = append(released, name); return Unit })
				}))
		})
	}
	ScopeRun(fb, Fn(func(at *Fiber, _ Value) Value {
		return Eff(func(at *Fiber) Value {
			ref := ScopedRefFromAcquire(at, acquire("first"))
			ScopedRefUse(at, ref, Fn(func(at *Fiber, _ Value) Value {
				return Eff(func(at *Fiber) Value {
					ScopedRefSet(at, acquire("second"), ref)
					if len(released) != 0 {
						t.Fatal("released while leased", released)
					}
					return Unit
				})
			}))
			if !reflect.DeepEqual(released, []string{"first"}) {
				t.Fatal("retired holder not released", released)
			}
			return Unit
		})
	}))
	if !reflect.DeepEqual(released, []string{"first", "second"}) {
		t.Fatal("holder close order", released)
	}
}

func TestOwnerEvidenceIdempotentBracketsPreserveReferences(t *testing.T) {
	ScopeRun(rootFiber(), Fn(func(at *Fiber, owner Value) Value {
		return Eff(func(at *Fiber) Value {
			EffectAnnotateLogsScoped(Within(WithOwnerEvidence(at, owner), owner), "request", "7")
			got := logAnnotationsOf(at)
			if i := got.index("request"); i < 0 || got.vals[i] != "7" {
				t.Fatal("scoped annotation lost", got)
			}
			return Unit
		})
	}))
}

func TestOwnerEvidenceRepeatedAbsencePreservesReferences(t *testing.T) {
	fb := WithOwnerEvidence(rootFiber(), newOwner())
	fb.owner = newOwner()
	absent := WithOwnerEvidence(fb, Unit)
	EffectAnnotateLogsScoped(WithOwnerEvidence(absent, Unit), "request", "7")
	got := logAnnotationsOf(absent)
	if i := got.index("request"); i < 0 || got.vals[i] != "7" {
		t.Fatal("repeated absence lost scoped annotation", got)
	}
	if cause := fb.owner.close(absent, nil); cause != nil {
		t.Fatal(cause)
	}
}

func assertEvidence(t *testing.T, fb *Fiber, d *Owner, c Value) {
	t.Helper()
	if ScopeCurrent(fb, Unit) != d || ReadOwnerEvidence(fb) != c {
		t.Fatalf("owner/evidence = %p/%v, want %p/%v", fb.owner, ReadOwnerEvidence(fb), d, c)
	}
}

func TestOwnerEvidenceInertDerivation(t *testing.T) {
	fb := rootFiber()
	d, c := newOwner(), newOwner()
	fb.owner = d
	assertEvidence(t, fb, d, Unit)
	inner := WithOwnerEvidence(fb, c)
	assertEvidence(t, inner, d, c)
	if inner.state != fb.state || inner.env != fb.env {
		t.Fatal("evidence changed fiber state or services")
	}
	assertEvidence(t, WithOwnerEvidence(inner, Unit), d, Unit)
	assertEvidence(t, inner, d, c)
	assertEvidence(t, provided(inner, Services{"test": Unit}), d, c)
	owned(inner, func(at *Fiber, owner *Owner) Value { assertEvidence(t, at, owner, c); return Unit })
	if cause := c.close(fb, nil); cause != nil {
		t.Fatal(cause)
	}
	assertEvidence(t, WithOwnerEvidence(fb, c), d, c)
	if c.open || len(c.hooks) != 0 || len(c.finalizers) != 0 || len(c.children) != 0 {
		t.Fatal("transport changed closed owner")
	}
	assertEvidence(t, fb, d, Unit)
}

func TestOwnerEvidenceFreshScopeAndSpawn(t *testing.T) {
	fb := WithOwnerEvidence(rootFiber(), newOwner())
	ScopeRun(fb, Fn(func(at *Fiber, owner Value) Value {
		return Eff(func(at *Fiber) Value { assertEvidence(t, at, owner.(*Owner), owner); return Unit })
	}))
	o := newOwner()
	seen := make(chan Value, 2)
	supervisor := Fn(func(at *Fiber, _ Value) Value {
		return Eff(func(at *Fiber) Value { seen <- ReadOwnerEvidence(at); return Unit })
	})
	FiberSpawn(fb, o, supervisor, Eff(func(at *Fiber) Value { seen <- ReadOwnerEvidence(at); panic(Defect{"expected test failure"}) }))
	if got := <-seen; got != o {
		t.Fatalf("child evidence = %v, want owner", got)
	}
	if cause := o.close(fb, nil); cause != nil {
		t.Fatal(cause)
	}
	if got := <-seen; got != o {
		t.Fatalf("supervisor evidence = %v, want owner", got)
	}
}

func TestOwnerEvidenceHandlerTransfers(t *testing.T) {
	for _, mode := range []string{"tail", "abort", "control", "discontinue", "forward"} {
		t.Run(mode, func(t *testing.T) {
			d, installC, performC := newOwner(), newOwner(), newOwner()
			fb := WithOwnerEvidence(rootFiber(), installC)
			fb.owner = d
			key := "test.evidence." + mode
			arm := Fn(func(_ *Fiber, _ Value) Value {
				if mode == "control" || mode == "discontinue" || mode == "forward" {
					return Fn(func(at *Fiber, resume Value) Value {
						assertEvidence(t, at, d, performC)
						return Eff(func(at *Fiber) Value {
							assertEvidence(t, at, d, performC)
							if mode == "discontinue" {
								return Unit
							}
							return Run(WithOwnerEvidence(at, installC), Apply(at, resume, Unit))
						})
					})
				}
				return Eff(func(at *Fiber) Value { assertEvidence(t, at, d, performC); return Unit })
			})
			in := Install{Key: key, Arms: Rec("op", arm)}
			if mode == "abort" {
				in.Aborts = []string{"op"}
			}
			if mode == "control" || mode == "discontinue" || mode == "forward" {
				in.Controls = []Control{{Name: "op", Arity: 1}}
			}
			continued, finalized := false, false
			body := Eff(func(at *Fiber) Value {
				at = WithOwnerEvidence(at, performC)
				return owned(at, func(at *Fiber, owner *Owner) Value {
					owner.add(at, func(at *Fiber, _ *Cause) Value { assertEvidence(t, at, owner, performC); finalized = true; return Unit })
					Run(at, Apply(at, Operation(key, "op", 1), Unit))
					assertEvidence(t, at, owner, performC)
					continued = true
					return Unit
				})
			})
			if mode == "forward" {
				original := body
				body = Eff(func(at *Fiber) Value {
					return Handle(at, []Install{{Key: key, Arms: Rec("unmentioned", handleConstant(1))}, {Key: "test.inner", Arms: Rec("unused", handleConstant(1)), Controls: []Control{{Name: "unused", Arity: 1}}}}, original, nil, nil)
				})
			}
			Handle(fb, []Install{in}, body, nil, nil)
			if !finalized || continued != (mode != "abort" && mode != "discontinue") {
				t.Fatalf("continued=%v finalized=%v", continued, finalized)
			}
			assertEvidence(t, fb, d, installC)
		})
	}
}

// Independently overlapping fibers keep their candidates through suspension,
// interruption, and finalizer dispatch on a differently evidenced closing fiber.
func TestOwnerEvidenceSuspensionCancellationOverlap(t *testing.T) {
	fb := rootFiber()
	d, c1, c2 := newOwner(), newOwner(), newOwner()
	started := make(chan struct{}, 2)
	finished := make(chan Value, 2)
	task := func(want Value) Eff {
		return Eff(func(at *Fiber) Value {
			assertEvidence(t, at, d, want)
			defer func() { finished <- ReadOwnerEvidence(at) }()
			d.add(at, func(at *Fiber, _ *Cause) Value { assertEvidence(t, at, d, want); return Unit })
			started <- struct{}{}
			wait(at, make(chan struct{}))
			return Unit
		})
	}
	a := fork(WithOwnerEvidence(fb, c1), d, task(c1), nil)
	b := fork(WithOwnerEvidence(fb, c2), d, task(c2), nil)
	<-started
	<-started
	stop(fb, a, b)
	seen := map[Value]bool{<-finished: true, <-finished: true}
	if !seen[c1] || !seen[c2] {
		t.Fatalf("overlapping candidates: %v", seen)
	}
	if cause := d.close(WithOwnerEvidence(fb, Unit), nil); cause != nil {
		t.Fatal(cause)
	}
	assertEvidence(t, fb, nil, Unit)
}

func TestOwnerEvidenceProviderPreservesCandidate(t *testing.T) {
	c := newOwner()
	fb := WithOwnerEvidence(rootFiber(), c)
	LayerRun(fb, LayerEmpty, Fn(func(at *Fiber, _ Value) Value {
		return Eff(func(at *Fiber) Value {
			if at.owner == c || at.owner == nil {
				t.Fatal("provider did not install independent default owner")
			}
			assertEvidence(t, at, at.owner, c)
			return Unit
		})
	}))
	assertEvidence(t, fb, nil, c)
}

func TestOwnerEvidenceHandlerExplicitAbsence(t *testing.T) {
	fb := WithOwnerEvidence(rootFiber(), newOwner())
	for _, abort := range []bool{false, true} {
		in := Install{Key: "test.absent", Arms: Rec("op", Fn(func(*Fiber, Value) Value {
			return Eff(func(at *Fiber) Value { assertEvidence(t, at, nil, Unit); return Unit })
		}))}
		if abort {
			in.Aborts = []string{"op"}
		}
		Handle(fb, []Install{in}, Eff(func(at *Fiber) Value {
			at = WithOwnerEvidence(at, Unit)
			return Run(at, Apply(at, Operation(in.Key, "op", 1), Unit))
		}), nil, nil)
	}
}

func TestOwnerEvidenceFreshLayerBuild(t *testing.T) {
	fb := WithOwnerEvidence(rootFiber(), newOwner())
	var buildOwner Value
	layer := LayerFromBuild(Fn(func(at *Fiber, _ Value) Value {
		buildOwner = ReadOwnerEvidence(at)
		return Eff(func(at *Fiber) Value {
			assertEvidence(t, at, at.owner, at.owner)
			if buildOwner != at.owner {
				t.Fatal("fresh build callback construction and execution disagree")
			}
			return Services{}
		})
	}))
	LayerRun(fb, layer, Fn(func(at *Fiber, _ Value) Value {
		return Eff(func(at *Fiber) Value {
			if ReadOwnerEvidence(at) != ReadOwnerEvidence(fb) {
				t.Fatal("build evidence leaked into provider body")
			}
			return Unit
		})
	}))
}

func TestOwnerEvidenceEffectLayerBuild(t *testing.T) {
	fb := WithOwnerEvidence(rootFiber(), newOwner())
	var built *Owner
	var released bool
	layer := LayerEffect("evidence.test", Eff(func(at *Fiber) Value {
		built = at.owner
		assertEvidence(t, at, built, built)
		built.add(at, func(*Fiber, *Cause) Value { released = true; return Unit })
		return Unit
	}))
	LayerRun(fb, layer, Fn(func(at *Fiber, _ Value) Value {
		return Eff(func(at *Fiber) Value {
			if released || at.owner == built {
				t.Fatal("layer owner must stay live independently of the consumer")
			}
			assertEvidence(t, at, at.owner, ReadOwnerEvidence(fb))
			return Unit
		})
	}))
	if !released {
		t.Fatal("layer owner was not released with its consumer")
	}
}
