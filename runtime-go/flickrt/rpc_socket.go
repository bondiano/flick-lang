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
	reconnect chan struct{}
	opening   chan struct{}
}

// rpcInflight is a call waiting for its answers.
type rpcInflight struct {
	ch      chan Value
	failure chan Value
	gone    chan struct{}
	once    sync.Once
}

func (f *rpcInflight) leave() { f.once.Do(func() { close(f.gone) }) }

// Transport failure has its own slot: a stopped stream may already have
// filled its bounded chunk queue, and shutdown must never wait for a pull.
func (f *rpcInflight) fail(message Value) {
	select {
	case f.failure <- message:
	default:
	}
}

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
	for {
		c.mu.Lock()
		if c.closed {
			c.mu.Unlock()
			Fail(rpcClientError("SocketCloseError: the client is closed"))
		}
		if c.sock != nil {
			sock, parser := c.sock, c.parser
			c.mu.Unlock()
			return sock, parser
		}
		if c.opening != nil {
			opening := c.opening
			c.mu.Unlock()
			wait(fb, opening)
			continue
		}
		c.opening = make(chan struct{})
		c.mu.Unlock()
		var sock rpcSocket
		var err error
		_, cause := catch(func() Value { sock, err = dialSocket(fb, c.transport, c.url); return Unit })
		c.mu.Lock()
		close(c.opening)
		c.opening = nil
		closed := c.closed
		if cause == nil && err == nil && !closed {
			c.sock, c.parser = sock, &lockedParser{p: c.ser.parser()}
			c.seen, c.pinged, c.stopPing = time.Now(), false, make(chan struct{})
			parser, stop := c.parser, c.stopPing
			c.mu.Unlock()
			concurrent.Store(true)
			go c.readLoop(sock, parser)
			go c.pingLoop(sock, parser, stop)
			return sock, parser
		}
		c.mu.Unlock()
		if sock != nil {
			sock.shut(1000)
		}
		if cause != nil {
			panic(&Failure{cause})
		}
		if closed {
			Fail(rpcClientError("SocketCloseError: the client is closed"))
		}
		Fail(rpcClientError("SocketOpenError: " + err.Error()))
	}
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
	retry := !c.closed
	c.mu.Unlock()
	sock.shut(1000)
	for _, f := range calls {
		f.fail(protocolError(message))
	}
	if retry && c.reconnect != nil {
		notify(c.reconnect)
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
		f.fail(m)
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
	var sock rpcSocket
	var parser *lockedParser
	id := msgField(m, "id")
	f := &rpcInflight{ch: make(chan Value, 16), failure: make(chan Value, 1), gone: make(chan struct{})}
	for {
		sock, parser = c.ensure(fb)
		c.mu.Lock()
		if c.sock == sock && !c.closed {
			c.calls[idKey(id)] = f
			c.mu.Unlock()
			break
		}
		c.mu.Unlock()
		Check(fb)
	}
	if err := c.write(sock, parser, m); err != nil {
		c.broken(sock, "SocketWriteError: "+err.Error())
	}
	return &socketResponses{c: c, sock: sock, parser: parser, id: id, f: f}
}

func (r *socketResponses) next(fb *Fiber) Value {
	idle()
	var m Value
	if fb.state.mask > 0 {
		select {
		case m = <-r.f.failure:
		case m = <-r.f.ch:
		}
	} else {
		select {
		case m = <-r.f.ch:
		case m = <-r.f.failure:
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

// reconnectLoop follows Effect's default min(exponential(500,1.5),
// spaced(5000)) policy. It is interrupted and joined by the client's owner.
func (c *rpcSocketClient) reconnectLoop(fb *Fiber) Value {
	for {
		wait(fb, c.reconnect)
		delay := 500 * time.Millisecond
		for {
			sleep(fb, delay)
			_, cause := catch(func() Value { c.ensure(fb); return Unit })
			if cause == nil {
				break
			}
			if cause.has(causeInterrupt) || cause.has(causeDie) {
				panic(&Failure{cause})
			}
			delay = min(delay*3/2, 5*time.Second)
		}
	}
}
