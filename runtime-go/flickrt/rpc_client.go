package flickrt

import (
	"io"
	"sync/atomic"
)

// The client of Effect's RpcClient: a request carries an id of the
// process's counter, the payload its codec writes, the headers of
// `with_headers` and the trace of its span; its exit comes back coded by
// the procedure's codecs. Over HTTP a call is a POST whose response holds
// its answers; over a socket (rpc_socket.go) calls share the connection.
// A failure of the client is a `ClientError` with Effect's message.

var rpcRequestIds atomic.Int64

const rpcHeadersKey = "effect/rpc/RpcClient/CurrentHeaders"

// RpcWithHeaders runs `task` with its calls carrying the headers too.
func RpcWithHeaders(fb *Fiber, input, task Value) Value {
	return withEnv(fb, rpcHeadersKey, HeadersMerge(HeadersFromInput(input), currentRPCHeaders(fb)), task)
}

func currentRPCHeaders(fb *Fiber) Value {
	if h, ok := fb.env.get(rpcHeadersKey); ok {
		return h
	}
	return HeadersEmpty
}

// headerPairs are headers as Effect's `Object.entries`.
func headerPairs(h *Record) []Value {
	out := make([]Value, len(h.keys))
	for i, k := range h.keys {
		out[i] = []Value{k, h.vals[i]}
	}
	return out
}

// hole is a hole of a message as the serialization writes it; unhole reads one.
func hole(ser *rpcSerialization, n *binNode, tree Value) Value {
	if !ser.binary {
		return tree
	}
	b, err := binEncode(n, tree)
	if err != nil {
		panic(Defect{err.Error()})
	}
	return b
}

func unhole(ser *rpcSerialization, n *binNode, v Value) Value {
	if !ser.binary {
		return v
	}
	b, _ := v.([]byte)
	tree, err := binDecode(n, b)
	if err != nil {
		panic(Defect{err.Error()})
	}
	return tree
}

// request is the message of a call and the span it is sent in.
func rpcRequest(fb *Fiber, ser *rpcSerialization, p *rpcProc, payload Value) (Value, *span) {
	tree, issues := encodeWire(fb, p.payload, payload)
	if issues != nil {
		panic(Defect{issuesText(issues)})
	}
	s := openSpan(fb, "RpcClient."+p.name, Rec("parent", None, "root", false))
	id := float64(rpcRequestIds.Add(1) - 1)
	m := Rec("_tag", "Request", "id", id, "tag", p.name, "payload", hole(ser, p.layouts().payload, tree),
		"traceId", s.traceId, "spanId", s.spanId, "sampled", true,
		"headers", headerPairs(headersOf(currentRPCHeaders(fb))))
	return m, s
}

// rpcResponses are the messages the server sends a call.
type rpcResponses interface {
	// next is the next message; it fails with the ClientError of a broken
	// transport.
	next(fb *Fiber) Value
	// stop gives up on the call before its exit: the server interrupts it.
	stop()
}

func isTerminal(m Value) bool {
	t := msgTag(m)
	return t == "Exit" || t == "Defect" || t == "ClientProtocolError"
}

// rpcClient is `Rpc.Client s`, and the client of a single call.
type rpcClient struct {
	ser  *rpcSerialization
	url  string
	sock *rpcSocketClient // nil over HTTP
}

func newRPCClient(options Value, url string) *rpcClient {
	o := record(options)
	c := &rpcClient{ser: serializationOf(o.Get("serialization")), url: url}
	if t := o.Get("transport").(rpcTransport); t != "http" {
		c.sock = &rpcSocketClient{ser: c.ser, transport: t, url: url, calls: map[string]*rpcInflight{}}
	}
	return c
}

func (c *rpcClient) close() {
	if c.sock != nil {
		c.sock.close()
	}
}

func (c *rpcClient) open(fb *Fiber, m Value) rpcResponses {
	if c.sock != nil {
		return c.sock.start(fb, m)
	}
	return rpcPost(fb, c.ser, c.url, m)
}

