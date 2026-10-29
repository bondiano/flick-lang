package flickrt

import (
	"context"
	"errors"
	"io"
	"net"
	"net/http"
	"strconv"
	"strings"
	"sync"
	"time"
)

// An HTTP server on net/http, as the JS runtime's on Node: each request
// runs its handler in a fiber of its own, with the services of the `serve`
// call; a handler's failure answers with its status (400 for a request it
// cannot parse, 404 for no route, 500 otherwise); closing the scope of the
// server stops accepting connections and waits for the running requests.
//
// shortcut: responses carry a status, a content type and a body; headers, cookies and status texts come with the rest of the Http modules

var (
	serveErrorTag   = Tag("Http.ServerError.ServeError.ServeError")
	serverErrorTag  = Tag("Http.ServerError.HttpServerError.HttpServerError")
	parseErrorTag   = Tag("Http.ServerError.ServerErrorReason.RequestParseError")
	routeMissingTag = Tag("Http.ServerError.ServerErrorReason.RouteNotFound")
	tcpAddressTag   = Tag("Http.Server.Address.TcpAddress")
	routeContextKey = "Http.Router.RouteContext"
)

// ServerRequest is a request a handler reads; its body is read once.
type ServerRequest struct {
	r    *http.Request
	once sync.Once
	body []byte
	err  error
}

func (q *ServerRequest) read() ([]byte, error) {
	q.once.Do(func() { q.body, q.err = io.ReadAll(q.r.Body) })
	return q.body, q.err
}

// ServerResponse is what a handler answers.
type ServerResponse struct {
	status      int
	contentType string
	body        []byte
}

// response is a response of the options, `status` unless they set one.
func response(options Value, status int, contentType string, body []byte) Value {
	o := options.(*Record)
	if s, ok := some(o.Get("status")); ok {
		status = int(s.(int32))
	}
	if t, ok := some(o.Get("contentType")); ok {
		contentType = t.(string)
	}
	return &ServerResponse{status, contentType, body}
}

func ResponseText(text, options Value) Value {
	return response(options, 200, "text/plain", []byte(wellFormed(text.(string))))
}

func ResponseEmpty(options Value) Value { return response(options, 204, "", nil) }

func ResponseJson(_ *Fiber, json, options Value) Value {
	return response(options, 200, "application/json", []byte(JsonStringify(json).(string)))
}

// ResponseSchemaJson is the JSON of a value of a codec.
func ResponseSchemaJson(_ *Fiber, c, v, options Value) Value {
	e, issues := codecOf(c).encode(v)
	if issues != nil {
		return Fail(schemaErrorOf(issues))
	}
	return ResponseJson(nil, e, options)
}

func methodOf(r *http.Request) Value { return NewData(Tag("Http.Method.Method." + r.Method)) }

// RequestSchemaBodyJson decodes the JSON of the body by a codec; a body
// that is not JSON is a request the server cannot parse.
func RequestSchemaBodyJson(_ *Fiber, c, request Value) Value {
	q := request.(*ServerRequest)
	body, err := q.read()
	parsed := JsonParse(string(body))
	if err != nil || !isOk(parsed) {
		description := "the body is not JSON"
		if err != nil {
			description = err.Error()
		}
		details := Rec("method", methodOf(q.r), "url", q.r.URL.String(), "description", Some(description))
		return Fail(NewData(serverErrorTag, NewData(parseErrorTag, details)))
	}
	v, issues := codecOf(c).decode(value(parsed))
	if issues != nil {
		return Fail(schemaErrorOf(issues))
	}
	return v
}

// statusOf is the status a server answers a failure with.
func statusOf(c *Cause) int {
	if !c.has(causeFail) && !c.has(causeDie) {
		return 503
	}
	if r := c.firstOf(causeFail); r != nil && !c.has(causeDie) {
		if d, ok := r.err.(*Data); ok && d.Tag == serverErrorTag {
			switch d.Fields[0].(*Data).Tag {
			case parseErrorTag:
				return 400
			case routeMissingTag:
				return 404
			}
		}
	}
	return 500
}

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
		state := newState()
		go func() {
			select {
			case <-r.Context().Done():
				state.interrupt(0)
			case <-state.signal:
			}
		}()
		inner := serving
		inner.state = state
		out, cause := catch(func() Value { return Run(&inner, Apply(handler, &ServerRequest{r: r})) })
		state.interrupt(0) // ends the watch above
		if cause != nil {
			w.WriteHeader(statusOf(cause))
			return
		}
		resp := out.(*ServerResponse)
		if resp.contentType != "" {
			w.Header().Set("Content-Type", resp.contentType)
		}
		w.WriteHeader(resp.status)
		w.Write(resp.body)
	})}
	go srv.Serve(listener)
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

// A route of a router: its method (nil for every one), the segments of its
// path pattern and its handler.
type route struct {
	method   string
	segments []string
	handler  Value
}

func HttpRoute(method, path, handler Value) Value {
	m := ""
	if d, ok := some(method); ok {
		m = ConstructorName(d.(*Data).Tag)
	}
	return &route{m, strings.Split(path.(string), "/"), handler}
}

// segments are the parts of a path; duplicate slashes count as one.
func segments(path string) []string {
	parts := strings.Split(path, "/")
	out := parts[:1]
	for i, p := range parts[1:] {
		if p != "" || i == len(parts)-2 {
			out = append(out, p)
		}
	}
	return out
}

// match is the parameters of a path the route's pattern matches.
func (rt *route) match(method string, path []string) ([]Value, bool) {
	if rt.method != "" && rt.method != method {
		return nil, false
	}
	params := []Value{}
	for i, p := range rt.segments {
		if p == "*" {
			return append(params, []Value{"*", strings.Join(path[i:], "/")}), true
		}
		if i >= len(path) {
			return nil, false
		}
		switch {
		case strings.HasPrefix(p, ":"):
			if path[i] == "" {
				return nil, false
			}
			params = append(params, []Value{p[1:], path[i]})
		case p != path[i]:
			return nil, false
		}
	}
	return params, len(path) == len(rt.segments)
}

// HttpRouter handles a request by its routes: the first match with the
// fewest parameters, its parameters given as the service RouteContext.
//
// shortcut: Effect's defaults only, the options of the config are not read; read them when a program sets one
func HttpRouter(fb *Fiber, _, routes, request Value) Value {
	q := request.(*ServerRequest)
	path := segments(q.r.URL.Path)
	var best *route
	var params []Value
	for _, r := range List(routes) {
		rt := r.(*route)
		if p, ok := rt.match(q.r.Method, path); ok && (best == nil || len(p) < len(params)) {
			best, params = rt, p
		}
	}
	if best == nil {
		details := Rec("method", methodOf(q.r), "url", q.r.URL.String(), "description", None)
		return Fail(NewData(serverErrorTag, NewData(routeMissingTag, details)))
	}
	context := Rec("params", Fn(func(Value) Value { return Succeed(params) }))
	return withEnv(fb, routeContextKey, context, Apply(best.handler, request))
}

// The platform and entity tags of Node are services the go target does not
// read yet: their layers provide them empty.
func HttpEmptyLayer(key Value) Value { return LayerSucceed(key, &Record{}) }
