package flickrt

import (
	"bufio"
	"io"
	"net"
	"net/http"
	"strings"
	"testing"
)

// orderMarked is the "method target" of each head marked in the input fed
// in the given pieces, and whether the marking still follows.
func orderMarked(pieces ...string) ([]string, bool) {
	c := &orderConn{}
	for _, p := range pieces {
		c.marker.feed(c, []byte(p))
	}
	var heads []string
	for _, h := range c.heads {
		heads = append(heads, h.method+" "+h.target)
	}
	return heads, c.marker.state != orderDone
}

func TestOrderMarkingSplitsHeadsFromBodies(t *testing.T) {
	long := "GET /long HTTP/1.1\r\nX: " + strings.Repeat("a", orderLimit) + "\r\n\r\n"
	for _, c := range []struct {
		name, input string
		heads       []string
		follows     bool
	}{
		{"get", "GET /a HTTP/1.1\r\nHost: x\r\nB: 1\r\n\r\n", []string{"GET /a"}, true},
		{"bare line feeds", "GET /a HTTP/1.1\nHost: x\n\nGET /b HTTP/1.1\n\n", []string{"GET /a", "GET /b"}, true},
		{"content length", "POST /p HTTP/1.1\r\ncontent-length: 19\r\n\r\nGET /x HTTP/1.1\r\n\r\nGET /b HTTP/1.1\r\n\r\n",
			[]string{"POST /p", "GET /b"}, true},
		{"chunked", "POST /c HTTP/1.1\r\nTransfer-Encoding: Chunked\r\nContent-Length: 3\r\n\r\n" +
			"5;ext=\"1\"\r\nGET /\r\n11 \r\nGET /x HTTP/1.1\r\n\r\n0\r\nX-T: 1\r\nX-U: 2\n\r\nGET /d HTTP/1.1\r\n\r\n",
			[]string{"POST /c", "GET /d"}, true},
		{"chunked without trailers", "POST /c HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\nGET /d HTTP/1.1\r\n\r\n",
			[]string{"POST /c", "GET /d"}, true},
		{"continue", "POST /e HTTP/1.1\r\nExpect: 100-continue\r\nContent-Length: 2\r\n\r\nhiGET /f HTTP/1.1\r\n\r\n",
			[]string{"POST /e", "GET /f"}, true},
		{"http/1.0 ignores chunked", "POST /g HTTP/1.0\r\nTransfer-Encoding: chunked\r\n\r\nGET /h HTTP/1.0\r\n\r\n",
			[]string{"POST /g", "GET /h"}, true},
		{"options", "OPTIONS * HTTP/1.1\r\nHost: x\r\n\r\nGET /i HTTP/1.1\r\n\r\n", []string{"OPTIONS *", "GET /i"}, true},
		{"too long a head", long, nil, false},
		{"bad chunk size", "POST /j HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\nzz\r\nGET /k HTTP/1.1\r\n\r\n", []string{"POST /j"}, false},
		{"chunk size ends in a bare line feed", "POST /j HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n1\nx\r\n0\r\n\r\n", []string{"POST /j"}, false},
		{"chunk without its line break", "POST /j HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n1\r\nab\r\n0\r\n\r\n", []string{"POST /j"}, false},
		{"other transfer coding", "POST /j HTTP/1.1\r\nTransfer-Encoding: gzip\r\n\r\n", []string{"POST /j"}, false},
		{"lengths that differ", "POST /j HTTP/1.1\r\nContent-Length: 1\r\nContent-Length: 2\r\n\r\n", nil, false},
		{"upgrade", "GET /ws HTTP/1.1\r\nUpgrade: websocket\r\n\r\nGET /no HTTP/1.1\r\n\r\n", []string{"GET /ws"}, false},
		{"h2 preface", "PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n", nil, false},
		{"empty request line", "\r\nGET /a HTTP/1.1\r\n\r\n", nil, false},
	} {
		check := func(how string, pieces ...string) {
			t.Helper()
			heads, follows := orderMarked(pieces...)
			if strings.Join(heads, "|") != strings.Join(c.heads, "|") || follows != c.follows {
				t.Fatalf("%s, %s: heads %q following %v, want %q %v", c.name, how, heads, follows, c.heads, c.follows)
			}
		}
		check("whole", c.input)
		if c.input == long {
			check("split", c.input[:len(c.input)/2], c.input[len(c.input)/2:])
			continue
		}
		for i := 1; i < len(c.input); i++ {
			check("split at "+c.input[:i], c.input[:i], c.input[i:])
		}
		check("by byte", strings.Split(c.input, "")...)
	}
}