// call is the success of a unary call, or its failure.
func (c *rpcClient) call(fb *Fiber, p *rpcProc, payload Value) Value {
	m, s := rpcRequest(fb, c.ser, p, payload)
	inner := *fb
	inner.env = fb.env.with(parentSpanKey, s)
	responses := c.open(&inner, m)
	defer responses.stop()
	for {
		r := responses.next(&inner)
		if isTerminal(r) {
			return c.exit(fb, p, r)
		}
	}
}

// exit is the success of a terminal message, or the failure it carries.
func (c *rpcClient) exit(fb *Fiber, p *rpcProc, m Value) Value {
	switch msgTag(m) {
	case "ClientProtocolError":
		return Fail(msgField(m, "error"))
	case "Defect":
		panic(&Failure{&Cause{kind: causeDie, defect: wireDefectText(unhole(c.ser, binDefectLayout, msgField(m, "defect")))}})
	}
	tree := unhole(c.ser, p.layouts().exit, msgField(m, "exit"))
	if msgField(tree, "_tag") == "Success" {
		if p.stream {
			return Unit
		}
		v, issues := decodeWire(fb, p.success, msgField(tree, "value"))
		if issues != nil {
			panic(Defect{issuesText(issues)})
		}
		return v
	}
	var cause *Cause
	for _, r := range asList(msgField(tree, "cause")) {
		switch msgTag(r) {
		case "Fail":
			cause = Sequential(cause, &Cause{kind: causeFail, err: decodeError(fb, p, msgField(r, "error"))})
		case "Die":
			cause = Sequential(cause, &Cause{kind: causeDie, defect: wireDefectText(msgField(r, "defect"))})
		case "Interrupt":
			id, _ := msgField(r, "fiberId").(float64)
			cause = Sequential(cause, &Cause{kind: causeInterrupt, fiber: int64(id)})
		}
	}
	if cause == nil {
		cause = &Cause{kind: causeDie, defect: "an exit of an empty cause"}
	}
	panic(&Failure{cause})
}

// decodeError is the error the first case that reads it gives.
func decodeError(fb *Fiber, p *rpcProc, tree Value) Value {
	var all []issue
	for _, e := range p.errors {
		v, issues := e.decode(fb, tree)
		if issues == nil {
			return v
		}
		all = append(all, issues...)
	}
	if len(all) == 0 {
		all = fail("Expected never, got " + describeInput(tree))
	}
	panic(Defect{issuesText(all)})
}

// stream is the stream of a call's chunks, the call made when it is run.
func (c *rpcClient) stream(p *rpcProc, payload Value, after func()) *Stream {
	return streamOpen(func(fb *Fiber) pull {
		m, s := rpcRequest(fb, c.ser, p, payload)
		inner := *fb
		inner.env = fb.env.with(parentSpanKey, s)
		responses := c.open(&inner, m)
		fb.owner.add(fb, func(*Fiber, *Cause) Value {
			responses.stop()
			if after != nil {
				after()
			}
			return Unit
		})
		done := false
		return func(fb *Fiber) ([]Value, bool) {
			for !done {
				r := responses.next(fb)
				if isTerminal(r) {
					done = true
					c.exit(fb, p, r)
					break
				}
				if msgTag(r) != "Chunk" {
					continue
				}
				tree := asList(unhole(c.ser, p.layouts().chunk, msgField(r, "values")))
				values := make([]Value, len(tree))
				for i, x := range tree {
					v, issues := decodeWire(fb, p.success, x)
					if issues != nil {
						panic(Defect{issuesText(issues)})
					}
					values[i] = v
				}
				if len(values) > 0 {
					return values, true
				}
			}
			return nil, false
		}
	})
}

var callDefaults = Rec("serialization", RpcNdjson, "transport", RpcHttp)

// RpcCall is `Rpc.call_with`: a client of its own for the call.
func RpcCall(fb *Fiber, options, url, proc, payload Value) Value {
	c := newRPCClient(options, url.(string))
	defer c.close()
	return c.call(fb, rpcProcOf(proc), payload)
}

func RpcCallNdjson(fb *Fiber, url, proc, payload Value) Value {
	return RpcCall(fb, callDefaults, url, proc, payload)
}

