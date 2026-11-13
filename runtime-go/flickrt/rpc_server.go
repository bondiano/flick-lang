package flickrt

import (
	"strconv"
	"strings"
	"sync"
)

// The dispatcher of Effect's RpcServer: each client (a POST, a WebSocket,
// a TCP connection) sends requests whose payloads its serialization
// decodes; each call runs in a fiber of its own with the services where
// the group was served, after the procedure's hooks and its check; its
// exit goes back coded by the procedure's codecs. A defect of a handler
// is fatal to its client, as Effect sends it: a Defect message. Over a
// socket a stream waits for the client's Ack of a chunk before the next.

type rpcServer struct {
	procs    map[string]*rpcProc
	handlers map[string]Value
	services *env
	mu       sync.Mutex
	conns    map[*rpcConn]struct{}
	closed   bool
	calls    sync.WaitGroup
}

// makeRPCServer serves the group with the handlers and the services `env`.
func makeRPCServer(env *env, g *rpcGroup, handlers map[string]Value) *rpcServer {
	s := &rpcServer{procs: make(map[string]*rpcProc, len(g.procs)), handlers: handlers, services: env, conns: map[*rpcConn]struct{}{}}
	for _, p := range g.procs {
		s.procs[p.name] = p
	}
	return s
}

// newRPCServer serves the group with the services of `fb` until the
// innermost owner closes: then the calls still running (those of
// sockets: an HTTP server has let its requests finish by then) are
// interrupted and awaited, and the sockets closed.
func newRPCServer(fb *Fiber, g *rpcGroup, handlers map[string]Value) *rpcServer {
	s := makeRPCServer(fb.env, g, handlers)
	fb.owner.onClose(fb, func(*Fiber) {
		s.mu.Lock()
		s.closed = true
		conns := make([]*rpcConn, 0, len(s.conns))
		for c := range s.conns {
			conns = append(conns, c)
		}
		s.mu.Unlock()
		for _, c := range conns {
			c.close()
		}
		s.calls.Wait()
	})
	return s
}

type rpcCall struct {
	key       string // internal tracking identity; notifications have no wire id
	state     *fiberState
	ack       chan struct{} // a stream's acknowledged chunks, over a socket
	responded bool          // its exit went out already
	silent    bool          // notification, or an acknowledged discarded call
	discard   bool
}

// rpcConn is a client of the server.
type rpcConn struct {
	srv           *rpcServer
	ser           *rpcSerialization
	headers       []Value       // the transport's, before each request's own
	acks          bool          // whether the client acknowledges chunks
	send          func(m Value) // writes a message; safe for concurrent use
	closer        func()        // closes the transport, or nil
	mu            sync.Mutex
	calls         map[string]*rpcCall
	ended         bool
	notifications uint64
	done          chan struct{} // closed once it ended and its calls did
	once          sync.Once
}

func (s *rpcServer) connect(ser *rpcSerialization, headers []Value, acks bool, send func(Value), closer func()) *rpcConn {
	c := &rpcConn{srv: s, ser: ser, headers: headers, acks: acks, send: send, closer: closer,
		calls: map[string]*rpcCall{}, done: make(chan struct{})}
	s.mu.Lock()
	closed := s.closed
	if !closed {
		s.conns[c] = struct{}{}
	}
	s.mu.Unlock()
	if closed {
		c.close()
	}
	return c
}

// finish ends the client once its calls ended.
func (c *rpcConn) finish() {
	c.once.Do(func() {
		close(c.done)
		c.srv.mu.Lock()
		delete(c.srv.conns, c)
		c.srv.mu.Unlock()
	})
}

// abort interrupts every request when its transport disconnects, including
// notifications and acknowledged discarded calls. EOF alone waits for them.
func (c *rpcConn) abort() {
	c.mu.Lock()
	c.ended = true
	idle := len(c.calls) == 0
	for _, call := range c.calls {
		call.state.interrupt(0)
	}
	c.mu.Unlock()
	if idle {
		c.finish()
	}
}

func (c *rpcConn) close() {
	c.abort()
	if c.closer != nil {
		c.closer()
	}
}

