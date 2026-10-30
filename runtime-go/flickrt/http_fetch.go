package flickrt

import (
	"bytes"
	"context"
	"errors"
	"io"
	"net/http"
	"runtime"
	"slices"
	"strings"
	"sync/atomic"
)

// Http.FetchClient as Effect's FetchHttpClient, on net/http: the request's
// headers but `Content-Length`, which the transport sets from the body;
// redirects followed up to 20 times as fetch follows them; the response's
// headers by lower-case name in order, their values joined, and its
// cookies of every `Set-Cookie`. An interruption of the fiber cancels the
// request, or the read of its body, which is read when it is asked for.
//
// shortcut: a stream body is read whole before the request is sent; pipe it to the connection when programs upload large streams

var fetchClient = &http.Client{
	CheckRedirect: func(_ *http.Request, via []*http.Request) error {
		if len(via) >= 20 {
			return errors.New("redirect count exceeded")
		}
		return nil
	},
}

// fetchTransport sends the request with net/http; a failure before the
// response is a TransportError, described as fetch describes it.
func fetchTransport(fb *Fiber, request *ClientRequest, url Value) Value {
	data, contentType, err := request.body.bytes(fb)
	if err != nil {
		var c *causeError
		if errors.As(err, &c) && c.cause.has(causeInterrupt) {
			rethrow(err)
		}
		return Fail(requestFailure(transportErrorTag, request, Some(err.Error())))
	}
	type answer struct {
		response *ClientResponse
		failed   bool
	}
	// the request outlives the call, until its body is read
	reqCtx, cancel := context.WithCancel(context.Background())
	got := ForeignAsync(fb, func(ctx Context) Value {
		stop := context.AfterFunc(ctx, cancel)
		defer stop()
		var body io.Reader
		if request.body.kind != bodyEmpty {
			body = bytes.NewReader(data)
		}
		req, err := http.NewRequestWithContext(reqCtx, request.method, urlOf(url).href(), body)
		if err != nil {
			return answer{failed: true}
		}
		h := request.headers
		for i, k := range h.keys {
			if k != "content-length" {
				req.Header.Set(k, wellFormed(h.vals[i].(string)))
			}
		}
		if request.body.kind == bodyForm {
			req.Header.Set("content-type", contentType)
		}
		res, err := fetchClient.Do(req)
		if err != nil {
			return answer{failed: true}
		}
		return answer{response: webResponse(request, res, cancel)}
	}).(answer)
	if got.failed {
		cancel()
		return Fail(requestFailure(transportErrorTag, request, Some("fetch failed")))
	}
	return got.response
}

// netBody is a response's body on the network, closed when it is read to
// its end, when it is closed, or when no one can read it any more.
type netBody struct {
	body   io.ReadCloser
	cancel context.CancelFunc
}

func (b *netBody) close() {
	b.body.Close()
	b.cancel()
}

// fiberReader reads the body in a fiber whose interruption stops the read.
type fiberReader struct {
	fb   *Fiber
	body *netBody
}

func (r *fiberReader) Read(p []byte) (int, error) {
	type read struct {
		n   int
		err error
	}
	// a read the fiber's interruption left behind writes to a buffer of its own
	buf := make([]byte, len(p))
	got := ForeignAsync(r.fb, func(ctx Context) Value {
		stop := context.AfterFunc(ctx, r.body.cancel)
		defer stop()
		n, err := r.body.body.Read(buf)
		return read{n, err}
	}).(read)
	copy(p, buf[:got.n])
	if got.err != nil {
		r.body.close()
	}
	return got.n, got.err
}

func (r *fiberReader) Close() error {
	r.body.close()
	return nil
}

// webResponse is the response net/http gave, its URL the last one
// followed, without a fragment.
func webResponse(request *ClientRequest, res *http.Response, cancel context.CancelFunc) *ClientResponse {
	names := make([]string, 0, len(res.Header))
	for k := range res.Header {
		names = append(names, strings.ToLower(k))
	}
	slices.Sort(names)
	headers := newRecord(len(names))
	setCookies := []Value{}
	for _, k := range names {
		vs := res.Header.Values(k)
		if k == "set-cookie" {
			for _, v := range vs {
				setCookies = append(setCookies, v)
			}
			headers.put(k, vs[len(vs)-1])
			continue
		}
		headers.put(k, strings.Join(vs, ", "))
	}
	body := &netBody{res.Body, cancel}
	runtime.SetFinalizer(body, (*netBody).close)
	var used atomic.Bool
	u := *res.Request.URL
	u.Fragment, u.RawFragment = "", ""
	href := u.String()
	if w, ok := parseURL(href); ok {
		href = w.href()
	}
	return &ClientResponse{
		request: request, url: href, status: int32(res.StatusCode),
		headers: headers, cookies: jarOf(CookiesFromSetCookie(setCookies)),
		open: func(fb *Fiber) (io.ReadCloser, error) {
			if used.Swap(true) {
				return nil, errString("Body is unusable: Body has already been read")
			}
			return &fiberReader{fb, body}, nil
		},
	}
}

// FetchClientLayer is the service `Http.Client.HttpClient` of the client.
func FetchClientLayer(key Value) Value {
	return LayerSucceed(key, clientImpl(clientOfTransport(fetchTransport)))
}
