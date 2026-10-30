package flickrt

import (
	"strconv"
	"strings"
)

// Http.ServerResponse as Effect's HttpServerResponse: an immutable status,
// status text, headers, cookies and body. As Effect makes a response, the
// content type and length of its body become its headers; a response of
// no other headers keeps them implicit, so the common answer of a text or
// JSON body costs one allocation and the server writes them directly.

type ServerResponse struct {
	status     int32
	statusText string
	hasText    bool
	headers    *Record // nil: those of the body, if any
	cookies    *Record
	body       *Body
	own        Body // the body of a response made with one
}

func responseOf(v Value) *ServerResponse { return v.(*ServerResponse) }

// derivesHeaders is whether the body gives a response headers.
func derivesHeaders(b *Body) bool {
	return b.kind != bodyEmpty && (b.hasType && b.contentType != "" || b.length >= 0)
}

// newResponse is Effect's `makeResponse`: headers `owned` were just made
// for the response, and a content type or length they set wins over the
// body's.
func newResponse(status int32, statusText string, hasText bool, headers, cookies *Record, body *Body, owned bool) *ServerResponse {
	r := &ServerResponse{status: status, statusText: statusText, hasText: hasText, cookies: cookies, body: body}
	if r.cookies == nil {
		r.cookies = jarOf(CookiesEmpty)
	}
	if body == nil {
		r.body = emptyBody
	}
	r.headers = r.withBody(headers, owned)
	return r
}

func (r *ServerResponse) withBody(headers *Record, owned bool) *Record {
	b := r.body
	if !derivesHeaders(b) {
		return headers
	}
	if headers == nil {
		if !owned {
			return nil
		}
		headers = headersOf(HeadersEmpty)
	}
	if !owned && len(headers.keys) == 0 {
		return nil
	}
	h := newRecord(len(headers.keys) + 2)
	h.keys, h.vals = append(h.keys, headers.keys...), append(h.vals, headers.vals...)
	if b.hasType && b.contentType != "" && (!owned || h.index("content-type") < 0) {
		h.put("content-type", b.contentType)
	}
	if b.length >= 0 && (!owned || h.index("content-length") < 0) {
		h.put("content-length", strconv.FormatInt(b.length, 10))
	}
	return h
}

// bodyResponse is a response of a body made with it: one allocation.
func bodyResponse(options Value, status int32, headers *Record, body Body) *ServerResponse {
	o := record(options)
	r := &ServerResponse{status: status, body: emptyBody}
	r.own = body
	r.body = &r.own
	r.setOptions(o)
	if c, ok := some(o.Get("cookies")); ok {
		r.cookies = jarOf(c)
	} else {
		r.cookies = jarOf(CookiesEmpty)
	}
	r.headers = r.withBody(headers, false)
	return r
}

func (r *ServerResponse) setOptions(o *Record) {
	if s, ok := some(o.Get("status")); ok {
		r.status = s.(int32)
	}
	if t, ok := some(o.Get("statusText")); ok {
		r.statusText, r.hasText = t.(string), true
	}
}

// optionHeaders are the headers the options give, nil for none.
func optionHeaders(o *Record) *Record {
	if h, ok := some(o.Get("headers")); ok {
		return headersOf(HeadersFromInput(h))
	}
	return nil
}

// contentTypeOf is Effect's content type of a body the options make: theirs,
// or that of their headers.
func contentTypeOf(o *Record, headers *Record, otherwise string) string {
	if t, ok := some(o.Get("contentType")); ok && t != "" {
		return t.(string)
	}
	if headers != nil {
		if t, ok := recordField(headers, "content-type").(string); ok {
			return t
		}
	}
	return otherwise
}

// headerRecord is the response's headers, those of its body included.
func (r *ServerResponse) headerRecord() *Record {
	if r.headers != nil {
		return r.headers
	}
	if !derivesHeaders(r.body) {
		return headersOf(HeadersEmpty)
	}
	return r.withBody(headersOf(HeadersEmpty), true)
}

