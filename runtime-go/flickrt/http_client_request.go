package flickrt

import "sync"

// Http.ClientRequest as Effect's HttpClientRequest: the method, the URL as
// given, the parameters and the hash added to it, the headers and the body.
// Each change makes a new request.
//
// shortcut: requests are values to build and to hand a handler as a test input; sending them comes with the go target's HTTP client

type ClientRequest struct {
	method  string
	url     string
	params  []Value
	hash    Value // an Option String
	headers *Record
	body    *Body
	// what the server requests made of this one note on it, as Effect keys
	// them by the request they come from
	marks requestMarks
}

// requestMarks are what Effect keeps by a request's source: whether the
// logger middleware is off for it and the handlers to run on its response.
type requestMarks struct {
	mu          sync.Mutex
	loggerOff   bool
	preResponse []preResponse
}

type preResponse func(fb *Fiber, request, response Value) Value

func clientOf(v Value) *ClientRequest { return v.(*ClientRequest) }

func makeClient(method, url string, params []Value, hash Value, headers *Record, body *Body) *ClientRequest {
	return &ClientRequest{method: method, url: url, params: params, hash: hash, headers: headers, body: body}
}

// with is the request with the headers and the body given.
func (c *ClientRequest) with(headers *Record, body *Body) *ClientRequest {
	return makeClient(c.method, c.url, c.params, c.hash, headers, body)
}

var ClientRequestEmpty Value = makeClient("GET", "", []Value{}, None, headersOf(HeadersEmpty), emptyBody)

func methodName(m Value) string { return ConstructorName(m.(*Data).Tag) }

func clientOfMethod(method string) func(Value) Value {
	return func(url Value) Value {
		return makeClient(method, url.(string), []Value{}, None, headersOf(HeadersEmpty), emptyBody)
	}
}

var (
	ClientRequestGet     = clientOfMethod("GET")
	ClientRequestPost    = clientOfMethod("POST")
	ClientRequestPut     = clientOfMethod("PUT")
	ClientRequestPatch   = clientOfMethod("PATCH")
	ClientRequestDelete  = clientOfMethod("DELETE")
	ClientRequestHead    = clientOfMethod("HEAD")
	ClientRequestOptions = clientOfMethod("OPTIONS")
	ClientRequestTrace   = clientOfMethod("TRACE")
	ClientRequestQuery   = clientOfMethod("QUERY")
)

func ClientRequestMake(m, url, options Value) Value {
	return ClientRequestModify(options, clientOfMethod(methodName(m))(url))
}

// ClientRequestModify sets what the options give, as Effect's `modify`
// does: an empty hash or accepted type sets nothing.
func ClientRequestModify(options, self Value) Value {
	o, c := record(options), clientOf(self)
	if h, ok := some(o.Get("headers")); ok {
		c = clientOf(ClientRequestSetHeaders(h, c))
	}
	if p, ok := some(o.Get("urlParams")); ok {
		c = makeClient(c.method, c.url, paramsOf(UrlParamsSetAll(p, c.params)), c.hash, c.headers, c.body)
	}
	if h, ok := some(o.Get("hash")); ok && h != "" {
		c = makeClient(c.method, c.url, c.params, Some(h), c.headers, c.body)
	}
	if b, ok := some(o.Get("body")); ok {
		c = clientOf(ClientRequestSetBody(b, c))
	}
	if a, ok := some(o.Get("accept")); ok && a != "" {
		c = clientOf(ClientRequestAccept(a, c))
	}
	if o.Get("acceptJson").(bool) {
		c = clientOf(ClientRequestAccept("application/json", c))
	}
	return c
}

func ClientRequestSetMethod(m, self Value) Value {
	c := clientOf(self)
	return makeClient(methodName(m), c.url, c.params, c.hash, c.headers, c.body)
}

func ClientRequestSetHeader(name, value, self Value) Value {
	c := clientOf(self)
	return c.with(headersOf(HeadersSet(name, value, c.headers)), c.body)
}

func ClientRequestSetHeaders(input, self Value) Value {
	c := clientOf(self)
	return c.with(headersOf(HeadersSetAll(input, c.headers)), c.body)
}

func ClientRequestRemoveHeader(name, self Value) Value {
	c := clientOf(self)
	return c.with(headersOf(HeadersRemove(name, c.headers)), c.body)
}

func ClientRequestAccept(mediaType, self Value) Value {
	return ClientRequestSetHeader("Accept", mediaType, self)
}

func ClientRequestAcceptJson(self Value) Value {
	return ClientRequestSetHeader("Accept", "application/json", self)
}

func ClientRequestSetUrl(url, self Value) Value {
	c := clientOf(self)
	return makeClient(c.method, url.(string), c.params, c.hash, c.headers, c.body)
}

// ClientRequestSetBody is the request with the body, its content type and
// length as headers.
func ClientRequestSetBody(body, self Value) Value {
	c := clientOf(self)
	return c.with(withBodyHeaders(c.headers, bodyOf(body)), bodyOf(body))
}

func ClientRequestBodyText(text, contentType, self Value) Value {
	return ClientRequestSetBody(BodyText(text, contentType), self)
}

func ClientRequestBodyUint8Array(list, contentType, self Value) Value {
	return ClientRequestSetBody(BodyUint8Array(list, contentType), self)
}

func ClientRequestBodyUrlParams(input, self Value) Value {
	return ClientRequestSetBody(BodyUrlParams(input, None), self)
}

func ClientRequestBodyFormData(pairs, self Value) Value {
	return ClientRequestSetBody(BodyFormData(pairs), self)
}

func ClientRequestBodyJson(_ *Fiber, json, self Value) Value {
	return ClientRequestSetBody(jsonBody(json, "application/json"), self)
}

func ClientRequestMethod(self Value) Value    { return methodValue(clientOf(self).method) }
func ClientRequestUrl(self Value) Value       { return clientOf(self).url }
func ClientRequestUrlParams(self Value) Value { return clientOf(self).params }
func ClientRequestHash(self Value) Value      { return clientOf(self).hash }
func ClientRequestHeaders(self Value) Value   { return clientOf(self).headers }
func ClientRequestBody(self Value) Value      { return clientOf(self).body }
func ClientRequestToUrl(self Value) Value     { return clientOf(self).toUrl() }
func (c *ClientRequest) toUrl() Value         { return resultOption(UrlMake(c.url, c.params, c.hash)) }

func resultOption(r Value) Value {
	if isOk(r) {
		return Some(value(r))
	}
	return None
}

// methods are the values of `Http.Method.Method`, by name: a request reads
// its method on every call.
var methods = func() map[string]Value {
	out := map[string]Value{}
	for _, m := range []string{"GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS", "TRACE", "QUERY"} {
		out[m] = Nullary(Tag("Http.Method.Method." + m))
	}
	return out
}()

// methodValue is the method of the name; Effect has no value for another.
func methodValue(method string) Value {
	if m, ok := methods[method]; ok {
		return m
	}
	panic(Defect{"TypeError: not a Http.Method.Method: " + method})
}
