package flickrt

import "sync"

// rpcSocketAcquisition owns a successful foreign result until the calling
// fiber adopts it. Interruption may win after open succeeds but before
// ForeignAsync delivers its result; joining the foreign work alone does
// not release that connection.
type rpcSocketAcquisition struct {
	mu        sync.Mutex
	pending   rpcSocket
	abandoned bool
}

func (a *rpcSocketAcquisition) offer(socket rpcSocket) {
	a.mu.Lock()
	abandoned := a.abandoned
	if !abandoned {
		a.pending = socket
	}
	a.mu.Unlock()
	if abandoned && socket != nil {
		socket.shut(1000)
	}
}

func (a *rpcSocketAcquisition) adopt() {
	a.mu.Lock()
	a.pending = nil
	a.mu.Unlock()
}

func (a *rpcSocketAcquisition) abandon() {
	a.mu.Lock()
	a.abandoned = true
	pending := a.pending
	a.pending = nil
	a.mu.Unlock()
	if pending != nil {
		pending.shut(1000)
	}
}

// acquireRPCSocket bridges a successful socket acquisition from foreign
// work back into its calling fiber, before the client admits the socket.
func acquireRPCSocket(fb *Fiber, open func(Context) (rpcSocket, error)) (rpcSocket, error) {
	type dialed struct {
		sock rpcSocket
		err  error
	}
	var acquisition rpcSocketAcquisition
	defer acquisition.abandon()
	got := ForeignAsync(fb, func(ctx Context) Value {
		socket, err := open(ctx)
		acquisition.offer(socket)
		return dialed{socket, err}
	}).(dialed)
	acquisition.adopt()
	return got.sock, got.err
}
