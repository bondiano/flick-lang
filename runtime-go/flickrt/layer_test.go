package flickrt

import "testing"

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