func RpcCallStream(options, url, proc, payload Value) Value {
	c := newRPCClient(options, url.(string))
	return c.stream(rpcProcOf(proc), payload, c.close)
}

func RpcCallStreamNdjson(url, proc, payload Value) Value {
	return RpcCallStream(callDefaults, url, proc, payload)
}

// RpcConnect is a client whose socket, opened on its first call, the
// innermost owner closes.
func RpcConnect(fb *Fiber, options, url Value) Value {
	c := newRPCClient(options, url.(string))
	if c.sock != nil {
		c.sock.reconnect = make(chan struct{}, 1)
		worker := fork(fb, fb.owner, Eff(c.sock.reconnectLoop), nil)
		fb.owner.onClose(fb, func(fb *Fiber) { c.close(); stop(fb, worker) })
	} else {
		fb.owner.onClose(fb, func(*Fiber) { c.close() })
	}
	return c
}

func RpcCallOn(fb *Fiber, client, proc, payload Value) Value {
	return client.(*rpcClient).call(fb, rpcProcOf(proc), payload)
}

func RpcCallStreamOn(client, proc, payload Value) Value {
	return client.(*rpcClient).stream(rpcProcOf(proc), payload, nil)
}

// httpResponses are the answers of a POST, read as they come with
// framing, at once without.
type httpResponses struct {
	ser    *rpcSerialization
	parser rpcParser
	body   io.ReadCloser // nil once read to its end
	buf    []byte
	queue  []Value
	seen   bool
	ended  bool
}

func clientDefect(message string) Value { return Fail(rpcClientError("RpcClientDefect: " + message)) }

// rpcPost sends the request, failing with the ClientError of a POST that
// cannot be made.
func rpcPost(fb *Fiber, ser *rpcSerialization, url string, m Value) rpcResponses {
	parser := ser.parser()
	data := string(parser.encode([]Value{m}, false))
	body := bytesBody(data, ser.contentType)
	if !ser.binary {
		body = textBody(data, ser.contentType)
	}
	req := ClientRequestSetBody(body, ClientRequestPrependUrl(url, ClientRequestPost("")))
	resp, c := catch(func() Value { return Run(fb, clientOfTransport(fetchTransport).execute(fb, req)) })
	if c != nil {
		if _, ok := typedFailure(c); ok {
			Fail(rpcClientError("HttpError: "))
		}
		panic(&Failure{c})
	}
	r, err := clientResponseOf(resp).open(fb)
	if err != nil {
		Fail(rpcClientError("HttpError: " + err.Error()))
	}
	return &httpResponses{ser: ser, parser: parser, body: r, buf: make([]byte, 32<<10)}
}

func (h *httpResponses) next(fb *Fiber) Value {
	for len(h.queue) == 0 {
		if h.body == nil {
			if !h.seen {
				clientDefect("Received empty HTTP response from RPC server")
			}
			clientDefect("HTTP response ended before RPC request completed")
		}
		if !h.ser.framed {
			data, err := io.ReadAll(h.body)
			h.body.Close()
			h.body = nil
			if err != nil {
				Fail(rpcClientError("HttpError: " + err.Error()))
			}
			msgs, err := h.parser.decode(data)
			if err != nil {
				clientDefect("Error decoding HTTP response")
			}
			h.queue = msgs
			continue
		}
		n, err := h.body.Read(h.buf)
		if n > 0 {
			msgs, derr := h.parser.decode(h.buf[:n])
			if derr != nil {
				clientDefect("Error decoding HTTP response")
			}
			h.queue = append(h.queue, msgs...)
		}
		if err != nil {
			h.body.Close()
			h.body = nil
			if err != io.EOF {
				rethrow(err)
				Fail(rpcClientError("HttpError: " + err.Error()))
			}
		}
	}
	m := h.queue[0]
	h.queue = h.queue[1:]
	h.seen = true
	h.ended = h.ended || isTerminal(m)
	return m
}

// stop closes the response: the server sees the client go.
func (h *httpResponses) stop() {
	if h.body != nil {
		h.body.Close()
		h.body = nil
	}
}
