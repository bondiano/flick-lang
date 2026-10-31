package flickrt

import (
	"sync/atomic"
	"testing"
	"time"
)

func TestLayerFromBuildCause(t *testing.T) {
	factories := []struct {
		name string
		make func(func(*Fiber, *MemoMap, *Owner) Services) *Layer
	}{
		{"direct", fromBuild},
		{"memoized", memoized},
	}
	cases := []struct {
		name         string
		buildFails   bool
		cleanupFails bool
		wantReasons  int
	}{
		{"body_failure", false, false, 1},
		{"body_and_cleanup_failure", false, true, 2},
		{"build_and_cleanup_failure", true, true, 2},
	}
	for _, factory := range factories {
		for _, test := range cases {
			t.Run(factory.name+"/"+test.name, func(t *testing.T) {
				primary := &Cause{kind: causeFail, err: test.name}
				cleanup := &Cause{kind: causeDie, defect: "cleanup"}
				var seenExit *Cause
				released, bodyRan := 0, false
				layer := factory.make(func(fb *Fiber, _ *MemoMap, scope *Owner) Services {
					scope.add(fb, func(_ *Fiber, exit *Cause) Value {
						seenExit = exit
						released++
						if test.cleanupFails {
							panic(&Failure{cleanup})
						}
						return Unit
					})
					if test.buildFails {
						panic(&Failure{primary})
					}
					return Services{}
				})
				_, cause := catch(func() Value {
					return Provide(rootFiber(), layer, Eff(func(*Fiber) Value {
						bodyRan = true
						panic(&Failure{primary})
					}))
				})
				reasons := cause.reasons()
				if len(reasons) != test.wantReasons {
					t.Fatalf("got %d reasons, want %d: %s", len(reasons), test.wantReasons, pretty(cause))
				}
				if reasons[0] != primary {
					t.Error("the original failure is not the first reason")
				}
				if test.cleanupFails && reasons[1] != cleanup {
					t.Error("the cleanup failure is not the second reason")
				}
				if seenExit != primary {
					t.Errorf("finalizer saw %v instead of the original exit %v", seenExit, primary)
				}
				if released != 1 {
					t.Errorf("cleanup ran %d times, want once", released)
				}
				if bodyRan != !test.buildFails {
					t.Errorf("body ran=%v after build fails=%v", bodyRan, test.buildFails)
				}
			})
		}
	}
}

// Merged layers build concurrently and release in parallel: each waits for
// the other, which would never come one after the other.
func TestLayerMergeBuildsAndReleasesConcurrently(t *testing.T) {
	built := [2]chan struct{}{make(chan struct{}), make(chan struct{})}
	released := [2]chan struct{}{make(chan struct{}), make(chan struct{})}
	waiting := func(i int) *Layer {
		return fromBuild(func(fb *Fiber, _ *MemoMap, scope *Owner) Services {
			scope.add(fb, func(fb *Fiber, _ *Cause) Value {
				close(released[i])
				wait(fb, released[1-i])
				return Unit
			})
			close(built[i])
			wait(fb, built[1-i])
			return Services{"k" + string(rune('a'+i)): int32(i)}
		})
	}
	done := make(chan Value, 1)
	go func() {
		done <- Provide(rootFiber(), LayerMerge(waiting(0), waiting(1)), Eff(func(fb *Fiber) Value {
			a, _ := fb.env.get("ka")
			b, _ := fb.env.get("kb")
			return []Value{a, b}
		}))
	}()
	select {
	case got := <-done:
		if Format(got) != "[0,1]" {
			t.Fatalf("services %s", Format(got))
		}
	case <-time.After(5 * time.Second):
		t.Fatal("the merged layers built or released one after the other")
	}
}

// A key nobody holds stays built for idleTimeToLive, then is released;
// the map's scope releases what is still idle.
func TestLayerMapKeepsIdleKeysForTheirTimeToLive(t *testing.T) {
	var builds, releases atomic.Int32
	lookup := Fn(func(*Fiber, Value) Value {
		return fromBuild(func(fb *Fiber, _ *MemoMap, scope *Owner) Services {
			builds.Add(1)
			scope.add(fb, func(*Fiber, *Cause) Value { releases.Add(1); return Unit })
			return Services{"k": builds.Load()}
		})
	})
	ttl := Some(fromMillis(100))
	owned(rootFiber(), func(fb *Fiber, _ *Owner) Value {
		m := LayerMapMake(fb, lookup, Rec("idleTimeToLive", ttl, "preloadKeys", None))
		use := func() { Provide(fb, LayerMapGet("a", m), Eff(func(*Fiber) Value { return Unit })) }
		use()
		use()
		if builds.Load() != 1 || releases.Load() != 0 {
			t.Fatalf("within the time to live: %d builds, %d releases", builds.Load(), releases.Load())
		}
		time.Sleep(300 * time.Millisecond)
		if releases.Load() != 1 {
			t.Fatalf("past the time to live: %d releases", releases.Load())
		}
		use()
		if builds.Load() != 2 {
			t.Fatalf("after its release: %d builds", builds.Load())
		}
		return Unit
	})
	if releases.Load() != 2 {
		t.Fatalf("the map's scope left %d of 2 released", releases.Load())
	}
}
