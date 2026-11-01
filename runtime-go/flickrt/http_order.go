package flickrt

// The order of a served request's headers, which net/http does not keep
// and Node's `req.headers` does: each connection's bytes are marked as
// net/http reads them, splitting the stream into request heads and
// bodies by the framing rules net/http applies (Content-Length, chunked
// with its trailers, nothing), and each head is kept until its handler
// takes it. A head net/http answers itself (`OPTIONS *`) is skipped by
// the next one's method and target. On anything the marking cannot
// follow (an upgrade, CONNECT, a framing net/http rejects) it stops for
// the connection, and the headers come in the old order.
// shortcut: only plain TCP HTTP/1.x is marked; TLS, h2 or hijacked connections, when the server has them, need the marking to step aside

import (
	"bytes"
	"context"
	"net"
	"net/http"
	"net/textproto"
	"strconv"
	"strings"
	"sync"
)

type orderListener struct{ net.Listener }

func (l orderListener) Accept() (net.Conn, error) {
	c, err := l.Listener.Accept()
	if tcp, ok := c.(*net.TCPConn); ok {
		return &orderConn{TCPConn: tcp}, nil
	}
	return c, err
}

type orderConnKey struct{}

// orderContext puts the connection in its requests' context.
func orderContext(ctx context.Context, c net.Conn) context.Context {
	if oc, ok := c.(*orderConn); ok {
		return context.WithValue(ctx, orderConnKey{}, oc)
	}
	return ctx
}

// orderConn is a TCP connection whose reads are marked; the embedded
// connection keeps ReadFrom and CloseWrite for net/http.
type orderConn struct {
	*net.TCPConn
	mu     sync.Mutex
	heads  []orderHead // marked and not yet taken
	marker orderMarker // of the reading goroutine alone
}

func (c *orderConn) Read(p []byte) (int, error) {
	n, err := c.TCPConn.Read(p)
	if n > 0 && c.marker.state != orderDone {
		c.marker.feed(c, p[:n])
	}
	return n, err
}

func (c *orderConn) push(h orderHead) {
	c.mu.Lock()
	c.heads = append(c.heads, h)
	c.mu.Unlock()
}

// pop is the head of the request of the method and target, dropping the
// heads before it, which reached no handler; "" without one.
func (c *orderConn) pop(method, target string) string {
	c.mu.Lock()
	defer c.mu.Unlock()
	for i, h := range c.heads {
		if h.method == method && h.target == target {
			c.heads = c.heads[i+1:]
			return h.raw
		}
	}
	return ""
}

// requestHead is the raw head of a served request, "" when unmarked.
func requestHead(r *http.Request) string {
	c, ok := r.Context().Value(orderConnKey{}).(*orderConn)
	if !ok || r.ProtoMajor != 1 {
		return ""
	}
	return c.pop(r.Method, r.RequestURI)
}

type orderHead struct {
	raw, method, target string
}

const (
	orderHeadLines = iota // the request line and header lines
	orderBody             // `left` bytes of a body
	orderChunkLine        // a chunk's size line
	orderChunkData        // `left` bytes of a chunk
	orderChunkEnd         // the CRLF after a chunk, `left` bytes of it
	orderTrailer          // the trailer lines
	orderDone             // not followed any more
)

// orderLimit is the most of a head net/http reads, MaxHeaderBytes and its
// slop.
const orderLimit = http.DefaultMaxHeaderBytes + 4096

type orderMarker struct {
	state int
	left  int64
	buf   []byte // the head so far, or the chunk or trailer line
	line  int    // where the head's current line starts
	// the framing headers of the head
	length, encoding string
	lengths, codings int
	upgrade          bool
}

func (m *orderMarker) feed(c *orderConn, p []byte) {
	for len(p) > 0 && m.state != orderDone {
		switch m.state {
		case orderBody, orderChunkData:
			n := min(int64(len(p)), m.left)
			p, m.left = p[n:], m.left-n
			if m.left == 0 && m.state == orderBody {
				m.state = orderHeadLines
			} else if m.left == 0 {
				m.state, m.left = orderChunkEnd, 2
			}
		case orderChunkEnd:
			if p[0] != "\r\n"[2-m.left] {
				m.state = orderDone
				return
			}
			p, m.left = p[1:], m.left-1
			if m.left == 0 {
				m.state = orderChunkLine
			}
		default:
			i := bytes.IndexByte(p, '\n')
			if i < 0 {
				m.buf = append(m.buf, p...)
				p = nil
			} else {
				m.buf, p = append(m.buf, p[:i+1]...), p[i+1:]
			}
			if len(m.buf) > orderLimit {
				m.state = orderDone
				return
			}
			if i >= 0 {
				m.lineEnded(c)
			}
		}
	}
}

