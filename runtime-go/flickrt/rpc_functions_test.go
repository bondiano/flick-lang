package flickrt

import "testing"

func TestServerMountSurvivesRepeatedCallsAndClosesWithOwner(t *testing.T) {
	const name = "test/mounted"
	calls := 0
	owned(rootFiber(), func(fb *Fiber, _ *Owner) Value {
		proc := ServerProcedure(name, SchemaString, SchemaString, ServerNoErrors)
		mounted := ServerHandler(fb, []Value{ServerFunction(proc, Fn(func(_ *Fiber, input Value) Value {
			calls++
			return input.(string) + "!"
		}))}).(*serverMount)
		localMu.Lock()
		first := localFns[name]
		localMu.Unlock()
		if mounted.handler == nil || first == nil {
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
