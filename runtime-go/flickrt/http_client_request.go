package flickrt

import (
	"encoding/base64"
	"strings"
	"sync"
	"unicode/utf8"
)

// Http.ClientRequest: the method, the URL as given, the parameters and the
// hash added to it, the headers and the body. Each change makes a new
// request.

type ClientRequest struct {
	method  string
	url     string
	params  []Value
	hash    Value // an Option String
	headers *Record
	body    *Body
}

// requestMarks are the state of a request, one a handled request (a served
// one, or one of `to_handled`): whether the logger middleware is off for it
// and the handlers to run on its response. Outside one nothing keeps them.
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

// ClientRequestModify sets what the options give: an empty hash or
// accepted type sets nothing.
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

// methodValue is the method of the name; another name has no value.
func methodValue(method string) Value {
	if m, ok := methods[method]; ok {
		return m
	}
	panic(Defect{"TypeError: not a Http.Method.Method: " + method})
}

func ClientRequestMakeWith(m, url, params, hash, headers, body Value) Value {
	return makeClient(methodName(m), url.(string), paramsOf(params), hash, headersOf(headers), bodyOf(body))
}

func ClientRequestUpdateHeaders(fb *Fiber, f, self Value) Value {
	c := clientOf(self)
	return c.with(headersOf(Apply(fb, f, c.headers)), c.body)
}

// ClientRequestBasicAuth is `Authorization: Basic` of the name and the
// password, encoded as `btoa`, which refuses a character past U+00FF.
func ClientRequestBasicAuth(user, password, self Value) Value {
	text := user.(string) + ":" + RedactedReveal(password).(string)
	latin1 := make([]byte, 0, len(text))
	for i := 0; i < len(text); {
		r, size := utf8.DecodeRuneInString(text[i:])
		if r > 0xFF || r == utf8.RuneError && size == 1 {
			return Die("InvalidCharacterError: Invalid character")
		}
		latin1 = append(latin1, byte(r))
		i += size
	}
	return ClientRequestSetHeader("Authorization", "Basic "+base64.StdEncoding.EncodeToString(latin1), self)
}

func ClientRequestBearerToken(token, self Value) Value {
	return ClientRequestSetHeader("Authorization", "Bearer "+RedactedReveal(token).(string), self)
}

// changedURL is the request with another URL, parameters or hash.
func (c *ClientRequest) changedURL(url string, params []Value, hash Value) Value {
	return makeClient(c.method, url, params, hash, c.headers, c.body)
}

// joinSegments joins two parts of a URL with one `/` between them.
func joinSegments(first, second string) string {
	ends, starts := strings.HasSuffix(first, "/"), strings.HasPrefix(second, "/")
	switch {
	case ends && starts:
		return first + second[1:]
	case !ends && !starts:
		return first + "/" + second
	}
	return first + second
}

func ClientRequestPrependUrl(path, self Value) Value {
	c := clientOf(self)
	if path == "" {
		return c
	}
	return c.changedURL(joinSegments(path.(string), c.url), c.params, c.hash)
}

func ClientRequestAppendUrl(path, self Value) Value {
	c := clientOf(self)
	if path == "" {
		return c
	}
	return c.changedURL(joinSegments(c.url, path.(string)), c.params, c.hash)
}

func ClientRequestUpdateUrl(fb *Fiber, f, self Value) Value {
	c := clientOf(self)
	return c.changedURL(Apply(fb, f, c.url).(string), c.params, c.hash)
}

func ClientRequestSetUrlParam(key, val, self Value) Value {
	c := clientOf(self)
	return c.changedURL(c.url, paramsOf(UrlParamsSet(key, val, c.params)), c.hash)
}

func ClientRequestSetUrlParams(input, self Value) Value {
	c := clientOf(self)
	return c.changedURL(c.url, paramsOf(UrlParamsSetAll(input, c.params)), c.hash)
}

func ClientRequestAppendUrlParam(key, val, self Value) Value {
	c := clientOf(self)
	return c.changedURL(c.url, paramsOf(UrlParamsAppend(key, val, c.params)), c.hash)
}

func ClientRequestAppendUrlParams(input, self Value) Value {
	c := clientOf(self)
	return c.changedURL(c.url, paramsOf(UrlParamsAppendAll(input, c.params)), c.hash)
}

func ClientRequestSetHash(hash, self Value) Value {
	c := clientOf(self)
	return c.changedURL(c.url, c.params, Some(hash))
}

func ClientRequestRemoveHash(self Value) Value {
	c := clientOf(self)
	return c.changedURL(c.url, c.params, None)
}

func ClientRequestSchemaBodyJson(fb *Fiber, codec, v, self Value) Value {
	return ClientRequestBodyJson(fb, CodecEncode(fb, codec, v), self)
}

// ClientRequestCompressBody compresses a body of bytes of at least
// `minSize` (by default 1024) bytes, unless the request has a
// `Content-Encoding` already.
func ClientRequestCompressBody(_ *Fiber, algorithm, minSize, self Value) Value {
	c := clientOf(self)
	if c.headers.index("content-encoding") >= 0 || c.body.kind != bodyBytes {
		return c
	}
	least := 1024.0
	if n, ok := some(minSize); ok {
		least = n.(float64)
	}
	if float64(c.body.length) < least {
		return c
	}
	data, contentType, _ := c.body.bytes(nil)
	name := algorithmOf[ConstructorName(algorithm.(*Data).Tag)]
	packed, err := compress(name, -1, data)
	if err != nil {
		return Die(err.Error())
	}
	body := bytesBody(string(packed), contentType)
	return ClientRequestSetHeader("content-encoding", name, ClientRequestSetBody(body, c))
}
