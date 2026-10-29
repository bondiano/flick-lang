package flickrt

import (
	"bytes"
	"errors"
	"io"
	"net/http"
	"slices"
	"strings"
)

// Http.FetchClient as Effect's FetchHttpClient, on net/http: the request's
// headers but `Content-Length`, which the transport sets from the body;
// redirects followed up to 20 times as fetch follows them; the response's
// headers by lower-case name in order, their values joined, and its
// cookies of every `Set-Cookie`. An interruption of the fiber cancels the
// request.
//
// shortcut: the body is read with the response, as one; read it as it comes when response streams come to the go target

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
	data, contentType, err := request.body.bytes()
	if err != nil {
		return Fail(requestFailure(transportErrorTag, request, Some(err.Error())))
	}
	type answer struct {
		response *ClientResponse
		failed   bool
	}
	got := ForeignAsync(fb, func(ctx Context) Value {
		var body io.Reader
		if request.body.kind != bodyEmpty {
			body = bytes.NewReader(data)
		}
		req, err := http.NewRequestWithContext(ctx, request.method, urlOf(url).href(), body)
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
		defer res.Body.Close()
		payload, readErr := io.ReadAll(res.Body)
		return answer{response: webResponse(request, res, payload, readErr)}
	}).(answer)
	if got.failed {
		return Fail(requestFailure(transportErrorTag, request, Some("fetch failed")))
	}
	return got.response
}

// webResponse is the response net/http gave, its URL the last one
// followed, without a fragment.
func webResponse(request *ClientRequest, res *http.Response, payload []byte, readErr error) *ClientResponse {
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
	u := *res.Request.URL
	u.Fragment, u.RawFragment = "", ""
	href := u.String()
	if w, ok := parseURL(href); ok {
		href = w.href()
	}
	return &ClientResponse{
		request: request, url: href, status: int32(res.StatusCode),
		headers: headers, cookies: jarOf(CookiesFromSetCookie(setCookies)),
		read: func() ([]byte, error) { return payload, readErr },
	}
}

// FetchClientLayer is the service `Http.Client.HttpClient` of the client.
func FetchClientLayer(key Value) Value {
	return LayerSucceed(key, clientImpl(clientOfTransport(fetchTransport)))
}