func optionsResponse(options Value, status int32, headers *Record, body *Body) *ServerResponse {
	o := record(options)
	r := newResponse(status, "", false, headers, nil, body, false)
	r.setOptions(o)
	if c, ok := some(o.Get("cookies")); ok {
		r.cookies = jarOf(c)
	}
	return r
}

func ResponseEmpty(options Value) Value {
	return optionsResponse(options, 204, optionHeaders(record(options)), nil)
}

func ResponseRedirect(location, options Value) Value {
	headers := headersOf(HeadersEmpty).set("location", location)
	if h := optionHeaders(record(options)); h != nil {
		headers = headersOf(HeadersMerge(h, headers))
	}
	return optionsResponse(options, 302, headers, nil)
}

func ResponseUint8Array(list, options Value) Value {
	o := record(options)
	h := optionHeaders(o)
	return bodyResponse(options, 200, h, *bytesBody(byteString(list), contentTypeOf(o, h, "application/octet-stream")))
}

func ResponseText(text, options Value) Value {
	o := record(options)
	h := optionHeaders(o)
	return bodyResponse(options, 200, h, *textBody(text.(string), contentTypeOf(o, h, "text/plain")))
}

// noOptions are options that set nothing.
var noOptions = Rec("status", None, "statusText", None, "headers", None, "cookies", None, "contentType", None)

func ResponseHtml(markup Value) Value {
	return bodyResponse(noOptions, 200, nil, *textBody(markup.(string), "text/html"))
}

func ResponseJson(_ *Fiber, json, options Value) Value {
	o := record(options)
	h := optionHeaders(o)
	return bodyResponse(options, 200, h, *textBody(JsonStringify(json).(string), contentTypeOf(o, h, "application/json")))
}

// ResponseSchemaJson is the JSON of a value of a codec: a canonical JSON
// codec writes it at once.
func ResponseSchemaJson(fb *Fiber, c, v, options Value) Value {
	w, ok := c.(*wire)
	if !ok {
		return ResponseJson(fb, CodecEncode(fb, c, v), options)
	}
	var b strings.Builder
	b.Grow(256)
	if issues := w.write(fb, &b, v); issues != nil {
		return Fail(schemaErrorOf(issues))
	}
	o := record(options)
	h := optionHeaders(o)
	return bodyResponse(options, 200, h, *textBody(b.String(), contentTypeOf(o, h, "application/json")))
}

func ResponseUrlParams(params, options Value) Value {
	o := record(options)
	h := optionHeaders(o)
	return bodyResponse(options, 200, h, *textBody(formEncode(paramsOf(params)), contentTypeOf(o, h, "application/x-www-form-urlencoded")))
}

func ResponseFormData(pairs, options Value) Value {
	return optionsResponse(options, 200, optionHeaders(record(options)), bodyOf(BodyFormData(pairs)))
}

func ResponseStatus(r Value) Value { return responseOf(r).status }

func ResponseStatusText(r Value) Value {
	if s := responseOf(r); s.hasText {
		return Some(s.statusText)
	}
	return None
}

func ResponseHeaders(r Value) Value { return responseOf(r).headerRecord() }
func ResponseCookies(r Value) Value { return responseOf(r).cookies }
func ResponseBody(r Value) Value    { return responseOf(r).body }

// changed is the response with other headers, cookies or body, as Effect's
// `makeResponse({ ...self, … })` makes it.
func (r *ServerResponse) changed(headers, cookies *Record, body *Body, owned bool) Value {
	return newResponse(r.status, r.statusText, r.hasText, headers, cookies, body, owned)
}

func ResponseSetHeader(name, value, r Value) Value {
	s := responseOf(r)
	return s.changed(headersOf(HeadersSet(name, value, s.headerRecord())), s.cookies, s.body, true)
}

