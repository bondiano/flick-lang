package flickrt

import "testing"

// A rank-polymorphic task receives fresh lexical acquisition evidence. Its
// scope releases resources before the provider returns or reports failure,
// while the surrounding scope remains live.
func TestLayerAcquisitionKeepsItsReleaseScope(t *testing.T) {
	for _, fails := range []bool{false, true} {
		name := "success"
		if fails {
			name = "failure"
		}
		t.Run(name, func(t *testing.T) {
			released := 0
			primary := &Cause{kind: causeFail, err: "layer build"}
			Scope(rootFiber(), func(fb *Fiber, outer, _ Value) Value {
				layer := LayerEffect("test.layer", Eff(func(at *Fiber) Value {
					// The generated raw call activates the task's fresh evidence.
					owner := ReadOwnerEvidence(at)
					if owner == outer || owner != at.owner {
						t.Fatal("the acquisition did not receive its fresh lexical owner")
					}
					AddFinalizer(Within(at, owner), Eff(func(*Fiber) Value {
						released++
						return Unit
					}))
					if fails {
						panic(&Failure{primary})
					}
					return Unit
				}))
				bodyRan := false
				_, cause := catch(func() Value {
					return Provide(fb, layer, Eff(func(*Fiber) Value {
						bodyRan = true
						if released != 0 {
							t.Fatal("the layer was released while its consumer was running")
						}
						return Unit
					}))
				})
				if fails && cause != primary {
					t.Fatalf("build failed with %v instead of the original cause", cause)
				}
				if !fails && cause != nil {
					t.Fatalf("successful build failed: %v", cause)
				}
				if bodyRan == fails {
					t.Fatalf("consumer ran=%v after build failed=%v", bodyRan, fails)
				}
				if !outer.(*Owner).open {
					t.Fatal("the surrounding scope closed before the release assertion")
				}
				if released != 1 {
					t.Errorf("layer release ran %d times before its still-live surrounding scope closed, want once", released)
				}
				if len(outer.(*Owner).finalizers) != 0 {
					t.Error("the surrounding owner received the layer's finalizer")
				}
				return Unit
			})
			if released != 1 {
				t.Errorf("layer release ran %d times after all scopes closed, want once", released)
			}
		})
	}
}

// Entering a layer build supplies its own work owner and fresh evidence;
// a nested provider restores its caller's evidence for the consumer.
func TestLayerBuildEntersItsScopeFromAnotherAcquisition(t *testing.T) {
	for _, shared := range []bool{false, true} {
		name := "private"
		if shared {
			name = "memoized"
		}
		t.Run(name, func(t *testing.T) {
			outer := newOwner()
			fb := WithOwnerEvidence(rootFiber(), outer)
			fb.owner = outer
			released := 0
			build := func(at *Fiber, _ *MemoMap, scope *Owner) Services {
				if scope == outer {
					t.Error("the layer reused its caller's acquisition scope")
				}
				return scoped(at, scope, Eff(func(at *Fiber) Value {
					if at.owner != scope || ReadOwnerEvidence(at) != scope {
						t.Error("the caller's acquisition prevented entering the layer build scope")
					}
					AddFinalizer(Within(at, ReadOwnerEvidence(at)), Eff(func(*Fiber) Value {
						released++
						return Unit
					}))
					return Services{}
				})).(Services)
			}
			layer := fromBuild(build)
			if shared {
				layer = memoized(build)
			}
			Provide(fb, layer, Eff(func(at *Fiber) Value {
				if at.owner == outer || released != 0 {
					t.Error("the consumer lost its provider owner or the layer released early")
				}
				if ReadOwnerEvidence(at) != outer {
					t.Error("the build's evidence leaked into the consumer")
				}
				return Unit
			}))
			if released != 1 || len(outer.finalizers) != 0 {
				t.Errorf("layer release=%d, finalizers leaked to caller=%d", released, len(outer.finalizers))
			}
		})
	}
	t.Run("ordinary_lexical_owner", func(t *testing.T) {
		outer := newOwner()
		fb := rootFiber()
		fb.owner = outer
		// Channels and Streams use this generic scope helper too. Explicit
		// lexical activation selects its owner independently in every scope.
		current := forkScope(fb, outer)
		inner := *fb
		inner.owner = current
		if Within(&inner, outer).owner != outer || inner.owner != current {
			t.Error("ordinary lexical activation no longer selects its requested owner independently")
		}
		if cause := outer.closeFinalizers(fb, nil); cause != nil {
			t.Fatalf("closing the generic scopes failed: %v", cause)
		}
	})
}

func TestLayerAcquisitionRespectsExplicitOuterOwner(t *testing.T) {
	released := 0
	Scope(rootFiber(), func(fb *Fiber, outer, _ Value) Value {
		layer := LayerEffect("test.explicit.layer", Eff(func(at *Fiber) Value {
			if at.owner == outer {
				t.Fatal("the acquisition did not start with its private work owner")
			}
			AddFinalizer(Within(at, outer), Eff(func(*Fiber) Value { released++; return Unit }))
			return Unit
		}))
		Provide(fb, layer, Eff(func(*Fiber) Value { return Unit }))
		if !outer.(*Owner).open || released != 0 || len(outer.(*Owner).finalizers) != 1 {
			t.Fatal("an explicitly outer-owned finalizer did not stay with its live owner")
		}
		return Unit
	})
	if released != 1 {
		t.Fatalf("the explicitly owned finalizer ran %d times, want once", released)
	}
}
