package flickrt

// `HttpApi.Inject.call`: `HttpApi.call` whose request goes to a handler of
// the server instead of the network, as Fastify's `inject` does, so a test
// runs the whole server, routing, decoding and errors, without a socket.

import (
	"io"
	"net/http"
	"net/http/httptest"
	neturl "net/url"
	"strings"
)

// HttpApiInject is `HttpApi.Inject.call handler base endpoint request`.
func HttpApiInject(fb *Fiber, handler, base, endpoint, request Value) Value {
	root := "http://localhost/" + strings.TrimPrefix(base.(string), "/")
	return apiCallWith(fb, root, endpoint, request, func(method, url string, header http.Header, body io.Reader) *received {
		// the request-target the server sees: the path and query
		target := url
		if u, err := neturl.Parse(url); err == nil {
			target = u.RequestURI()
		}
		r := httptest.NewRequest(method, target, body)
		r.Header = header
		source := &requestSource{r: r, marks: &requestMarks{}}
		q := &ServerRequest{src: source, method: r.Method, url: r.RequestURI, originalURL: r.RequestURI}
		out, c := catch(func() Value { return Run(fb, Apply(fb, handler, q)) })
		if c != nil {
			// an error the handler did not answer is what a server would send
			out = causeResponse(c)
		}
		w := httptest.NewRecorder()
		writeResponse(w, out.(*ServerResponse), fb)
		return &received{int32(w.Code), w.Header(), w.Body.Bytes()}
	})
}
