package flickrt

import (
	"sync"
	"testing"
	"time"
)

// Pinned Effect HTTP waits for ClientEnd (all calls, including silent calls).
// Transport disconnect interrupts every request; the mount owner joins cleanup.
func TestRPCSilentHTTPCallsWaitAndDisconnect(t *testing.T) {
	for _, flag := range []string{"discard", "isNotification"} {
		t.Run(flag, func(t *testing.T) {
			started, cleaned := make(chan struct{}), make(chan struct{})
			owned(rootFiber(), func(fb *Fiber, _ *Owner) Value {
				p := rpcProcOf(RpcMake("wait"))
				handler := Fn(func(*Fiber, Value) Value {
					return Eff(func(fb *Fiber) Value {
						fb.owner.add(fb, func(*Fiber, *Cause) Value { close(cleaned); return Unit })
						close(started)
						wait(fb, make(chan struct{}))
						return Unit
					})
				})
				srv := newRPCServer(fb, &rpcGroup{procs: []*rpcProc{p}}, map[string]Value{"wait": handler})
				conn := srv.connect(serializationOf(RpcNdjson), nil, false, func(Value) {}, nil)
				conn.receive(Rec("_tag", "Request", "id", 1.0, "tag", "wait", "payload", JsonNull, "headers", []Value{}, flag, true))
				conn.receive(eofMsg)
				wait(fb, started)
				select {
				case <-conn.done:
					t.Error("HTTP response ended before silent handler cleanup")
				default:
				}
				conn.abort()
				select {
				case <-cleaned:
				case <-time.After(time.Second):
					conn.close()
					t.Error("transport disconnect did not interrupt silent handler")
				}
				return Unit
			})
		})
	}
}

// Expected replies follow Effect's RpcServer contract, independently of Go's
// serializer: notifications suppress even invalid requests, discard failures
// before acknowledgement remain local Exits, and silent streams need no Ack.
func TestRPCFlagReplies(t *testing.T) {
	for _, tc := range []struct {
		name, flag                                                      string
		stream, middlewareFailure, constructionFailure, unknown, binary bool
		want                                                            int
	}{
		{name: "notification unknown", flag: "isNotification", unknown: true},
		{name: "notification middleware defect", flag: "isNotification", middlewareFailure: true},
		{name: "discard middleware defect", flag: "discard", middlewareFailure: true, want: 1},
		{name: "discard construction defect", flag: "discard", constructionFailure: true, want: 1},
		{name: "discard binary void acknowledgement", flag: "discard", binary: true, want: 1},
		{name: "discard stream", flag: "discard", stream: true, want: 1},
		{name: "notification stream", flag: "isNotification", stream: true},
	} {
		t.Run(tc.name, func(t *testing.T) {
			var replies []Value
			var mu sync.Mutex
			p := rpcProcOf(RpcMake("call"))
			p.stream = tc.stream
			if tc.middlewareFailure {
				p.middleware = []Value{Fn(func(*Fiber, Value) Value { panic(Defect{"middleware"}) })}
			}
			handler := Fn(func(*Fiber, Value) Value {
				if tc.constructionFailure {
					panic(Defect{"construction"})
				}
				if tc.stream {
					return StreamFromArrays([]Value{[]Value{1.0, 2.0}})
				}
				return Eff(func(*Fiber) Value { return "ignored" })
			})
			var reports []string
			logger := &logger{log: func(_ *Fiber, m *logEntry) Value { reports = append(reports, m.level); return Unit }}
			env := rootFiber().env.with(loggersKey, []Value{logger})
			srv := makeRPCServer(env, &rpcGroup{procs: []*rpcProc{p}}, map[string]Value{"call": handler})
			ser := serializationOf(RpcNdjson)
			if tc.binary {
				ser = serializationOf(RpcBinary)
			}
			conn := srv.connect(ser, nil, true, func(v Value) { mu.Lock(); replies = append(replies, v); mu.Unlock() }, nil)
			tag := "call"
			if tc.unknown {
				tag = "unknown"
			}
			var payload Value = JsonNull
			if tc.binary {
				var err error
				payload, err = binEncode(p.layouts().payload, JsonNull)
				if err != nil {
					t.Fatal(err)
				}
			}
			conn.receive(Rec("_tag", "Request", "id", 1.0, "tag", tag, "payload", payload, "headers", []Value{}, tc.flag, true))
			conn.receive(eofMsg)
			joined := make(chan struct{})
			go func() { srv.calls.Wait(); close(joined) }()
			select {
			case <-joined:
			case <-time.After(2 * time.Second):
				conn.close()
				t.Fatal("silent stream blocked waiting for Ack")
			}
			mu.Lock()
			defer mu.Unlock()
			if tc.middlewareFailure || tc.constructionFailure {
				if len(reports) != 1 || reports[0] != "Error" {
					t.Fatalf("failure reports: %v", reports)
				}
			}
			if len(replies) != tc.want {
				t.Fatalf("got %d replies, want %d", len(replies), tc.want)
			}
			if tc.want == 0 {
				return
			}
			if msgTag(replies[0]) != "Exit" {
				t.Fatalf("expected local Exit, got %s", msgTag(replies[0]))
			}
			exit := msgField(replies[0], "exit")
			if tc.binary {
				layout := *p.layouts().exit
				layout.value = binLeaf(binUndefined)
				var err error
				exit, err = binDecode(&layout, exit.([]byte))
				if err != nil {
					t.Fatal(err)
				}
			}
			want := "Success"
			if tc.middlewareFailure || tc.constructionFailure {
				want = "Failure"
			}
			if msgTag(exit) != want {
				t.Fatalf("got %s, want %s", msgTag(exit), want)
			}
		})
	}
}
