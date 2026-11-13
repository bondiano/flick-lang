package flickrt

import (
	"testing"
	"time"
)

func TestRPCOwnerJoinsInterruptedCallBeforeReleasingServices(t *testing.T) {
	started := make(chan struct{})
	cleaned := make(chan struct{})
	closed := make(chan struct{})
	go func() {
		defer close(closed)
		owned(rootFiber(), func(fb *Fiber, _ *Owner) Value {
			p := rpcProcOf(RpcMake("wait"))
			g := &rpcGroup{procs: []*rpcProc{p}}
			handler := Fn(func(fb *Fiber, _ Value) Value {
				fb.owner.add(fb, func(*Fiber, *Cause) Value { close(cleaned); return Unit })
				close(started)
				wait(fb, make(chan struct{}))
				return Unit
			})
			srv := newRPCServer(fb, g, map[string]Value{"wait": handler})
			conn := srv.connect(serializationOf(RpcNdjson), nil, false, func(Value) {}, nil)
			conn.receive(Rec("_tag", "Request", "id", 1.0, "tag", "wait", "payload", JsonNull, "headers", []Value{}))
			wait(fb, started)
			// Registered last: a resource finalizer must still run after calls join.
			fb.owner.add(fb, func(*Fiber, *Cause) Value {
				select {
				case <-cleaned:
				default:
					t.Error("released services before RPC call cleanup")
				}
				return Unit
			})
			return Unit
		})
	}()
	select {
	case <-closed:
	case <-time.After(3 * time.Second):
		t.Fatal("RPC owner did not interrupt and join its call")
	}
}
