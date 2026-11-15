package flickrt

import "testing"

// A task may carry the lexical candidate of its surrounding scope. The
// layer's acquisition scope must still release its resources before the
// provider returns or reports failure, while that surrounding scope lives.
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
					// The generated raw scoped call reinstalls captured evidence.
					captured := WithOwnerEvidence(at, outer)
					AddFinalizer(Within(captured, ReadOwnerEvidence(captured)), Eff(func(*Fiber) Value {
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

// Entering a trusted layer build scope must work even when its caller is
// already a detached or pinned acquisition. Ordinary lexical Within stays
// able to select an outer region when no acquisition is in progress.
func TestLayerBuildEntersItsScopeFromAnotherAcquisition(t *testing.T) {
	for _, pinned := range []bool{false, true} {
		name := "detached"
		if pinned {
			name = "pinned"
		}
		t.Run(name, func(t *testing.T) {
			outer := newOwner()
			outer.detached = true
			fb := WithOwnerEvidence(rootFiber(), outer)
			fb.owner, fb.pinOwner = outer, pinned
			released := 0
			layer := fromBuild(func(at *Fiber, _ *MemoMap, scope *Owner) Services {
				if !scope.detached {
					t.Error("the layer build scope is not detached")
				}
				return scoped(at, scope, Eff(func(at *Fiber) Value {
					if at.owner != scope || ReadOwnerEvidence(at) != scope {
						t.Error("the caller's acquisition prevented entering the layer build scope")
					}
					captured := WithOwnerEvidence(at, outer)
					AddFinalizer(Within(captured, outer), Eff(func(*Fiber) Value {
						released++
						return Unit
					}))
					return Services{}
				})).(Services)
			})
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
		// Channels and Streams use this generic scope helper too; only
		// layer acquisition scopes may block an explicit lexical owner.
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
