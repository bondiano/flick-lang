package flickrt

import (
	"sync"
	"time"
)

// A client's socket, as Effect's socket protocol: the calls share it,
// each answer goes to its call by request id, a chunk is acknowledged once
// its call has room for it, and a ping goes every five seconds; a
// connection silent since the last one is broken. A broken connection
// fails its calls with a ClientError, and the next call opens another.
//
// shortcut: a broken connection is opened again by the next call, not in the background with Effect's retry schedule; retry when programs keep idle connections

const rpcPingInterval = 5 * time.Second

type rpcSocketClient struct {
	ser       *rpcSerialization
	transport rpcTransport
	url       string
	mu        sync.Mutex
	sock      rpcSocket
	parser    *lockedParser
	calls     map[string]*rpcInflight
	seen      time.Time // the last frame of the server
	pinged    bool      // a ping waits for any frame
	stopPing  chan struct{}
	closed    bool
}

// rpcInflight is a call waiting for its answers.
type rpcInflight struct {
	ch   chan Value
	gone chan struct{}
	once sync.Once
}

func (f *rpcInflight) leave() { f.once.Do(func() { close(f.gone) }) }

func (f *rpcInflight) deliver(m Value) bool {
	select {
	case f.ch <- m:
		return true
	case <-f.gone:
		return false
	}
}

func protocolError(message string) Value {
	return Rec("_tag", "ClientProtocolError", "error", rpcClientError(message))
}

// ensure is the open socket, opening it if it is not.
func (c *rpcSocketClient) ensure(fb *Fiber) (rpcSocket, *lockedParser) {
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.sock != nil {
		return c.sock, c.parser
	}
	if c.closed {
		Fail(rpcClientError("SocketCloseError: the client is closed"))
	}
	sock, err := dialSocket(fb, c.transport, c.url)
	if err != nil {
		Fail(rpcClientError("SocketOpenError: " + err.Error()))
	}
	c.sock, c.parser = sock, &lockedParser{p: c.ser.parser()}
	c.seen, c.pinged, c.stopPing = time.Now(), false, make(chan struct{})
	concurrent.Store(true)
	go c.readLoop(sock, c.parser)
	go c.pingLoop(sock, c.parser, c.stopPing)
	return sock, c.parser
}

func (c *rpcSocketClient) write(sock rpcSocket, parser *lockedParser, m Value) error {
	if b := parser.encode([]Value{m}, false); b != nil {
		return sock.write(b, !c.ser.binary)
	}
	return nil
}

// broken fails the calls of a connection that ended, and forgets it.
func (c *rpcSocketClient) broken(sock rpcSocket, message string) {
	c.mu.Lock()
	if c.sock != sock {
		c.mu.Unlock()
		return
	}
	c.sock = nil
	close(c.stopPing)
	calls := c.calls
	c.calls = map[string]*rpcInflight{}
	c.mu.Unlock()
	sock.shut(1000)
	for _, f := range calls {
		f.deliver(protocolError(message))
	}
}

func (c *rpcSocketClient) readLoop(sock rpcSocket, parser *lockedParser) {
	for {
		data, err := sock.read()
		if err != nil {
			c.broken(sock, "SocketCloseError: the connection closed")
			return
		}
		msgs, err := parser.decode(data)
		if err != nil {
			c.broadcast(protocolError("RpcClientDefect: Error decoding message"))
			continue
		}
		if len(msgs) > 0 {
			c.mu.Lock()
			c.seen, c.pinged = time.Now(), false
			c.mu.Unlock()
		}
		for _, m := range msgs {
			if msgTag(m) == "Pong" {
				continue
			}
			id := msgField(m, "requestId")
			if id == nil {
				c.broadcast(m)
				continue
			}
			key := idKey(id)
			c.mu.Lock()
			f := c.calls[key]
			if f != nil && msgTag(m) == "Exit" {
				delete(c.calls, key)
			}
			c.mu.Unlock()
			if f == nil {
				c.broadcast(m)
				continue
			}
			if f.deliver(m) && msgTag(m) == "Chunk" {
				c.write(sock, parser, Rec("_tag", "Ack", "requestId", id))
			}
		}
	}
}

// broadcast gives a message of no call, a Defect, to every call.
func (c *rpcSocketClient) broadcast(m Value) {
	c.mu.Lock()
	calls := make([]*rpcInflight, 0, len(c.calls))
	for _, f := range c.calls {
		calls = append(calls, f)
	}
	if isTerminal(m) {
		c.calls = map[string]*rpcInflight{}
	}
	c.mu.Unlock()
	for _, f := range calls {
		f.deliver(m)
	}
}

func (c *rpcSocketClient) pingLoop(sock rpcSocket, parser *lockedParser, stop chan struct{}) {
	t := time.NewTicker(rpcPingInterval)
	defer t.Stop()
	for {
		select {
		case <-stop:
			return
		case <-t.C:
		}
		c.mu.Lock()
		silent := c.pinged && time.Since(c.seen) > rpcPingInterval
		c.pinged = true
		c.mu.Unlock()
		if silent {
			c.broken(sock, "SocketReadError: ping timeout")
			return
		}
		c.write(sock, parser, pingMsg)
	}
}

func (c *rpcSocketClient) close() {
	c.mu.Lock()
	c.closed = true
	sock := c.sock
	c.mu.Unlock()
	if sock != nil {
		c.broken(sock, "SocketCloseError: the client is closed")
	}
}

// socketResponses are the answers of a call over the socket.
type socketResponses struct {
	c      *rpcSocketClient
	sock   rpcSocket
	parser *lockedParser
	id     Value
	f      *rpcInflight
	done   bool
}

func (c *rpcSocketClient) start(fb *Fiber, m Value) rpcResponses {
	sock, parser := c.ensure(fb)
	id := msgField(m, "id")
	f := &rpcInflight{ch: make(chan Value, 16), gone: make(chan struct{})}
	c.mu.Lock()
	c.calls[idKey(id)] = f
	c.mu.Unlock()
	if err := c.write(sock, parser, m); err != nil {
		c.broken(sock, "SocketWriteError: "+err.Error())
	}
	return &socketResponses{c: c, sock: sock, parser: parser, id: id, f: f}
}

func (r *socketResponses) next(fb *Fiber) Value {
	idle()
	var m Value
	if fb.state.mask > 0 {
		m = <-r.f.ch
	} else {
		select {
		case m = <-r.f.ch:
		case <-fb.state.signal:
			panic(fb.state.stopped())
		}
	}
	r.done = r.done || isTerminal(m)
	return m
}

// stop interrupts the call on the server, unless it ended.
func (r *socketResponses) stop() {
	r.f.leave()
	if r.done {
		return
	}
	r.done = true
	key := idKey(r.id)
	r.c.mu.Lock()
	_, waiting := r.c.calls[key]
	delete(r.c.calls, key)
	r.c.mu.Unlock()
	if waiting {
		r.c.write(r.sock, r.parser, Rec("_tag", "Interrupt", "requestId", r.id))
	}
}