func ResponseSetHeaders(input, r Value) Value {
	s := responseOf(r)
	return s.changed(headersOf(HeadersSetAll(input, s.headerRecord())), s.cookies, s.body, true)
}

func ResponseRemoveHeader(name, r Value) Value {
	s := responseOf(r)
	return s.changed(headersOf(HeadersRemove(name, s.headerRecord())), s.cookies, s.body, false)
}

func ResponseRemoveCookie(name, r Value) Value {
	s := responseOf(r)
	return s.changed(s.headers, jarOf(CookiesRemove(name, s.cookies)), s.body, false)
}

func ResponseReplaceCookies(cookies, r Value) Value {
	s := responseOf(r)
	return s.changed(s.headers, jarOf(cookies), s.body, false)
}

func ResponseUpdateCookies(fb *Fiber, f, r Value) Value {
	s := responseOf(r)
	return s.changed(s.headers, jarOf(Apply(fb, f, s.cookies)), s.body, false)
}

func ResponseMergeCookies(cookies, r Value) Value {
	s := responseOf(r)
	return s.changed(s.headers, jarOf(CookiesMerge(cookies, s.cookies)), s.body, false)
}

func ResponseSetCookie(_ *Fiber, name, val, attributes, r Value) Value {
	s := responseOf(r)
	return s.changed(s.headers, jarOf(CookiesSet(name, val, attributes, s.cookies)), s.body, false)
}

func ResponseExpireCookie(_ *Fiber, name, attributes, r Value) Value {
	s := responseOf(r)
	return s.changed(s.headers, jarOf(CookiesExpireCookie(name, attributes, s.cookies)), s.body, false)
}

func ResponseSetCookies(_ *Fiber, entries, r Value) Value {
	s := responseOf(r)
	return s.changed(s.headers, jarOf(CookiesSetAll(entries, s.cookies)), s.body, false)
}

// ResponseSetBody is the response with the body, its content type and
// length as headers.
func ResponseSetBody(body, r Value) Value {
	s := responseOf(r)
	return s.changed(withBodyHeaders(s.headerRecord(), bodyOf(body)), s.cookies, bodyOf(body), true)
}

func ResponseSetStatus(status, text, r Value) Value {
	s := responseOf(r)
	out := responseOf(s.changed(s.headers, s.cookies, s.body, false))
	out.status = status.(int32)
	if t, ok := some(text); ok {
		out.statusText, out.hasText = t.(string), true
	}
	return out
}

// omitsBody is whether the response is sent without its body.
func (r *ServerResponse) omitsBody(withoutBody bool) bool {
	return withoutBody || r.status == 204 || r.status == 205 || r.status == 304
}

func ResponseOmitsBody(withoutBody, r Value) Value {
	return responseOf(r).omitsBody(withoutBody.(bool))
}

// statusOf is the status a server answers a failure with: that of a Flick
// HttpServerError or MultipartError, 503 for an interruption, 500 otherwise.
func statusOf(c *Cause) int32 {
	if r := c.firstOf(causeFail); r != nil {
		if d, ok := r.err.(*Data); ok && d.Tag == serverErrorTag {
			switch d.Fields[0].(*Data).Tag {
			case parseErrorTag:
				return 400
			case routeMissingTag:
				return 404
			}
			return 500
		}
		if d, ok := r.err.(*Data); ok && d.Tag == multipartErrorTag {
			return multipartStatus[d.Fields[0].(*Data).Tag]
		}
	}
	if !c.has(causeFail) && !c.has(causeDie) && c.has(causeInterrupt) {
		return 503
	}
	return 500
}

func causeResponse(c *Cause) *ServerResponse {
	return newResponse(statusOf(c), "", false, nil, nil, nil, false)
}

func ResponseCauseResponse(_ *Fiber, failure Value) Value {
	return causeResponse(causeOfFailure(failure))
}
