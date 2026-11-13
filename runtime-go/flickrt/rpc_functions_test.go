package flickrt

import (
	"testing"
	"time"
)

func TestServerMountSurvivesRepeatedCallsAndClosesWithOwner(t *testing.T) {
	const name = "test/mounted"
	calls := 0
	owned(rootFiber(), func(fb *Fiber, _ *Owner) Value {
		proc := ServerProcedure(name, SchemaString, SchemaString, ServerNoErrors)
		mounted := ServerHandler(fb, []Value{ServerFunction(proc, Fn(func(_ *Fiber, input Value) Value {
			calls++
			return input.(string) + "!"
		}))})
		localMu.Lock()
		first := localFns[name]
		localMu.Unlock()
		if _, ok := mounted.(Fn); !ok || first == nil {
			t.Fatal("handler did not mount")
		}
		for _, input := range []string{"first", "second"} {
			if got := ServerCall(fb, proc, input); got != input+"!" {
				t.Fatalf("call: %v", got)
			}
			localMu.Lock()
			same := localFns[name] == first
			localMu.Unlock()
			if !same {
				t.Fatal("a call remounted the server group")
			}
		}
		return Unit
	})
	if calls != 2 {
		t.Fatalf("handler ran %d times, want 2", calls)
	}
	localMu.Lock()
	defer localMu.Unlock()
	if localFns[name] != nil {
		t.Fatal("server group outlived its owner")
	}
}

func TestInProcessCallCleanupPrecedesMountedServiceRelease(t *testing.T) {
	for _, cancel := range []string{"mount", "caller"} {
		t.Run(cancel, func(t *testing.T) {
			mount, caller := rootFiber(), rootFiber()
			mount.owner, caller.owner = newOwner(), newOwner()
			started, cleaned := make(chan struct{}), make(chan struct{})
			done := make(chan *Cause, 1)
			proc := ServerProcedure("test/local-owner/"+cancel, SchemaString, SchemaString, ServerNoErrors)
			ServerHandler(mount, []Value{ServerFunction(proc, Fn(func(fb *Fiber, _ Value) Value {
				fb.owner.add(fb, func(*Fiber, *Cause) Value { close(cleaned); return Unit })
				close(started)
				wait(fb, make(chan struct{}))
				return "unreachable"
			}))})
			go func() {
				_, cause := catch(func() Value { return ServerCall(caller, proc, "start") })
				done <- cause
			}()
			select {
			case <-started:
			case <-time.After(3 * time.Second):
				t.Fatal("call did not start")
			}
			if cancel == "mount" {
				mount.owner.add(mount, func(*Fiber, *Cause) Value {
					if !isClosed(cleaned) {
						t.Error("released mounted services before in-process call cleanup")
					}
					return Unit
				})
				mount.owner.close(mount, nil)
				if caller.state.requested.Load() {
					t.Error("mount cancellation interrupted the caller's fiber state")
				}
			}
			// Also unblock the old implementation after recording the mount
			// ordering failure, so a red regression leaves no running call.
			caller.state.interrupt(0)
			select {
			case cause := <-done:
				if cause == nil || !cause.has(causeInterrupt) || !isClosed(cleaned) {
					t.Error("call did not join interrupted handler cleanup")
				}
			case <-time.After(3 * time.Second):
				t.Fatal("interrupted call did not finish")
			}
			if cancel == "caller" {
				mount.owner.close(mount, nil)
			}
			caller.owner.close(caller, nil)
		})
	}
}

func TestInProcessCallRefusesAClosedMount(t *testing.T) {
	mount := rootFiber()
	mount.owner = newOwner()
	called := false
	proc := ServerProcedure("test/closed-mount", SchemaString, SchemaString, ServerNoErrors)
	ServerHandler(mount, []Value{ServerFunction(proc, Fn(func(*Fiber, Value) Value {
		called = true
		return "unreachable"
	}))})
	localMu.Lock()
	here := localFns["test/closed-mount"]
	localMu.Unlock()
	mount.owner.close(mount, nil)
	// A caller may already have read the registry entry while the mount
	// closes. Keeping the pointer must not admit work into released services.
	_, cause := catch(func() Value { return here.call(rootFiber(), "start") })
	if cause == nil || called {
		t.Fatal("closed mount admitted an in-process call")
	}
}