// hole is a hole of a message as the serialization writes it.
func (c *rpcConn) hole(n *binNode, tree Value) (Value, error) {
	if !c.ser.binary {
		return tree, nil
	}
	b, err := binEncode(n, tree)
	return b, err
}

func (c *rpcConn) defect(tree Value) {
	if v, err := c.hole(binDefectLayout, tree); err == nil {
		c.send(defectMsg(v))
	}
}

// exit sends a call's exit; one its codecs cannot write is a defect.
func (c *rpcConn) exit(n *binNode, id, tree Value) {
	v, err := c.hole(n, tree)
	if err != nil {
		v, err = c.hole(n, exitFailureTree(dieReason(err.Error())))
		if err != nil {
			c.defect(err.Error())
			return
		}
	}
	c.send(exitMsg(id, v))
}

// receive handles a message of the client.
func (c *rpcConn) receive(m Value) {
	tag := msgTag(m)
	c.mu.Lock()
	ended := c.ended
	var call *rpcCall
	if tag == "Ack" || tag == "Interrupt" {
		call = c.calls[idKey(msgField(m, "requestId"))]
	}
	c.mu.Unlock()
	if ended && call == nil {
		return
	}
	switch tag {
	case "Request":
		c.request(m)
	case "Ack":
		if call != nil && call.ack != nil {
			select {
			case call.ack <- struct{}{}:
			default:
			}
		}
	case "Interrupt":
		if call != nil {
			call.state.interrupt(0)
		}
	case "Ping":
		c.send(pongMsg)
	case "Eof":
		c.mu.Lock()
		c.ended = true
		idle := len(c.calls) == 0
		c.mu.Unlock()
		if idle {
			c.finish()
		}
	default:
		name := "undefined"
		if t, ok := msgField(m, "_tag").(string); ok {
			name = t
		}
		c.defect("Unknown request tag: " + name)
	}
}

// decodeWire is a value its codec reads, Schema.Void's `null` without one.
func decodeWire(fb *Fiber, c codec, in Value) (Value, []issue) {
	if c == nil {
		if _, ok := in.(*jsonNull); !ok {
			return nil, fail("Expected null")
		}
		return Unit, nil
	}
	return c.decode(fb, in)
}

func encodeWire(fb *Fiber, c codec, v Value) (Value, []issue) {
	if c == nil {
		return JsonNull, nil
	}
	return c.encode(fb, v)
}

func issuesText(issues []issue) string {
	parts := make([]string, len(issues))
	for i, is := range issues {
		parts[i] = is.message
		if is.path != "" {
			parts[i] += "\n  at " + is.path
		}
	}
	return strings.Join(parts, "\n")
}

func (c *rpcConn) request(m Value) {
	id := msgField(m, "id")
	notification := msgField(m, "isNotification") == true
	if !notification {
		switch id.(type) {
		case string, float64:
		default:
			c.defect("Invalid request id: " + rpcJSText(id))
			return
		}
	}
	tag, _ := msgField(m, "tag").(string)
	p := c.srv.procs[tag]
	if p == nil {
		if notification {
			c.dropNotification("Unknown request tag: " + tag)
		} else {
			c.exit(binUnknownExitLayout, id, exitFailureTree(dieReason("Unknown request tag: "+tag)))
		}
		return
	}
	payload := msgField(m, "payload")
	if c.ser.binary {
		b, _ := payload.([]byte)
		tree, err := binDecode(p.layouts().payload, b)
		if err != nil {
			if notification {
				c.dropNotification(err.Error())
			} else {
				c.exit(p.layouts().exit, id, exitFailureTree(dieReason(err.Error())))
			}
			return
		}
		payload = tree
	}
	fb := &Fiber{state: newState(), env: c.srv.services}
	decoded, issues := decodeWire(fb, p.payload, payload)
	if issues != nil {
		if notification {
			c.dropNotification(issuesText(issues))
		} else {
			c.exit(p.layouts().exit, id, exitFailureTree(dieReason(issuesText(issues))))
		}
		return
	}
	headers := append(c.headers[:len(c.headers):len(c.headers)], asList(msgField(m, "headers"))...)
	// Validate untrusted header pairs before constructing the handler. A malformed
	// request is a local failed Exit, rather than a client-wide fatal Defect.
	if _, cause := catch(func() Value { return HeadersFromInput(headers) }); cause != nil {
		if notification {
			c.dropNotification("Invalid request headers")
		} else {
			c.exit(p.layouts().exit, id, exitFailureTree(dieReason("Invalid request headers")))
		}
		return
	}

	call := &rpcCall{state: newState(), silent: notification, discard: !notification && msgField(m, "discard") == true}
	if p.stream && c.acks {
		call.ack = make(chan struct{}, 1)
	}
	key := idKey(id)
	c.mu.Lock()
	if notification {
		// JSON-RPC notifications all arrive without an id. Their internal key
		// must still be unique while their handlers overlap.
		for {
			key = idKey("@effect/rpc/notification/" + strconv.FormatUint(c.notifications, 10))
			c.notifications++
			if _, exists := c.calls[key]; !exists {
				break
			}
		}
	}
	call.key = key
	if _, dup := c.calls[key]; dup || c.ended {
		c.mu.Unlock()
		return
	}
	c.calls[key] = call
	c.srv.calls.Add(1)
	c.mu.Unlock()
	concurrent.Store(true)
	go c.run(call, p, id, decoded, headers, m)
}

