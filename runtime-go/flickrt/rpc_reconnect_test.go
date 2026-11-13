package flickrt

import (
	"bufio"
	"io"
	"net"
	"net/http"
	"testing"
	"time"
)

func TestRPCScopedClientReconnectsWithoutAnotherCall(t *testing.T) {
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	defer listener.Close()
	accepted := make(chan struct{})
	ended := make(chan struct{})
	go func() {
		defer close(ended)
		first, err := listener.Accept()
		if err != nil {
			return
		}
		first.Close()
		second, err := listener.Accept()
		if err != nil {
			return
		}
		defer second.Close()
		close(accepted)
		io.Copy(io.Discard, second)
	}()
	owned(rootFiber(), func(fb *Fiber, _ *Owner) Value {
		options := Rec("serialization", RpcNdjson, "transport", RpcTcp)
		client := RpcConnect(fb, options, "tcp://"+listener.Addr().String()).(*rpcClient)
		client.sock.ensure(fb)
		select {
		case <-accepted:
		case <-time.After(3 * time.Second):
			t.Error("idle scoped socket did not reconnect")
		}
		return Unit
	})
	listener.Close()
	select {
	case <-ended:
	case <-time.After(3 * time.Second):
		t.Fatal("client owner did not join socket shutdown")
	}
}

func TestRPCSocketFailureDoesNotWaitForPausedStream(t *testing.T) {
	inflight := &rpcInflight{ch: make(chan Value, 1), failure: make(chan Value, 1), gone: make(chan struct{})}
	inflight.ch <- chunkMsg(1.0, []Value{1.0})
	finished := make(chan struct{})
	go func() { inflight.fail(protocolError("disconnected")); close(finished) }()
	select {
	case <-finished:
	case <-time.After(time.Second):
		t.Fatal("transport shutdown waited for a paused stream")
	}
	if got := <-inflight.failure; msgTag(got) != "ClientProtocolError" {
		t.Fatalf("failure %v", got)
	}
}

func TestRPCOwnerInterruptsUnfinishedWebsocketHandshake(t *testing.T) {
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	defer listener.Close()
	started := make(chan struct{})
	connection := make(chan net.Conn, 1)
	serverDone := make(chan struct{})
	go func() {
		defer close(serverDone)
		conn, err := listener.Accept()
		if err != nil {
			return
		}
		defer conn.Close()
		connection <- conn
		if _, err := http.ReadRequest(bufio.NewReader(conn)); err != nil {
			return
		}
		close(started)
		io.Copy(io.Discard, conn) // Never send the WebSocket handshake response.
	}()
	ownerDone := make(chan struct{})
	go func() {
		defer close(ownerDone)
		owned(rootFiber(), func(fb *Fiber, _ *Owner) Value {
			c := RpcConnect(fb, Rec("serialization", RpcNdjson, "transport", RpcWebsocket), "ws://"+listener.Addr().String()).(*rpcClient)
			FiberSpawn(fb, fb.owner, Fn(func(*Fiber, Value) Value { return Unit }), Eff(func(fb *Fiber) Value {
				c.sock.ensure(fb)
				return Unit
			}))
			wait(fb, started)
			return Unit
		})
	}()
	select {
	case <-ownerDone:
	case <-time.After(3 * time.Second):
		select {
		case conn := <-connection:
			conn.Close()
		default:
		}
		t.Fatal("owner could not interrupt a pending handshake")
	}
	select {
	case <-serverDone:
	case <-time.After(time.Second):
		t.Fatal("handshake connection outlived its owner")
	}
}

func TestRPCWebsocketShutdownUnblocksAStalledWriter(t *testing.T) {
	local, remote := net.Pipe()
	defer remote.Close()
	ws := &wsConn{conn: local}
	writing := make(chan struct{})
	written := make(chan struct{})
	go func() { close(writing); ws.write([]byte("blocked"), true); close(written) }()
	<-writing
	closed := make(chan struct{})
	go func() { ws.shut(1000); close(closed) }()
	select {
	case <-closed:
	case <-time.After(3 * time.Second):
		local.Close()
		t.Fatal("WebSocket shutdown waited forever for its peer")
	}
	select {
	case <-written:
	case <-time.After(time.Second):
		t.Fatal("writer did not end with its socket")
	}
}
