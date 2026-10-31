package flickrt

import (
	"bytes"
	"io"
	"sync"
)

// Http.ClientResponse as Effect's HttpClientResponse: the status, headers
// and cookies of a response and its body, read when it is asked for. Read
// whole, it is kept, so reading it again gives the same bytes, as Effect's
// web responses cache theirs; streamed, it is read as it comes. A response
// comes from the network or from a handler's response, for a test double.

type ClientResponse struct {
	request *ClientRequest
	url     string
	status  int32
	headers *Record
	cookies *Record
	open    func(fb *Fiber) (io.ReadCloser, error)
	mu      sync.Mutex
	read    bool // data and err are the body read whole
	data    []byte
	err     error
}

func clientResponseOf(v Value) *ClientResponse { return v.(*ClientResponse) }

func (r *ClientResponse) readAll(fb *Fiber) ([]byte, error) {
	r.mu.Lock()
	defer r.mu.Unlock()
	if !r.read {
		body, err := r.open(fb)
		if err == nil {
			r.data, err = io.ReadAll(body)
			body.Close()
		}
		r.err, r.read = err, true
	}
	return r.data, r.err
}

// body is the bytes of the body; a body that cannot be read fails with a
// DecodeError.
func (r *ClientResponse) body(fb *Fiber) []byte {
	data, err := r.readAll(fb)
	if err != nil {
		r.fail(fb, err)
	}
	return data
}

func (r *ClientResponse) fail(fb *Fiber, err error) Value {
	rethrow(err)
	return Fail(responseFailure(fb, decodeErrorTag, r, Some(err.Error())))
}

// cached is the body when it was read whole.
func (r *ClientResponse) cached() ([]byte, error, bool) {
	r.mu.Lock()
	defer r.mu.Unlock()
	return r.data, r.err, r.read
}

// ClientResponseStream is the chunks of the body as they are read.
func ClientResponseStream(v Value) Value {
	r := clientResponseOf(v)
	return streamOpen(func(fb *Fiber) pull {
		if data, err, ok := r.cached(); ok {
			if err != nil {
				r.fail(fb, err)
			}
			return chunkStream([]Value{byteList(data)}).open(fb)
		}
		body, err := r.open(fb)
		if err != nil {
			r.fail(fb, err)
		}
		fb.owner.add(fb, func(*Fiber, *Cause) Value { body.Close(); return Unit })
		buf := make([]byte, 64<<10)
		return func(fb *Fiber) ([]Value, bool) {
			for {
				n, err := body.Read(buf)
				if n > 0 {
					return []Value{byteList(buf[:n])}, true
				}
				if err == io.EOF {
					return nil, false
				}
				if err != nil {
					r.fail(fb, err)
				}
			}
		}
	})
}

// ResponseToClientResponse is the response as a client of the handler
// gets it: of the request's URL without its hash, and with the content
// type of a form body among the headers.
func ResponseToClientResponse(request, r Value) Value {
	c, s := clientOf(request), responseOf(r)
	out := &ClientResponse{request: c, status: s.status, headers: s.headerRecord(), cookies: s.cookies,
		open: func(fb *Fiber) (io.ReadCloser, error) {
			body, _, err := s.body.reader(fb)
			return body, err
		}}
	if s.body.kind == bodyForm {
		data, contentType, _ := s.body.bytes(nil)
		out.headers = out.headers.set("content-type", contentType)
		out.open = func(*Fiber) (io.ReadCloser, error) { return io.NopCloser(bytes.NewReader(data)), nil }
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
		open: func(fb *Fiber) (io.ReadCloser, error) {
			if data, err, ok := in.cached(); ok {
				return io.NopCloser(bytes.NewReader(data)), err
			}
			return in.open(fb)
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
func ClientResponseText(fb *Fiber, r Value) Value {
	return decodeUTF8(bytes.TrimPrefix(clientResponseOf(r).body(fb), []byte("\uFEFF")))
}

// ClientResponseJson is the body as JSON, `null` for an empty one.
func ClientResponseJson(fb *Fiber, r Value) Value {
	text := ClientResponseText(fb, r).(string)
	if text == "" {
		return JsonNull
	}
	v, reason := parseJSON(text)
	if reason != "" {
		return Fail(responseFailure(fb, decodeErrorTag, clientResponseOf(r), Some(reason)))
	}
	return v
}

func ClientResponseUrlParamsBody(fb *Fiber, r Value) Value {
	return formDecode(ClientResponseText(fb, r).(string))
}

func ClientResponseBytes(fb *Fiber, r Value) Value { return byteList(clientResponseOf(r).body(fb)) }

func ClientResponseFilterStatus(fb *Fiber, f, r Value) Value {
	if !Apply(fb, f, clientResponseOf(r).status).(bool) {
		return Fail(responseFailure(fb, statusErrorTag, clientResponseOf(r), Some("invalid status code")))
	}
	return r
}

func ClientResponseFilterStatusOk(fb *Fiber, r Value) Value {
	if s := clientResponseOf(r).status; s < 200 || s >= 300 {
		return Fail(responseFailure(fb, statusErrorTag, clientResponseOf(r), Some("non 2xx status code")))
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

func requestFailure(fb *Fiber, reason string, c *ClientRequest, description Value) Value {
	return NewData(clientErrorTag, NewData(reason, Rec("method", methodValue(c.method),
		"url", redactURL(c.url, currentUrlRedactedNames(fb)), "description", description)))
}

func responseFailure(fb *Fiber, reason string, r *ClientResponse, description Value) Value {
	return NewData(clientErrorTag, NewData(reason, Rec("method", methodValue(r.request.method),
		"url", redactURL(r.request.url, currentUrlRedactedNames(fb)), "status", r.status, "description", description)))
}

// failureReason is the reason of a Flick HttpClientError, if `err` is one.
func failureReason(err Value) (*Data, bool) {
	if d, ok := err.(*Data); ok && d.Tag == clientErrorTag {
		return d.Fields[0].(*Data), true
	}
	return nil, false
}
