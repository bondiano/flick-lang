package flickrt

import (
	"io"
	"net"
	"testing"
	"time"
)

func TestRPCInterruptedSocketAcquisitionClosesLateResult(t *testing.T) {
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	defer listener.Close()
	connected, returned := make(chan struct{}), make(chan struct{})
	var acquired net.Conn
	owned(rootFiber(), func(fb *Fiber, _ *Owner) Value {
		worker := fork(fb, fb.owner, Eff(func(fb *Fiber) Value {
			_, err := acquireRPCSocket(fb, func(ctx Context) (rpcSocket, error) {
				defer close(returned)
				var dial net.Dialer
				connection, err := dial.DialContext(ctx, "tcp", listener.Addr().String())
				if err != nil {
					close(connected)
					return nil, err
				}
				acquired = connection
				close(connected)
				<-ctx.Done() // The connection exists before cancellation, but ownership has not transferred.
				return &tcpSocket{conn: connection}, nil
			})
			if err != nil {
				t.Error(err)
			}
			return Unit
		}), nil)
		fb.owner.onClose(fb, func(fb *Fiber) { stop(fb, worker) })
		wait(fb, connected)
		return Unit
	})
	if acquired == nil {
		t.Fatal("test could not acquire its TCP connection")
	}
	defer acquired.Close()
	select {
	case <-returned:
	default:
		t.Fatal("owner did not join the interrupted acquisition")
	}
	peer, err := listener.Accept()
	if err != nil {
		t.Fatal(err)
	}
	defer peer.Close()
	peer.SetReadDeadline(time.Now().Add(100 * time.Millisecond))
	if _, err := peer.Read(make([]byte, 1)); err != io.EOF {
		t.Fatalf("interrupted successful dial leaked its connection: %v", err)
	}
}

func TestRPCSocketAcquisitionClosesUnadoptedBufferedResult(t *testing.T) {
	local, peer := net.Pipe()
	defer local.Close()
	defer peer.Close()
	var acquisition rpcSocketAcquisition
	acquisition.offer(&tcpSocket{conn: local})
	// The foreign result has been published, but interruption wins before
	// the fiber receives it. Cleanup must release the buffered resource.
	acquisition.abandon()
	peer.SetReadDeadline(time.Now().Add(100 * time.Millisecond))
	if _, err := peer.Read(make([]byte, 1)); err != io.EOF {
		t.Fatalf("buffered successful dial leaked its connection: %v", err)
	}
}

func TestRPCSocketAcquisitionTransfersSuccessfulResult(t *testing.T) {
	local, peer := net.Pipe()
	defer local.Close()
	defer peer.Close()
	owned(rootFiber(), func(fb *Fiber, _ *Owner) Value {
		socket, err := acquireRPCSocket(fb, func(Context) (rpcSocket, error) { return &tcpSocket{conn: local}, nil })
		if err != nil {
			t.Fatal(err)
		}
		written := make(chan error, 1)
		go func() { written <- socket.write([]byte("ok"), true) }()
		peer.SetReadDeadline(time.Now().Add(3 * time.Second))
		data := make([]byte, 2)
		if _, err := io.ReadFull(peer, data); err != nil || string(data) != "ok" {
			t.Fatalf("adopted connection was closed: %q, %v", data, err)
		}
		if err := <-written; err != nil {
			t.Fatal(err)
		}
		socket.shut(1000)
		return Unit
	})
}
