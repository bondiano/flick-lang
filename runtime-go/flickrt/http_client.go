package flickrt

// Http.Client as Effect's HttpClient: a client is the preparation of each
// request and the sending of the prepared one, both tasks, so that a
// combinator retrying the sending prepares the request again as Effect's
// does. Its failures are Flick values already: a transport makes the Flick
// HttpClientError itself.

type Client struct {
	pre  func(request Value) Value  // a task of the prepared request
	post func(prepared Value) Value // a task of the response, of a task of the prepared request
}

func httpClientOf(v Value) *Client { return v.(*Client) }

func (c *Client) execute(request Value) Value { return c.post(c.pre(request)) }

// The fiber-local settings of the client's spans, as Effect's references.
const (
	clientTracerDisabledKey = "effect/http/HttpClient/TracerDisabledWhen"
	clientHeaderFilterKey   = "effect/http/HttpClient/TracerHeaderFilter"
	clientPropagationKey    = "effect/http/HttpClient/TracerPropagationEnabled"
	clientSpanNameKey       = "effect/http/HttpClient/SpanNameGenerator"
)

func HttpClientExecute(fb *Fiber, client, request Value) Value {
	return Run(fb, httpClientOf(client).execute(request))
}

// transport sends a prepared request to its URL.
type transport func(fb *Fiber, request *ClientRequest, url Value) Value

// HttpClientMake is a client of a Flick function of the request and its
// URL.
func HttpClientMake(f Value) Value {
	return clientOfTransport(func(fb *Fiber, request *ClientRequest, url Value) Value {
		return Run(fb, Apply(Apply(f, request), url))
	})
}

// clientOfTransport is Effect's `make`: the URL of the request with its
// parameters and hash, an InvalidUrlError without one; the request sent in
// a client span whose trace context goes with it as headers, unless the
// fiber's settings say otherwise.
//
// shortcut: the span has no attributes and is not exported, as the go target's spans are not; record the request and response when spans are
func clientOfTransport(send transport) *Client {
	return &Client{pre: Succeed, post: func(prepared Value) Value {
		return Eff(func(fb *Fiber) Value {
			request := clientOf(Run(fb, prepared))
			url := UrlMake(request.url, request.params, request.hash)
			if !isOk(url) {
				// Effect's UrlError has no message of its own
				return Fail(requestFailure(invalidUrlTag, request, Some("")))
			}
			if f, ok := fb.env.get(clientTracerDisabledKey); ok && Apply(f, request).(bool) {
				return send(fb, request, value(url))
			}
			name := Value(request.method)
			if f, ok := fb.env.get(clientSpanNameKey); ok {
				name = Apply(f, request)
			}
			s := openSpan(fb, name, Rec("parent", None, "root", false))
			if on, ok := fb.env.get(clientPropagationKey); !ok || on.(bool) {
				request = clientOf(ClientRequestSetHeaders([]Value{
					[]Value{"b3", s.b3()},
					[]Value{"traceparent", "00-" + s.traceId + "-" + s.spanId + "-01"},
				}, request))
			}
			return withEnv(fb, parentSpanKey, s, Eff(func(fb *Fiber) Value { return send(fb, request, value(url)) }))
		})
	}}
}

// b3 is the span's compact B3 header.
func (s *span) b3() string {
	out := s.traceId + "-" + s.spanId + "-1"
	if p, ok := some(s.parent); ok {
		out += "-" + spanOf(p).spanId
	}
	return out
}

// HttpClientMakeWith is a client of the sending of the prepared request
// and the preparation of each request, both Flick functions.
func HttpClientMakeWith(post, pre Value) Value {
	return &Client{
		pre:  func(request Value) Value { return Apply(pre, request) },
		post: func(prepared Value) Value { return Apply(post, prepared) },
	}
}

// clientImpl is the operations of the Flick service `Http.Client.HttpClient`.
func clientImpl(client Value) Value {
	return Rec("client", Fn(func(Value) Value { return Succeed(client) }))
}

func HttpClientLayer(key, task Value) Value {
	return LayerEffect(key, MapEff(Fn(clientImpl), task))
}

func HttpClientWithTracerDisabledWhen(fb *Fiber, f, task Value) Value {
	return withEnv(fb, clientTracerDisabledKey, f, task)
}

func HttpClientWithTracerHeaderFilter(fb *Fiber, f, task Value) Value {
	return withEnv(fb, clientHeaderFilterKey, f, task)
}

func HttpClientWithTracerPropagation(fb *Fiber, on, task Value) Value {
	return withEnv(fb, clientPropagationKey, on, task)
}

func HttpClientWithSpanNameGenerator(fb *Fiber, f, task Value) Value {
	return withEnv(fb, clientSpanNameKey, f, task)
}

// HttpClientWithScope is the client itself: a response's body is read
// with it, so nothing stays open for a scope to close.
func HttpClientWithScope(client Value) Value { return client }