// orderServer answers each request with its header names, in order, and
// its body.
func orderServer(t *testing.T, fb *Fiber) string {
	handler := Fn(func(_ *Fiber, req Value) Value {
		return Eff(func(fb *Fiber) Value {
			q := requestOf(req)
			body, err := q.src.read(fb)
			if err != nil {
				t.Error(err)
			}
			return ResponseText(strings.Join(q.headerRecord().keys, ",")+" "+string(body), noOptions)
		})
	})
	server := HttpServe(fb, Rec("host", "127.0.0.1", "port", 0.0, "gracefulShutdownTimeout", None), handler).(*HttpServer)
	return server.listener.Addr().String()
}

func TestServerKeepsTheOrderOfRequestHeaders(t *testing.T) {
	owned(rootFiber(), func(fb *Fiber, _ *Owner) Value {
		conn, err := net.Dial("tcp", orderServer(t, fb))
		if err != nil {
			t.Fatal(err)
		}
		defer conn.Close()
		// pipelined, with a chunked body holding a request line, and a
		// request net/http answers itself
		io.WriteString(conn, "GET /1 HTTP/1.1\r\nZ-Last: 1\r\nHost: h\r\nA-First: 2\r\nz-last: 3\r\nAccept: */*\r\n\r\n"+
			"GET /2 HTTP/1.1\r\naccept: x\r\nHost: h\r\nCookie: a=1\r\nCookie: b=2\r\n\r\n"+
			"POST /3 HTTP/1.1\r\nHost: h\r\nTransfer-Encoding: chunked\r\nX-B: 1\r\n\r\n11\r\nGET /x HTTP/1.1\r\n\r\n0\r\nT: 1\r\n\r\n"+
			"OPTIONS * HTTP/1.1\r\nHost: h\r\n\r\n"+
			"GET /4 HTTP/1.1\r\nX-Z: 1\r\nHost: h\r\n\r\n")
		r := bufio.NewReader(conn)
		for _, want := range []string{
			"z-last,host,a-first,accept ",
			"accept,host,cookie ",
			"host,transfer-encoding,x-b GET /x HTTP/1.1\r\n",
			"",
			"x-z,host ",
		} {
			res, err := http.ReadResponse(r, nil)
			if err != nil {
				t.Fatal(err)
			}
			body, _ := io.ReadAll(res.Body)
			res.Body.Close()
			if string(body) != want {
				t.Errorf("got %q, want %q", body, want)
			}
		}
		return Unit
	})
}

func BenchmarkOrderMarking(b *testing.B) {
	input := []byte("POST /item/r1 HTTP/1.1\r\nhost: 127.0.0.1:3920\r\nconnection: keep-alive\r\naccept: */*\r\n" +
		"accept-language: *\r\nsec-fetch-mode: cors\r\nuser-agent: node\r\naccept-encoding: gzip, deflate\r\n" +
		"content-type: application/json\r\ncontent-length: 13\r\n\r\n{\"id\": \"r1\"}\n")
	c := &orderConn{}
	b.SetBytes(int64(len(input)))
	b.ReportAllocs()
	for range b.N {
		c.marker.feed(c, input)
		c.heads = c.heads[:0]
	}
}
