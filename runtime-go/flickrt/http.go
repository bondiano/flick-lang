package flickrt

import (
	"context"
	"errors"
	"io"
	"net"
	"net/http"
	"strconv"
	"strings"
	"time"
)

// An HTTP server on net/http, as the JS runtime's on Node: each request
// runs its handler in a fiber of its own, with the services of the `serve`
// call; a handler's failure answers with its status (400 for a request it
// cannot parse, 404 for no route, 500 otherwise); the pre-response
// handlers registered for the request change the response before it is
// sent; closing the scope of the server stops accepting connections and
// waits for the running requests.
//
// shortcut: net/http sends the standard reason phrase of a status, not a response's status text; write the status line by hand if a program needs its own

var (
	serveErrorTag  = Tag("Http.ServerError.ServeError.ServeError")
	serverErrorTag = Tag("Http.ServerError.HttpServerError.HttpServerError")
	parseErrorTag  = Tag("Http.ServerError.ServerErrorReason.RequestParseError")
	tcpAddressTag  = Tag("Http.Server.Address.TcpAddress")
)

// servedRequest is a request with the fiber its handler runs in.
type servedRequest struct {
	state   fiberState
	fiber   Fiber
	source  requestSource
	marks   requestMarks
	request ServerRequest
}

func (q *servedRequest) interrupt() { q.state.interrupt(0) }

// HttpServer is a server listening in the owner of a scope.
type HttpServer struct {
	srv      *http.Server
	listener net.Listener
}

func HttpServe(fb *Fiber, options, handler Value) Value {
	o := options.(*Record)
	host, port := o.Get("host").(string), int(o.Get("port").(float64))
	listener, err := net.Listen("tcp", net.JoinHostPort(host, strconv.Itoa(port)))
	if err != nil {
		return Fail(NewData(serveErrorTag, err.Error()))
	}
	concurrent.Store(true)
	serving := *fb
	srv := &http.Server{Handler: http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		// one allocation for what a request's fiber needs
		q := &servedRequest{fiber: serving}
		q.source.r, q.source.marks, q.source.head = r, &q.marks, requestHead(r)
		q.request = ServerRequest{src: &q.source, method: r.Method, url: r.RequestURI, originalURL: r.RequestURI}
		q.state.id, q.state.signal = fibers.Add(1), make(chan struct{})
		q.fiber.state = &q.state
		stop := context.AfterFunc(r.Context(), q.interrupt)
		out, cause := catch(func() Value { return Run(&q.fiber, Apply(&q.fiber, handler, &q.request)) })
		if cause != nil {
			out = causeResponse(cause)
		}
		q.marks.mu.Lock()
		pre := len(q.marks.preResponse) > 0
		q.marks.mu.Unlock()
		if pre {
			if sent, failed := catch(func() Value { return q.request.preResponded(&q.fiber, out) }); failed == nil {
				out = sent
			} else {
				out = causeResponse(failed)
			}
		}
		// a streamed body runs in the request's fiber, which a client gone interrupts
		aborted := writeResponse(w, out.(*ServerResponse), &q.fiber)
		stop()
		if aborted {
			panic(http.ErrAbortHandler)
		}
	}), ConnContext: orderContext}
	go srv.Serve(orderListener{listener})
	var grace time.Duration
	if d, ok := some(o.Get("gracefulShutdownTimeout")); ok && !dur(d).infinite() {
		grace = time.Duration(dur(d).toNanos().Int64())
	}
	fb.owner.add(fb, func(*Fiber, *Cause) Value {
		ctx := context.Background()
		if grace > 0 {
			var cancel context.CancelFunc
			ctx, cancel = context.WithTimeout(ctx, grace)
			defer cancel()
		}
		if err := srv.Shutdown(ctx); err != nil && !errors.Is(err, context.DeadlineExceeded) {
			panic(Defect{err.Error()})
		}
		srv.Close()
		return Unit
	})
	return &HttpServer{srv, listener}
}

func HttpAddress(server Value) Value {
	addr := server.(*HttpServer).listener.Addr().(*net.TCPAddr)
	return NewData(tcpAddressTag, addr.IP.String(), float64(addr.Port))
}

// HttpServerLogAddress logs `Listening on <url>` at the info level.
func HttpServerLogAddress(fb *Fiber, server Value) Value {
	addr := server.(*HttpServer).listener.Addr().(*net.TCPAddr)
	host := addr.IP.String()
	if strings.Contains(host, ":") {
		host = "[" + host + "]"
	}
	logMessage(fb, "Info", "Listening on http://"+host+":"+strconv.Itoa(addr.Port))
	return Unit
}

// writeResponse sends the response: its headers, a `Set-Cookie` header of
// each cookie, and the body unless its status has none; whether a
// streamed body failed after its headers went, which aborts the response.
func writeResponse(w http.ResponseWriter, resp *ServerResponse, fb *Fiber) bool {
	h, b := w.Header(), resp.body
	if resp.headers == nil {
		if b.hasType && b.contentType != "" {
			h["Content-Type"] = []string{b.contentType}
		}
		if b.length >= 0 {
			h["Content-Length"] = []string{strconv.FormatInt(b.length, 10)}
		}
	} else {
		for i, k := range resp.headers.keys {
			h.Set(k, resp.headers.vals[i].(string))
		}
	}
	for _, c := range resp.cookies.vals {
		h.Add("Set-Cookie", cookieOf(c).serialize())
	}
	status := int(resp.status)
	if status < 100 || status > 999 {
		status = 500
	}
	switch {
	case resp.omitsBody(false):
		w.WriteHeader(status)
	case b.kind == bodyBytes:
		// a text body is written as it is, without a copy
		w.WriteHeader(status)
		if b.text {
			io.WriteString(w, wellFormed(b.data))
		} else {
			io.WriteString(w, b.data)
		}
	case b.kind == bodyStream:
		return writeStream(w, status, b, fb)
	default:
		data, contentType, err := b.bytes(fb)
		if err != nil {
			status, data = 500, nil
		} else if b.kind == bodyForm && h.Get("Content-Type") == "" {
			h.Set("Content-Type", contentType)
		}
		w.WriteHeader(status)
		w.Write(data)
	}
	return false
}

// writeStream sends each chunk of the body as it is read.
func writeStream(w http.ResponseWriter, status int, b *Body, fb *Fiber) bool {
	r, err := b.open(fb)
	if err != nil {
		w.WriteHeader(500)
		return false
	}
	defer r.Close()
	w.WriteHeader(status)
	flusher, _ := w.(http.Flusher)
	buf := make([]byte, 32<<10)
	for {
		n, err := r.Read(buf)
		if n > 0 {
			if _, werr := w.Write(buf[:n]); werr != nil {
				return false
			}
			if flusher != nil {
				flusher.Flush()
			}
		}
		if err == io.EOF {
			return false
		}
		if err != nil {
			return true
		}
	}
}
