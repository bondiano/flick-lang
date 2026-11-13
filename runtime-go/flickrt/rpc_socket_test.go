package flickrt

import (
	"io"
	"sync"
	"testing"
)

type rpcReplySocket struct {
	frame             []byte
	consumed, release chan struct{}
	once              sync.Once
}

func (s *rpcReplySocket) read() ([]byte, error) {
	if s.frame != nil {
		frame := s.frame
		s.frame = nil
		return frame, nil
	}
	close(s.consumed)
	<-s.release
	return nil, io.EOF
}
func (*rpcReplySocket) write([]byte, bool) error { return nil }
func (s *rpcReplySocket) shut(uint16)            { s.once.Do(func() { close(s.release) }) }

func TestRPCSocketIgnoresLateRepliesOfCanceledCalls(t *testing.T) {
	ser := serializationOf(RpcNdjson)
	frames := []Value{exitMsg(1.0, exitSuccessTree("canceled")), chunkMsg(3.0, []Value{"unknown"}), exitMsg(2.0, exitSuccessTree("live"))}
	socket := &rpcReplySocket{frame: ser.parser().encode(frames, true), consumed: make(chan struct{}), release: make(chan struct{})}
	live := &rpcInflight{ch: make(chan Value, 16), failure: make(chan Value, 1), gone: make(chan struct{})}
	client := &rpcSocketClient{ser: ser, sock: socket, calls: map[string]*rpcInflight{idKey(2.0): live}, stopPing: make(chan struct{})}
	ended := make(chan struct{})
	go func() { defer close(ended); client.readLoop(socket, &lockedParser{p: ser.parser()}) }()
	defer func() { socket.shut(1000); <-ended }()
	<-socket.consumed
	response := (&socketResponses{f: live}).next(rootFiber())
	if msgField(response, "requestId") != 2.0 || msgField(msgField(response, "exit"), "value") != "live" {
		t.Fatalf("unrelated active call received %v", response)
	}
	select {
	case extra := <-live.failure:
		t.Fatalf("late reply failed live call: %v", extra)
	default:
	}
	select {
	case extra := <-live.ch:
		t.Fatalf("unknown chunk entered live call: %v", extra)
	default:
	}
}

func TestRPCGlobalDefectStillFailsEverySocketCall(t *testing.T) {
	ser := serializationOf(RpcNdjson)
	socket := &rpcReplySocket{frame: ser.parser().encode([]Value{defectMsg("global")}, false), consumed: make(chan struct{}), release: make(chan struct{})}
	calls := map[string]*rpcInflight{}
	for _, id := range []Value{1.0, 2.0} {
		calls[idKey(id)] = &rpcInflight{ch: make(chan Value, 1), failure: make(chan Value, 1), gone: make(chan struct{})}
	}
	client := &rpcSocketClient{ser: ser, sock: socket, calls: calls, stopPing: make(chan struct{})}
	ended := make(chan struct{})
	go func() { defer close(ended); client.readLoop(socket, &lockedParser{p: ser.parser()}) }()
	defer func() { socket.shut(1000); <-ended }()
	<-socket.consumed
	for id, call := range calls {
		select {
		case response := <-call.failure:
			if msgTag(response) != "Defect" || msgField(response, "defect") != "global" {
				t.Fatalf("call %s: %v", id, response)
			}
		default:
			t.Fatalf("global defect did not fail call %s", id)
		}
	}
}
