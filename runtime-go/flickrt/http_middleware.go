package flickrt

import "strings"

// Http.Middleware as Effect's HttpMiddleware and HttpEffect: each binding
// takes the handler and the request at once and answers it. What Effect
// keeps by a request's source (the logger turned off, the handlers to run
// on the response before it is sent) lives in the marks of its source.

const (
	tracerDisabledKey = "effect/http/HttpMiddleware/TracerDisabledWhen"
	spanNameKey       = "@effect/platform/HttpMiddleware/SpanNameGenerator"
	headerFilterKey   = "effect/http/HttpMiddleware/TracerHeaderFilter"
)

func handle(fb *Fiber, handler Value, request Value) Value {
	return Run(fb, Apply(fb, handler, request))
}

func (q *ServerRequest) appendPreResponse(h preResponse) {
	m := q.src.marks
	m.mu.Lock()
	m.preResponse = append(m.preResponse, h)
	m.mu.Unlock()
}

// preResponded is the response after the handlers registered for the
// request's source, in order.
func (q *ServerRequest) preResponded(fb *Fiber, response Value) Value {
	m := q.src.marks
	m.mu.Lock()
	hs := m.preResponse
	m.mu.Unlock()
	for _, h := range hs {
		response = h(fb, q, response)
	}
	return response
}

// flickPreResponse is a Flick function of the request and the response.
func flickPreResponse(f Value) preResponse {
	return func(fb *Fiber, request, response Value) Value {
		return Run(fb, Apply(fb, Apply(fb, f, request), response))
	}
}

func MiddlewareAppendPreResponseHandler(_ *Fiber, f, request Value) Value {
	requestOf(request).appendPreResponse(flickPreResponse(f))
	return Unit
}

func MiddlewareWithPreResponseHandler(fb *Fiber, f, handler, request Value) Value {
	requestOf(request).appendPreResponse(flickPreResponse(f))
	return handle(fb, handler, request)
}

// MiddlewareToHandled answers the request and passes the request and the
// response, after its pre-response handlers, to `send`; a failure is sent
// as the response of its cause, then fails the call.
func MiddlewareToHandled(fb *Fiber, send, handler, request Value) Value {
	q := requestOf(request)
	out, cause := catch(func() Value { return handle(fb, handler, request) })
	if cause != nil {
		out = causeResponse(cause)
	}
	Run(fb, Apply(fb, Apply(fb, send, request), q.preResponded(fb, out)))
	if cause != nil {
		panic(&Failure{cause})
	}
	return Unit
}

// MiddlewareLogger logs each answered request at the info level, unless
// `with_logger_disabled` turned the logger off for it.
func MiddlewareLogger(fb *Fiber, handler, request Value) Value {
	q := requestOf(request)
	out, cause := catch(func() Value { return handle(fb, handler, request) })
	m := q.src.marks
	m.mu.Lock()
	off := m.loggerOff
	m.mu.Unlock()
	if !off {
		message := "Sent HTTP response"
		if cause != nil && (cause.has(causeFail) || cause.has(causeDie)) {
			message = pretty(cause)
		}
		logMessage(fb, "Info", message)
	}
	if cause != nil {
		panic(&Failure{cause})
	}
	return out
}

func MiddlewareWithLoggerDisabled(fb *Fiber, handler, request Value) Value {
	m := requestOf(request).src.marks
	m.mu.Lock()
	m.loggerOff = true
	m.mu.Unlock()
	return handle(fb, handler, request)
}

// MiddlewareTracer runs the request in a server span named by the span
// name generator, the request's method by default.
//
// shortcut: the span does not continue the trace of the request's headers, as the go target's spans are not exported; read them when spans are
func MiddlewareTracer(fb *Fiber, handler, request Value) Value {
	if f, ok := fb.env.get(tracerDisabledKey); ok && Apply(fb, f, request).(bool) {
		return handle(fb, handler, request)
	}
	name := Value(requestOf(request).method)
	if f, ok := fb.env.get(spanNameKey); ok {
		name = Apply(fb, f, request)
	}
	span := openSpan(fb, name, Rec("parent", None, "root", false))
	return withEnv(fb, parentSpanKey, span, Apply(fb, handler, request))
}

func MiddlewareWithTracerDisabledWhen(fb *Fiber, f, task Value) Value {
	return withEnv(fb, tracerDisabledKey, f, task)
}

