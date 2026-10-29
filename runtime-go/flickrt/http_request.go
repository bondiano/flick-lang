package flickrt

import (
	"io"
	"math/big"
	"net"
	"net/http"
	"sort"
	"strings"
	"sync"
)

// Http.ServerRequest as Effect's HttpServerRequest: a request a server got
// or one made of a client request, as a handler sees it. A middleware or a
// router gives a handler a changed copy (another URL, other headers); the
// copies share their source, whose body is read once.

type ServerRequest struct {
	src         *requestSource
	method      string
	url         string // the path and query the handler sees
	originalURL string
	headers     *Record    // nil: the source's
	remote      Value      // nil: the source's
	inflate     *inflation // the decompression middleware's, if any
}

type requestSource struct {
	r       *http.Request  // a request a server got, or
	client  *ClientRequest // the client request it was made of
	marks   *requestMarks
	hdrOnce sync.Once
	hdrs    *Record
	once    sync.Once
	body    []byte
	err     error
}

const maxBodySizeKey = "effect/http/HttpIncomingMessage/MaxBodySize"

func requestOf(v Value) *ServerRequest { return v.(*ServerRequest) }

// modified is a copy of the request, sharing its source.
func (q *ServerRequest) modified() *ServerRequest {
	c := *q
	return &c
}

// sourceHeaders are the headers as Node gives them: lower-case names, the
// values of a repeated one joined, the host first and the others by name.
//
// shortcut: net/http does not keep the order of the headers, so they come sorted by name; keep the order when a program needs it
func (s *requestSource) headers() *Record {
	if s.client != nil {
		return s.client.headers
	}
	s.hdrOnce.Do(func() {
		h := newRecord(len(s.r.Header) + 1)
		if s.r.Host != "" {
			h.put("host", s.r.Host)
		}
		names := make([]string, 0, len(s.r.Header))
		for k := range s.r.Header {
			names = append(names, k)
		}
		sort.Strings(names)
		for _, k := range names {
			sep, name := ", ", strings.ToLower(k)
			if name == "cookie" {
				sep = "; "
			}
			h.put(name, strings.Join(s.r.Header[k], sep))
		}
		s.hdrs = h
	})
	return s.hdrs
}

func (q *ServerRequest) headerRecord() *Record {
	if q.headers != nil {
		return q.headers
	}
	return q.src.headers()
}

func (q *ServerRequest) header(name string) (string, bool) {
	v, ok := recordField(q.headerRecord(), name).(string)
	return v, ok
}

// read is the body's bytes, read once: a served request's up to the
// largest size `with_max_body_size` gives, when it is first read.
func (s *requestSource) read(fb *Fiber) ([]byte, error) {
	s.once.Do(func() {
		switch {
		case s.client != nil:
			s.body, _, s.err = s.client.body.bytes()
		default:
			var r io.Reader = s.r.Body
			limit := int64(-1)
			if fb != nil {
				if v, ok := fb.env.get(maxBodySizeKey); ok && v.(*big.Int).IsInt64() {
					limit = v.(*big.Int).Int64()
					r = io.LimitReader(r, limit+1)
				}
			}
			s.body, s.err = io.ReadAll(r)
			if s.err == nil && limit >= 0 && int64(len(s.body)) > limit {
				s.body, s.err = nil, errString("maxBytes exceeded")
			}
		}
	})
	return s.body, s.err
}

type errString string

func (e errString) Error() string { return string(e) }

// bytes is the body, failing as Effect does with a RequestParseError.
func (q *ServerRequest) bytes(fb *Fiber) []byte {
	var body []byte
	var err error
	if q.inflate != nil {
		body, err = q.inflate.read(fb, q)
	} else {
		body, err = q.src.read(fb)
	}
	if err != nil {
		Fail(q.parseError(err.Error()))
	}
	return body
}

func (q *ServerRequest) details(description Value) Value {
	return Rec("method", methodValue(q.method), "url", redactURL(q.url, defaultUrlRedactedName), "description", description)
}

func (q *ServerRequest) parseError(description string) Value {
	return NewData(serverErrorTag, NewData(parseErrorTag, q.details(Some(description))))
}

func RequestMethod(r Value) Value      { return methodValue(requestOf(r).method) }
func RequestUrl(r Value) Value         { return requestOf(r).url }
func RequestOriginalUrl(r Value) Value { return requestOf(r).originalURL }
func RequestHeaders(r Value) Value     { return requestOf(r).headerRecord() }

// RequestRemoteAddress is the address of the client, as Node gives it.
func RequestRemoteAddress(r Value) Value {
	q := requestOf(r)
	if q.remote != nil {
		return q.remote
	}
	if q.src.r == nil {
		return None
	}
	host, _, err := net.SplitHostPort(q.src.r.RemoteAddr)
	if err != nil {
		return None
	}
	return Some(host)
}

func (q *ServerRequest) cookieHeader() string {
	h, _ := q.header("cookie")
	return h
}

func RequestCookies(r Value) Value { return CookiesParseHeader(requestOf(r).cookieHeader()) }

// searchParams is the query of the URL, as `new URL(url, base)` reads it.
func searchParamsOf(url string) []Value {
	if u, ok := parseURL(url); ok {
		return u.searchParams()
	}
	url, _, _ = strings.Cut(url, "#")
	if _, query, ok := strings.Cut(url, "?"); ok {
		return formDecode(query)
	}
	return []Value{}
}

func RequestSearchParams(r Value) Value { return searchParamsOf(requestOf(r).url) }

func RequestSearchParamsFromUrl(u Value) Value { return UrlParamsToRecord(urlOf(u).searchParams()) }

