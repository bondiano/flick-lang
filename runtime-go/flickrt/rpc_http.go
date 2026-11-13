package flickrt

import (
	"errors"
	"io"
	"strings"
	"sync"
)

// The servers of a group over HTTP, as Effect's RpcServer serves them: a
// POST per call, its body the messages of the call and the response the
// answers (one document for a serialization without framing, the
// messages as they come for one with it), a dropped connection
// interrupting the calls; or a WebSocket per client.

type rpcListening struct{ port float64 }

func RpcPort(s Value) Value { return s.(*rpcListening).port }

// httpOut is what a POST answers, gathered as its calls send it.
type httpOut struct {
	mu     sync.Mutex
	parser rpcParser
	framed bool
	msgs   []Value       // without framing: the messages, written together at the end
	buf    []byte        // with framing: what is written and not yet sent
	ready  chan struct{} // a message that does not end a call was sent
	more   chan struct{} // a message was sent
}

func notify(ch chan struct{}) {
	select {
	case ch <- struct{}{}:
	default:
	}
}

func (o *httpOut) push(m Value) {
	o.mu.Lock()
	if o.framed {
		o.buf = append(o.buf, o.parser.encode([]Value{m}, false)...)
	} else {
		o.msgs = append(o.msgs, m)
	}
	o.mu.Unlock()
	notify(o.more)
	if t := msgTag(m); t != "Exit" && t != "Defect" {
		notify(o.ready)
	}
}

func closed(ch <-chan struct{}) bool {
	select {
	case <-ch:
		return true
	default:
		return false
	}
}

var errClientGone = errors.New("the client went away")

// httpOutReader is the body of a framed answer, read in the request's
// fiber: what the calls sent, until the last ended.
type httpOutReader struct {
	out  *httpOut
	conn *rpcConn
	fb   *Fiber
}

func (r *httpOutReader) Read(p []byte) (int, error) {
	for {
		done := closed(r.conn.done)
		r.out.mu.Lock()
		if len(r.out.buf) > 0 {
			n := copy(p, r.out.buf)
			r.out.buf = r.out.buf[n:]
			r.out.mu.Unlock()
			return n, nil
		}
		r.out.mu.Unlock()
		if done {
			return 0, io.EOF
		}
		select {
		case <-r.out.more:
		case <-r.conn.done:
		case <-r.fb.state.signal:
			return 0, errClientGone
		}
	}
}

func (r *httpOutReader) Close() error {
	r.conn.abort()
	return nil
}

func (s *rpcServer) httpHandler(ser *rpcSerialization) Value {
	return Fn(func(fb *Fiber, r Value) Value {
		return Eff(func(fb *Fiber) Value { return s.serveHTTP(fb, ser, requestOf(r)) })
	})
}

func (s *rpcServer) serveHTTP(fb *Fiber, ser *rpcSerialization, q *ServerRequest) Value {
	data, c := catch(func() Value { return q.bytes(fb) })
	if c != nil {
		// Effect reads the body or dies
		panic(&Failure{c.failsToDies()})
	}
	out := &httpOut{parser: ser.parser(), framed: ser.framed, ready: make(chan struct{}, 1), more: make(chan struct{}, 1)}
	conn := s.connect(ser, headerPairs(q.headerRecord()), false, out.push, nil)
	msgs, err := out.parser.decode(data.([]byte))
	if err != nil {
		conn.defect(errorDefect(err))
	}
	for _, m := range msgs {
		conn.receive(m)
	}
	conn.receive(eofMsg)
	if _, c := catch(func() Value { out.await(fb, conn.done); return nil }); c != nil {
		conn.abort()
		panic(&Failure{c})
	}
	if !ser.framed {
		text := ""
		if b := out.parser.encode(out.msgs, true); b != nil {
			text = string(b)
		}
		return bodyResponse(noOptions, 200, nil, *textBody(text, ser.contentType))
	}
	if closed(conn.done) {
		out.mu.Lock()
		data := string(out.buf)
		out.mu.Unlock()
		return bodyResponse(noOptions, 200, nil, *bytesBody(data, ser.contentType))
	}
	body := Body{kind: bodyStream, hasType: true, contentType: ser.contentType, length: -1,
		open: func(fb *Fiber) (io.ReadCloser, error) { return &httpOutReader{out, conn, fb}, nil }}
	return bodyResponse(noOptions, 200, nil, body)
}

// await waits for the client to end or, with framing, for a chunk of a
// stream: an answer whose calls all ended by then is sent whole, as
// Effect sends a call that answers at once.
func (o *httpOut) await(fb *Fiber, done <-chan struct{}) {
	if !o.framed {
		wait(fb, done)
		return
	}
	idle()
	select {
	case <-o.ready:
	case <-done:
	case <-fb.state.signal:
		if fb.state.mask == 0 {
			panic(fb.state.stopped())
		}
		<-done
	}
}

// rpcPath is a route's path, which starts with `/`.
func rpcPath(path string) string {
	if strings.HasPrefix(path, "/") {
		return path
	}
	return "/" + path
}

// rpcListen serves the group at the options' path, a POST per call or a
// WebSocket per client, until the innermost owner closes.
func rpcListen(fb *Fiber, websocket bool, options, group, handlers Value) Value {
	o := record(options)
	ser := serializationOf(o.Get("serialization"))
	srv := newRPCServer(fb, groupOf(group), rpcHandlers(groupOf(group), handlers))
	method, serve := "POST", srv.httpHandler(ser)
	if websocket {
		method, serve = "GET", srv.websocketHandler(ser)
	}
	routes := []Value{newRoute(method, rpcPath(o.Get("path").(string)), "", false, serve)}
	router := Fn(func(fb *Fiber, r Value) Value {
		return Eff(func(fb *Fiber) Value { return HttpRouter(fb, apiRouterConfig, routes, r) })
	})
	options = Rec("host", o.Get("host"), "port", o.Get("port"), "gracefulShutdownTimeout", None)
	server, c := catch(func() Value { return HttpServe(fb, options, router) })
	if c != nil {
		if err, ok := typedFailure(c); ok && err.(*Data).Tag == serveErrorTag {
			return Fail(rpcServeError(err.(*Data).Fields()[0].(string)))
		}
		panic(&Failure{c})
	}
	return &rpcListening{HttpAddress(server).(*Data).Fields()[1].(float64)}
}

func RpcListen(fb *Fiber, options, group, handlers Value) Value {
	return rpcListen(fb, false, options, group, handlers)
}

func RpcListenWebsocket(fb *Fiber, options, group, handlers Value) Value {
	return rpcListen(fb, true, options, group, handlers)
}

// RpcHandler answers a request of the group's calls, a POST per call.
// The go target calls a foreign function with every stage at once, so it
// runs per request, with the services of the request's fiber: its calls
// end with the request, as the server lets requests finish, and a
// WebSocket of RpcHandlerWebsocket closes with the server.
//
// shortcut: the handlers run with the services where the server runs, not where `handler` ran; mount it once, as Server.handler does, when a program provides them between the two
func RpcHandler(fb *Fiber, ser, group, handlers, request Value) Value {
	g := groupOf(group)
	return makeRPCServer(fb.env, g, rpcHandlers(g, handlers)).serveHTTP(fb, serializationOf(ser), requestOf(request))
}

func RpcHandlerWebsocket(fb *Fiber, ser, group, handlers, request Value) Value {
	g := groupOf(group)
	return makeRPCServer(fb.env, g, rpcHandlers(g, handlers)).websocket(serializationOf(ser), requestOf(request))
}