// rpcJSText is a value as JS's template literal writes it.
func rpcJSText(v Value) string {
	switch x := v.(type) {
	case nil:
		return "undefined"
	case string:
		return x
	case *Record:
		return "[object Object]"
	}
	return JsonStringify(v).(string)
}

// run runs the call in its own fiber and sends its exit.
func (c *rpcConn) run(call *rpcCall, p *rpcProc, id, payload Value, headers []Value, m Value) {
	defer c.srv.calls.Done()
	fb := &Fiber{state: call.state, env: c.srv.services}
	var parent Value = None
	if spanID, ok := msgField(m, "spanId").(string); ok {
		traceID, _ := msgField(m, "traceId").(string)
		sampled, _ := msgField(m, "sampled").(bool)
		parent = Some(&span{name: None, spanId: spanID, traceId: traceID, sampled: sampled, parent: None, external: true})
	}
	fb.env = fb.env.with(parentSpanKey, openSpan(fb, "RpcServer."+p.name, Rec("parent", parent, "root", false)))
	body := func() Value {
		return owned(fb, func(fb *Fiber, _ *Owner) Value { return c.serve(fb, call, p, id, payload, headers) })
	}
	var out Value
	var cause *Cause
	if p.uninterruptible {
		out, cause = catch(func() Value { return masked(fb, 1, body) })
	} else {
		out, cause = catch(body)
	}
	if !call.responded && !call.silent {
		if call.discard && cause != nil {
			c.exit(p.layouts().exit, id, c.failureTree(fb, p, cause))
		} else {
			c.respond(fb, p, id, out, cause)
		}
	}
	if cause != nil && (call.silent || call.discard) {
		logMessage(fb, "Error", causePretty(cause))
	}
	c.mu.Lock()
	delete(c.calls, call.key)
	idle := c.ended && len(c.calls) == 0
	c.mu.Unlock()
	if idle {
		c.finish()
	}
}

// rpcInvoke runs the hooks, the check and the handler of a call: what the
// handler gave, and the fiber with the service the check made.
func rpcInvoke(fb *Fiber, p *rpcProc, handler, payload Value, headers []Value) (Value, *Fiber) {
	return rpcInvokeStart(fb, p, handler, payload, headers, nil)
}

func rpcInvokeStart(fb *Fiber, p *rpcProc, handler, payload Value, headers []Value, start func()) (Value, *Fiber) {
	seen := Rec("payload", payload, "headers", HeadersFromInput(headers))
	for _, hook := range p.middleware {
		runMaybe(fb, Apply(fb, hook, seen))
	}
	if p.authTag != "" {
		impl := runMaybe(fb, Apply(fb, p.authCheck, seen))
		inner := *fb
		inner.env = fb.env.with(p.authTag, impl)
		fb = &inner
	}
	result := Apply(fb, handler, payload)
	if start != nil {
		start()
	}
	return runMaybe(fb, result), fb
}