func RequestText(fb *Fiber, r Value) Value { return decodeUTF8(requestOf(r).bytes(fb)) }

// RequestJson is the body as JSON, `null` for an empty one.
func RequestJson(fb *Fiber, r Value) Value {
	q := requestOf(r)
	text := decodeUTF8(q.bytes(fb))
	if text == "" {
		return JsonNull
	}
	p := &jsonParser{s: text}
	p.space()
	v, ok := p.value()
	if ok {
		p.space()
		if p.i < len(p.s) {
			p.problem, ok = "Unexpected non-whitespace character after JSON "+p.at(), false
		}
	}
	if !ok {
		return Fail(q.parseError(p.reason()))
	}
	return v
}

func RequestUrlParamsBody(fb *Fiber, r Value) Value { return formDecode(RequestText(fb, r).(string)) }

func RequestBytes(fb *Fiber, r Value) Value { return byteList(requestOf(r).bytes(fb)) }

func RequestWithMaxBodySize(fb *Fiber, size, task Value) Value {
	return withEnv(fb, maxBodySizeKey, size, task)
}

// toURL is the full URL of the request: of the `Host` header (localhost
// without one), https when `X-Forwarded-Proto` says so.
func (q *ServerRequest) toURL() Value {
	host, ok := q.header("host")
	if !ok {
		host = "localhost"
	}
	protocol := "http"
	if p, _ := q.header("x-forwarded-proto"); p == "https" {
		protocol = "https"
	}
	return resolveURL(q.url, protocol, host)
}

// resolveURL is `new URL(url, protocol://host)` for a URL a request gives:
// an absolute one, or a path.
func resolveURL(url, protocol, host string) Value {
	if u, ok := parseURL(url); ok {
		return Some(*u)
	}
	base := protocol + "://" + host
	switch {
	case strings.HasPrefix(url, "//"):
		base = protocol + ":"
	case !strings.HasPrefix(url, "/"):
		base += "/"
	}
	if u, ok := parseURL(base + url); ok {
		return Some(*u)
	}
	return None
}

func RequestToUrl(r Value) Value { return requestOf(r).toURL() }

// RequestToClientRequest is the request a client would send: of the full
// URL, its query as the parameters, and the body when the method has one.
func RequestToClientRequest(r Value) Value {
	q := requestOf(r)
	headers := q.headerRecord()
	body := emptyBody
	if hasBody(q.method) {
		if q.src.client != nil && q.src.client.body.kind == bodyForm {
			body = q.src.client.body
		} else {
			ct, hasType := q.header("content-type")
			body = &Body{kind: bodyStream, hasType: hasType, contentType: ct,
				length: parseContentLength(recordField(headers, "content-length")),
				read: func() ([]byte, error) {
					if q.inflate != nil {
						return q.inflate.read(nil, q)
					}
					return q.src.read(nil)
				}}
			if body.length < 0 {
				headers = headers.without("content-length")
			}
		}
	}
	c := makeClient(q.method, q.url, []Value{}, None, headers, body)
	if u, ok := some(q.toURL()); ok {
		w := urlOf(u)
		c.params, c.hash = w.searchParams(), None
		if w.hasFragment && w.fragment != "" {
			c.hash = Some(w.fragment)
		}
		w.query, w.hasQuery, w.fragment, w.hasFragment = "", false, "", false
		c.url = w.href()
	}
	return c
}

func hasBody(method string) bool {
	return method != "GET" && method != "HEAD" && method != "OPTIONS" && method != "TRACE"
}

// RequestFromClientRequest is the request a server would get: its original
// URL the full one of the client request, its URL the path of that.
func RequestFromClientRequest(c Value) Value {
	cr := clientOf(c)
	original := cr.url
	if u, ok := some(cr.toUrl()); ok {
		original = urlOf(u).href()
	}
	return &ServerRequest{
		src:    &requestSource{client: cr, marks: &cr.marks},
		method: cr.method, url: removeHost(original), originalURL: original,
	}
}

// removeHost is the path of a URL, as Effect takes it from a full one.
func removeHost(url string) string {
	if strings.HasPrefix(url, "/") {
		return url
	}
	start := strings.Index(url, "//") + 2
	if start == 1 {
		start = 0
	}
	if i := strings.IndexByte(url[start:], '/'); i >= 0 {
		return url[start+i:]
	}
	return "/"
}

// cookieValues is the cookies by name, as a codec reads them.
func (q *ServerRequest) cookieValues() *Record {
	pairs := CookiesParseHeader(q.cookieHeader()).([]Value)
	cookies := newRecord(len(pairs))
	for _, p := range pairs {
		kv := p.([]Value)
		cookies.put(kv[0].(string), RedactedReveal(kv[1]))
	}
	return cookies
}

func RequestSchemaCookies(fb *Fiber, c, r Value) Value {
	return CodecDecode(fb, c, requestOf(r).cookieValues())
}

func RequestSchemaHeaders(fb *Fiber, c, r Value) Value {
	return CodecDecode(fb, c, requestOf(r).headerRecord())
}

func RequestSchemaSearchParams(fb *Fiber, c, r Value) Value {
	return CodecDecode(fb, c, paramsRecord(searchParamsOf(requestOf(r).url)))
}

// RequestSchemaBodyJson decodes the JSON of the body by a codec.
func RequestSchemaBodyJson(fb *Fiber, c, r Value) Value {
	return CodecDecode(fb, c, RequestJson(fb, r))
}

func RequestSchemaBodyUrlParams(fb *Fiber, c, r Value) Value {
	return CodecDecode(fb, c, paramsRecord(paramsOf(RequestUrlParamsBody(fb, r))))
}
