package flickrt

import (
	"bytes"
	"sync"
)

// Http.ClientResponse as Effect's HttpClientResponse: the status, headers
// and cookies of a response and its body, read once and kept, so reading
// it again gives the same bytes, as Effect's web responses cache theirs.
// A response comes from the network or from a handler's response, for a
// test double.

type ClientResponse struct {
	request *ClientRequest
	url     string
	status  int32
	headers *Record
	cookies *Record
	read    func() ([]byte, error)
	once    sync.Once
	data    []byte
	err     error
}

func clientResponseOf(v Value) *ClientResponse { return v.(*ClientResponse) }

// body is the bytes of the body; a body that cannot be read fails with a
// DecodeError.
func (r *ClientResponse) body() []byte {
	r.once.Do(func() { r.data, r.err = r.read() })
	if r.err != nil {
		Fail(responseFailure(decodeErrorTag, r, Some(r.err.Error())))
	}
	return r.data
}

// ResponseToClientResponse is the response as a client of the handler
// gets it: of the request's URL without its hash, and with the content
// type of a form body among the headers.
func ResponseToClientResponse(request, r Value) Value {
	c, s := clientOf(request), responseOf(r)
	out := &ClientResponse{request: c, status: s.status, headers: s.headerRecord(), cookies: s.cookies,
		read: func() ([]byte, error) {
			data, _, err := s.body.bytes()
			return data, err
		}}
	if s.body.kind == bodyForm {
		data, contentType, _ := s.body.bytes()
		out.headers = out.headers.set("content-type", contentType)
		out.read = func() ([]byte, error) { return data, nil }
	}
	if c != ClientRequestEmpty {
		if u, ok := some(c.toUrl()); ok {
			w := urlOf(u)
			w.fragment, w.hasFragment = "", false
			out.url = w.href()
		}
	}
	return out
}

// ResponseFromClientResponse is the response a proxy sends: the status,
// the headers but `Set-Cookie`, the cookies, and the body as it comes.
func ResponseFromClientResponse(c Value) Value {
	in := clientResponseOf(c)
	headers := in.headers.without("set-cookie")
	ct, hasType := recordField(headers, "content-type").(string)
	body := &Body{kind: bodyStream, hasType: hasType, contentType: ct,
		length: parseContentLength(recordField(headers, "content-length")),
		read: func() ([]byte, error) {
			in.once.Do(func() { in.data, in.err = in.read() })
			return in.data, in.err
		}}
	return newResponse(in.status, "", false, headers, in.cookies, body, false)
}

func ClientResponseStatus(r Value) Value  { return clientResponseOf(r).status }
func ClientResponseHeaders(r Value) Value { return clientResponseOf(r).headers }
func ClientResponseCookies(r Value) Value { return clientResponseOf(r).cookies }
func ClientResponseUrl(r Value) Value     { return clientResponseOf(r).url }
func ClientResponseRequest(r Value) Value { return clientResponseOf(r).request }

// ClientResponseText is the body as UTF-8, a leading byte order mark left
// out as TextDecoder does.
func ClientResponseText(_ *Fiber, r Value) Value {
	return decodeUTF8(bytes.TrimPrefix(clientResponseOf(r).body(), []byte("\uFEFF")))
}

// ClientResponseJson is the body as JSON, `null` for an empty one.
func ClientResponseJson(fb *Fiber, r Value) Value {
	text := ClientResponseText(fb, r).(string)
	if text == "" {
		return JsonNull
	}
	v, reason := parseJSON(text)
	if reason != "" {
		return Fail(responseFailure(decodeErrorTag, clientResponseOf(r), Some(reason)))
	}
	return v
}

func ClientResponseUrlParamsBody(fb *Fiber, r Value) Value {
	return formDecode(ClientResponseText(fb, r).(string))
}

func ClientResponseBytes(_ *Fiber, r Value) Value { return byteList(clientResponseOf(r).body()) }

func ClientResponseFilterStatus(_ *Fiber, f, r Value) Value {
	if !Apply(f, clientResponseOf(r).status).(bool) {
		return Fail(responseFailure(statusErrorTag, clientResponseOf(r), Some("invalid status code")))
	}
	return r
}

func ClientResponseFilterStatusOk(_ *Fiber, r Value) Value {
	if s := clientResponseOf(r).status; s < 200 || s >= 300 {
		return Fail(responseFailure(statusErrorTag, clientResponseOf(r), Some("non 2xx status code")))
	}
	return r
}

// schemaInput is `{ status, headers }` of the response, with the body when
// one is given, for a codec.
func (r *ClientResponse) schemaInput(body Value) *Record {
	if body == nil {
		return record(Rec("status", float64(r.status), "headers", r.headers))
	}
	return record(Rec("status", float64(r.status), "headers", r.headers, "body", body))
}

func ClientResponseSchemaJson(fb *Fiber, c, r Value) Value {
	body := ClientResponseJson(fb, r)
	return CodecDecode(fb, c, clientResponseOf(r).schemaInput(body))
}

func ClientResponseSchemaNoBody(fb *Fiber, c, r Value) Value {
	return CodecDecode(fb, c, clientResponseOf(r).schemaInput(nil))
}

func ClientResponseSchemaBodyJson(fb *Fiber, c, r Value) Value {
	return CodecDecode(fb, c, ClientResponseJson(fb, r))
}

func ClientResponseSchemaBodyUrlParams(fb *Fiber, c, r Value) Value {
	return CodecDecode(fb, c, paramsRecord(paramsOf(ClientResponseUrlParamsBody(fb, r))))
}

func ClientResponseSchemaHeaders(fb *Fiber, c, r Value) Value {
	return CodecDecode(fb, c, clientResponseOf(r).headers)
}

// The Flick error `Http.ClientError.HttpClientError` and its reasons; the
// URL in it has its secrets masked.
var (
	clientErrorTag    = Tag("Http.ClientError.HttpClientError.HttpClientError")
	transportErrorTag = Tag("Http.ClientError.ClientErrorReason.TransportError")
	invalidUrlTag     = Tag("Http.ClientError.ClientErrorReason.InvalidUrlError")
	statusErrorTag    = Tag("Http.ClientError.ClientErrorReason.StatusCodeError")
	decodeErrorTag    = Tag("Http.ClientError.ClientErrorReason.DecodeError")
)

func requestFailure(reason string, c *ClientRequest, description Value) Value {
	return NewData(clientErrorTag, NewData(reason, Rec("method", methodValue(c.method),
		"url", redactURL(c.url, defaultUrlRedactedName), "description", description)))
}

func responseFailure(reason string, r *ClientResponse, description Value) Value {
	return NewData(clientErrorTag, NewData(reason, Rec("method", methodValue(r.request.method),
		"url", redactURL(r.request.url, defaultUrlRedactedName), "status", r.status, "description", description)))
}

// failureReason is the reason of a Flick HttpClientError, if `err` is one.
func failureReason(err Value) (*Data, bool) {
	if d, ok := err.(*Data); ok && d.Tag == clientErrorTag {
		return d.Fields[0].(*Data), true
	}
	return nil, false
}