// serve runs a call, sending a stream's chunks as they come.
func (c *rpcConn) serve(fb *Fiber, call *rpcCall, p *rpcProc, id, payload Value, headers []Value) Value {
	result, fb := rpcInvokeStart(fb, p, c.srv.handlers[p.name], payload, headers, func() {
		if !call.discard {
			return
		}
		layout := *p.layouts().exit
		layout.value = binLeaf(binUndefined)
		c.mu.Lock()
		call.silent = true
		c.mu.Unlock()
		c.exit(&layout, id, exitSuccessTree(JsonNull))
	})
	if !p.stream {
		return result
	}
	runStream(fb, streamOf(result), func(chunk []Value) {
		if call.silent {
			return
		}
		values := make([]Value, len(chunk))
		for i, x := range chunk {
			v, issues := encodeWire(fb, p.success, x)
			if issues != nil {
				c.failedEncode(call, p, id, issues)
			}
			values[i] = v
		}
		hole, err := c.hole(p.layouts().chunk, values)
		if err != nil {
			c.failedEncode(call, p, id, []issue{{"", err.Error()}})
		}
		c.send(chunkMsg(id, hole))
		if call.ack != nil {
			wait(fb, call.ack)
		}
	})
	return Unit
}

// failedEncode answers the call with the defect of a value its codecs
// cannot write, and ends it.
func (c *rpcConn) failedEncode(call *rpcCall, p *rpcProc, id Value, issues []issue) {
	message := `Failed to encode response for RPC "` + p.name + `": ` + issuesText(issues)
	c.exit(p.layouts().exit, id, exitFailureTree(dieReason(message)))
	call.responded = true
	panic(Defect{message})
}

// respond sends the exit of a call: a defect without interruption is
// fatal, a Defect message; a typed error goes with the first case whose
// codec has it.
func (c *rpcConn) respond(fb *Fiber, p *rpcProc, id, out Value, cause *Cause) {
	if cause == nil {
		var tree Value = JsonNull
		var issues []issue
		if !p.stream {
			tree, issues = encodeWire(fb, p.success, out)
		}
		if issues != nil {
			c.exit(p.layouts().exit, id, exitFailureTree(dieReason(`Failed to encode response for RPC "`+p.name+`": `+issuesText(issues))))
			return
		}
		c.exit(p.layouts().exit, id, exitSuccessTree(tree))
		return
	}
	if cause.has(causeDie) && !cause.has(causeInterrupt) {
		c.defect(defectTree(cause.firstOf(causeDie).defect))
		return
	}
	c.exit(p.layouts().exit, id, c.failureTree(fb, p, cause))
}

// encodeError is the error as the first case whose codec has it writes it.
func encodeError(fb *Fiber, p *rpcProc, err Value) (Value, bool) {
	for _, e := range p.errors {
		if codecHas(fb, e, err) {
			tree, issues := e.encode(fb, err)
			return tree, issues == nil
		}
	}
	return nil, false
}

// failureTree keeps a pre-ack discard failure local to its request.
func (c *rpcConn) failureTree(fb *Fiber, p *rpcProc, cause *Cause) Value {
	var reasons []Value
	for _, r := range cause.reasons() {
		switch r.kind {
		case causeFail:
			tree, ok := encodeError(fb, p, r.err)
			if !ok {
				message := `Failed to encode response for RPC "` + p.name + `": no error case has ` + describe(r.err)
				return exitFailureTree(dieReason(message))
			}
			reasons = append(reasons, failReason(tree))
		case causeDie:
			reasons = append(reasons, dieReason(defectTree(r.defect)))
		case causeInterrupt:
			reasons = append(reasons, interruptReason(r.fiber))
		}
	}
	return exitFailureTree(reasons...)
}

// A notification has no response in which to carry an input error.
func (c *rpcConn) dropNotification(reason string) {
	logMessage(&Fiber{state: newState(), env: c.srv.services}, "Debug", "Dropped RPC notification: "+reason)
}