// lineEnded takes the line that ends `buf`.
func (m *orderMarker) lineEnded(c *orderConn) {
	switch m.state {
	case orderChunkLine:
		m.chunkLine(m.buf)
		m.buf = m.buf[:0]
		return
	case orderTrailer:
		if len(orderTrim(m.buf)) == 0 {
			m.state = orderHeadLines
		}
		m.buf = m.buf[:0]
		return
	}
	line := orderTrim(m.buf[m.line:])
	switch {
	case m.line > 0 && len(line) == 0:
		m.headEnded(c)
		return
	case m.line > 0:
		// only three names frame the body: the others are not looked into
		colon := bytes.IndexByte(line, ':')
		if colon != len("content-length") && colon != len("transfer-encoding") && colon != len("upgrade") {
			break
		}
		name, value := line[:colon], bytes.Trim(line[colon+1:], " \t")
		switch {
		case orderIs(name, "content-length"):
			if m.lengths > 0 && m.length != string(value) {
				m.state = orderDone
			}
			m.length = string(value)
			m.lengths++
		case orderIs(name, "transfer-encoding"):
			m.encoding = string(value)
			m.codings++
		case orderIs(name, "upgrade"):
			m.upgrade = true
		}
	}
	m.line = len(m.buf)
}

// headEnded keeps the head in `buf` and follows its body as net/http
// frames it (transfer.go).
func (m *orderMarker) headEnded(c *orderConn) {
	first, _, _ := bytes.Cut(m.buf, []byte("\n"))
	method, rest, ok1 := bytes.Cut(orderTrim(first), []byte(" "))
	target, proto, ok2 := bytes.Cut(rest, []byte(" "))
	raw := string(m.buf)
	length, encoding, lengths, codings, upgrade := m.length, m.encoding, m.lengths, m.codings, m.upgrade
	*m = orderMarker{buf: m.buf[:0], state: m.state}
	if !ok1 || !ok2 || !bytes.HasPrefix(proto, []byte("HTTP/1.")) {
		m.state = orderDone
		return
	}
	c.push(orderHead{raw, raw[:len(method)], raw[len(method)+1 : len(method)+1+len(target)]})
	http11 := string(proto) != "HTTP/1.0"
	switch {
	case upgrade || string(method) == "CONNECT":
		m.state = orderDone
	case http11 && codings > 0:
		if codings > 1 || !strings.EqualFold(encoding, "chunked") {
			m.state = orderDone
		} else {
			m.state = orderChunkLine
		}
	case lengths > 0:
		n, err := strconv.ParseUint(length, 10, 63)
		if err != nil {
			m.state = orderDone
		} else if n > 0 {
			m.state, m.left = orderBody, int64(n)
		}
	}
}

// chunkLine follows a chunk's size line, as net/http's chunked reader
// reads it.
func (m *orderMarker) chunkLine(line []byte) {
	// CRLF, and no CR before it
	if len(line) < 2 || bytes.IndexByte(line, '\r') != len(line)-2 || len(line)-2 >= 4096 {
		m.state = orderDone
		return
	}
	size, _, _ := bytes.Cut(bytes.TrimRight(line[:len(line)-2], " \t"), []byte(";"))
	n, err := strconv.ParseUint(string(size), 16, 64)
	switch {
	case err != nil || len(size) > 16:
		m.state = orderDone
	case n == 0:
		m.state = orderTrailer
	default:
		m.state, m.left = orderChunkData, int64(n)
	}
}

func orderTrim(line []byte) []byte {
	return bytes.TrimSuffix(bytes.TrimSuffix(line, []byte("\n")), []byte("\r"))
}

func orderIs(name []byte, want string) bool { return bytes.EqualFold(name, []byte(want)) }

// orderedHeaders are the headers in the order of the raw head, values
// from net/http's request; nil unless they are all the request has.
func orderedHeaders(r *http.Request, head string) *Record {
	h := newRecord(len(r.Header) + 1)
	found := 0
	_, lines, _ := strings.Cut(head, "\n")
	for lines != "" {
		var line string
		line, lines, _ = strings.Cut(lines, "\n")
		raw, _, ok := strings.Cut(line, ":")
		if !ok || raw == "" || raw[0] == ' ' || raw[0] == '\t' {
			continue
		}
		name := strings.ToLower(raw)
		if h.index(name) >= 0 {
			continue
		}
		switch name {
		case "host":
			if r.Host != "" {
				h.put(name, r.Host)
			}
		case "transfer-encoding":
			if len(r.TransferEncoding) > 0 {
				h.put(name, strings.Join(r.TransferEncoding, ", "))
			}
		default:
			values, ok := r.Header[textproto.CanonicalMIMEHeaderKey(raw)]
			if !ok {
				continue // dropped by net/http, as Content-Length beside chunked
			}
			sep := ", "
			if name == "cookie" {
				sep = "; "
			}
			h.put(name, strings.Join(values, sep))
			found++
		}
	}
	if found != len(r.Header) {
		return nil
	}
	return h
}