func MiddlewareWithTracerDisabledForUrls(fb *Fiber, urls, task Value) Value {
	disabled := Fn(func(fb *Fiber, r Value) Value {
		for _, u := range List(urls) {
			if u == requestOf(r).url {
				return true
			}
		}
		return false
	})
	return withEnv(fb, tracerDisabledKey, disabled, task)
}

func MiddlewareWithSpanNameGenerator(fb *Fiber, f, task Value) Value {
	return withEnv(fb, spanNameKey, f, task)
}

func MiddlewareWithTracerHeaderFilter(fb *Fiber, f, task Value) Value {
	return withEnv(fb, headerFilterKey, f, task)
}

// MiddlewareXForwardedHeaders takes the host and the client's address of
// the `X-Forwarded-*` headers, when a proxy sets the host.
func MiddlewareXForwardedHeaders(fb *Fiber, handler, request Value) Value {
	q := requestOf(request)
	if host, _ := q.header("x-forwarded-host"); host != "" {
		q = q.modified()
		q.headers = q.headerRecord().set("host", host)
		q.remote = None
		if forwarded, ok := q.header("x-forwarded-for"); ok {
			first, _, _ := strings.Cut(forwarded, ",")
			q.remote = Some(Trim(first))
		}
	}
	return handle(fb, handler, q)
}

// MiddlewareSearchParamsParser parses the query of the original URL, which
// only a full URL has, as Effect does; no Flick decoder reads the parse.
func MiddlewareSearchParamsParser(fb *Fiber, handler, request Value) Value {
	if _, ok := parseURL(requestOf(request).originalURL); !ok {
		panic(Defect{"TypeError: Invalid URL"})
	}
	return handle(fb, handler, request)
}

// MiddlewareCors answers preflight requests and adds the CORS headers to
// the responses of the others, as Effect's `cors` does.
func MiddlewareCors(fb *Fiber, options, handler, request Value) Value {
	o := record(options)
	origins := stringsOf(o.Get("allowedOrigins"))
	methods := stringsOf(o.Get("allowedMethods"))
	if len(methods) == 0 {
		methods = []string{"GET", "HEAD", "PUT", "PATCH", "POST", "DELETE"}
	}
	allowed := stringsOf(o.Get("allowedHeaders"))
	exposed := stringsOf(o.Get("exposedHeaders"))
	credentials := o.Get("credentials").(bool)
	common := func(q *ServerRequest) *Record {
		h := newRecord(4)
		origin, _ := q.header("origin")
		switch {
		case len(origins) > 1:
			for _, a := range origins {
				if a == origin {
					h.put("access-control-allow-origin", origin)
				}
			}
			h.put("vary", "Origin")
		case len(origins) == 0:
			h.put("access-control-allow-origin", "*")
		default:
			h.put("access-control-allow-origin", origins[0])
			h.put("vary", "Origin")
		}
		if credentials {
			h.put("access-control-allow-credentials", "true")
		}
		if len(exposed) > 0 {
			h.put("access-control-expose-headers", strings.Join(exposed, ","))
		}
		return h
	}
	q := requestOf(request)
	if q.method == "OPTIONS" {
		h := common(q)
		h.put("access-control-allow-methods", strings.Join(methods, ", "))
		if m, ok := some(o.Get("maxAge")); ok && m.(float64) != 0 {
			h.put("access-control-max-age", ShowNumber(m.(float64)))
		}
		requested, _ := q.header("access-control-request-headers")
		if len(allowed) == 0 && requested != "" {
			h = h.set("vary", varyWith(h, "Access-Control-Request-Headers")).
				set("access-control-allow-headers", requested)
		} else {
			h = h.set("access-control-allow-headers", strings.Join(allowed, ","))
		}
		return newResponse(204, "", false, h, nil, nil, false)
	}
	q.appendPreResponse(func(_ *Fiber, request, response Value) Value {
		h := common(requestOf(request))
		if recordField(h, "vary") != nil {
			h = h.set("vary", varyWith(responseOf(response).headerRecord(), "Origin"))
		}
		return ResponseSetHeaders(HeadersToList(h), response)
	})
	return handle(fb, handler, request)
}

// varyWith is the `Vary` header with the dimension added.
func varyWith(headers *Record, dimension string) string {
	vary, ok := recordField(headers, "vary").(string)
	if !ok {
		return dimension
	}
	for _, m := range strings.Split(vary, ",") {
		m = strings.ToLower(strings.TrimSpace(m))
		if m == "*" || m == strings.ToLower(dimension) {
			return vary
		}
	}
	return vary + ", " + dimension
}
