package flickrt

import (
	"bufio"
	"context"
	"crypto/rand"
	"crypto/sha1"
	"crypto/tls"
	"encoding/base64"
	"encoding/binary"
	"errors"
	"io"
	"net"
	"net/http"
	"strconv"
	"strings"
	"sync"
	"time"
)

// WebSockets as RFC 6455 has them, enough for the messages of RPC: text
// and binary messages, fragmented or not, pings answered, a close
// answered and sent with its code. A client masks what it sends.

const wsGUID = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"

// wsMaxMessage is the largest message read: Effect's buffer limit, and
// room for a frame's own bytes.
const wsMaxMessage = rpcMaxBuffer + 1<<16

var errTooBig = errors.New("the message is too big")

type wsConn struct {
	conn   net.Conn
	r      *bufio.Reader
	client bool // a client masks its frames
	wmu    sync.Mutex
	closed bool
}

func wsAccept(key string) string {
	h := sha1.Sum([]byte(key + wsGUID))
	return base64.StdEncoding.EncodeToString(h[:])
}

// writeFrame writes one frame of `op` with `payload`.
func (c *wsConn) writeFrame(op byte, payload []byte) error {
	c.wmu.Lock()
	defer c.wmu.Unlock()
	if c.closed {
		return net.ErrClosed
	}
	head := make([]byte, 2, 14)
	head[0] = 0x80 | op
	n := len(payload)
	switch {
	case n < 126:
		head[1] = byte(n)
	case n <= 0xFFFF:
		head[1] = 126
		head = binary.BigEndian.AppendUint16(head, uint16(n))
	default:
		head[1] = 127
		head = binary.BigEndian.AppendUint64(head, uint64(n))
	}
	if c.client {
		head[1] |= 0x80
		var mask [4]byte
		rand.Read(mask[:])
		head = append(head, mask[:]...)
		masked := make([]byte, n)
		for i := range payload {
			masked[i] = payload[i] ^ mask[i%4]
		}
		payload = masked
	}
	if op == 8 {
		c.closed = true
	}
	_, err := c.conn.Write(append(head, payload...))
	return err
}

// write sends a message: text for JSON, binary for bytes.
func (c *wsConn) write(data []byte, text bool) error {
	if text {
		return c.writeFrame(1, data)
	}
	return c.writeFrame(2, data)
}

// shut sends a close frame of `code` and closes the connection.
func (c *wsConn) shut(code uint16) {
	// A peer that stopped reading cannot hold owner shutdown indefinitely.
	c.conn.SetWriteDeadline(time.Now().Add(time.Second))
	c.writeFrame(8, binary.BigEndian.AppendUint16(nil, code))
	c.conn.Close()
}

// read is the next message, after the control frames before it.
func (c *wsConn) read() ([]byte, error) {
	var message []byte
	for {
		var head [2]byte
		if _, err := io.ReadFull(c.r, head[:]); err != nil {
			return nil, err
		}
		op, n := head[0]&0x0F, uint64(head[1]&0x7F)
		switch n {
		case 126:
			var b [2]byte
			if _, err := io.ReadFull(c.r, b[:]); err != nil {
				return nil, err
			}
			n = uint64(binary.BigEndian.Uint16(b[:]))
		case 127:
			var b [8]byte
			if _, err := io.ReadFull(c.r, b[:]); err != nil {
				return nil, err
			}
			n = binary.BigEndian.Uint64(b[:])
		}
		if n > wsMaxMessage || uint64(len(message))+n > wsMaxMessage {
			c.shut(1009)
			return nil, errTooBig
		}
		var mask [4]byte
		masked := head[1]&0x80 != 0
		if masked {
			if _, err := io.ReadFull(c.r, mask[:]); err != nil {
				return nil, err
			}
		}
		payload := make([]byte, n)
		if _, err := io.ReadFull(c.r, payload); err != nil {
			return nil, err
		}
		if masked {
			for i := range payload {
				payload[i] ^= mask[i%4]
			}
		}
		switch op {
		case 8:
			code := uint16(1000)
			if len(payload) >= 2 {
				code = binary.BigEndian.Uint16(payload)
			}
			c.shut(code)
			return nil, io.EOF
		case 9:
			c.writeFrame(10, payload)
			continue
		case 10:
			continue
		}
		message = append(message, payload...)
		if head[0]&0x80 != 0 {
			return message, nil
		}
	}
}

// websocket answers a GET with the request's connection taken over as a
// WebSocket whose messages carry the calls of its client; the server's
// stopping closes it.
func (s *rpcServer) websocket(ser *rpcSerialization, q *ServerRequest) Value {
	hr := q.src.r
	if hr == nil || !strings.EqualFold(hr.Header.Get("Upgrade"), "websocket") || hr.Header.Get("Sec-WebSocket-Key") == "" {
		return bodyResponse(noOptions, 400, nil, *textBody("not a WebSocket request", "text/plain"))
	}
	headers := headerPairs(q.headerRecord())
	resp := newResponse(101, "", false, nil, nil, nil, false)
	resp.upgrade = func(conn net.Conn, rw *bufio.ReadWriter, stopping <-chan struct{}) {
		accept := wsAccept(hr.Header.Get("Sec-WebSocket-Key"))
		rw.WriteString("HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: " + accept + "\r\n\r\n")
		if rw.Flush() != nil {
			return
		}
		ws := &wsConn{conn: conn, r: rw.Reader}
		ended := make(chan struct{})
		defer close(ended)
		go func() {
			select {
			case <-stopping:
				ws.shut(1000)
			case <-ended:
			}
		}()
		s.serveSocket(ser, headers, ws)
	}
	return resp
}

func (s *rpcServer) websocketHandler(ser *rpcSerialization) Value {
	return Fn(func(fb *Fiber, r Value) Value {
		return Eff(func(*Fiber) Value { return s.websocket(ser, requestOf(r)) })
	})
}

// rpcSocket is a connection whose messages carry calls: a WebSocket, or
// TCP whose bytes the serialization frames.
type rpcSocket interface {
	read() ([]byte, error)
	write(data []byte, text bool) error
	shut(code uint16)
}

type tcpSocket struct {
	conn net.Conn
	buf  []byte
	wmu  sync.Mutex
}

func (t *tcpSocket) read() ([]byte, error) {
	n, err := t.conn.Read(t.buf)
	if n > 0 {
		return append([]byte(nil), t.buf[:n]...), nil
	}
	return nil, err
}

func (t *tcpSocket) write(data []byte, _ bool) error {
	t.wmu.Lock()
	defer t.wmu.Unlock()
	_, err := t.conn.Write(data)
	return err
}

func (t *tcpSocket) shut(uint16) { t.conn.Close() }

// lockedParser is a parser a socket's reader and its writers share.
type lockedParser struct {
	mu sync.Mutex
	p  rpcParser
}

func (l *lockedParser) decode(data []byte) ([]Value, error) {
	l.mu.Lock()
	defer l.mu.Unlock()
	return l.p.decode(data)
}

func (l *lockedParser) encode(msgs []Value, many bool) []byte {
	l.mu.Lock()
	defer l.mu.Unlock()
	return l.p.encode(msgs, many)
}

// serveSocket answers the calls of the socket's messages until it closes,
// each message written as one of its own.
func (s *rpcServer) serveSocket(ser *rpcSerialization, headers []Value, sock rpcSocket) {
	parser := &lockedParser{p: ser.parser()}
	text := !ser.binary
	send := func(m Value) {
		if b := parser.encode([]Value{m}, false); b != nil {
			sock.write(b, text)
		}
	}
	conn := s.connect(ser, headers, true, send, func() { sock.shut(1000) })
	// the calls end, those that cannot be interrupted at their end, before the socket goes
	defer func() {
		conn.abort()
		<-conn.done
	}()
	for {
		data, err := sock.read()
		if err != nil {
			return
		}
		msgs, err := parser.decode(data)
		if errors.Is(err, errMaxBuffer) {
			sock.shut(1009)
			return
		}
		if err != nil {
			conn.defect(errorDefect(err))
		}
		for _, m := range msgs {
			conn.receive(m)
		}
	}
}

// dialSocket opens a client's connection to `url`: `ws://`, `wss://` or
// `tcp://`, in a fiber whose interruption stops it.
func dialSocket(fb *Fiber, transport rpcTransport, rawURL string) (rpcSocket, error) {
	u, ok := parseURL(rawURL)
	if !ok || !u.hasHost {
		return nil, errors.New("Invalid URL: " + rawURL)
	}
	secure := u.scheme == "wss" || u.scheme == "https"
	port := u.port
	if port == "" {
		port = map[bool]string{true: "443", false: "80"}[secure]
	}
	address := net.JoinHostPort(strings.Trim(u.host, "[]"), port)
	type dialed struct {
		sock rpcSocket
		err  error
	}
	got := ForeignAsync(fb, func(ctx Context) Value {
		var d net.Dialer
		conn, err := d.DialContext(ctx, "tcp", address)
		if err != nil {
			return dialed{err: err}
		}
		if transport == "tcp" {
			return dialed{sock: &tcpSocket{conn: conn, buf: make([]byte, 64<<10)}}
		}
		stop := context.AfterFunc(ctx, func() { conn.Close() })
		defer stop()
		if secure {
			tc := tls.Client(conn, &tls.Config{ServerName: strings.Trim(u.host, "[]")})
			if err := tc.HandshakeContext(ctx); err != nil {
				conn.Close()
				return dialed{err: err}
			}
			conn = tc
		}
		ws, err := wsHandshake(conn, u)
		if err != nil {
			conn.Close()
			return dialed{err: err}
		}
		return dialed{sock: ws}
	}).(dialed)
	return got.sock, got.err
}

// wsHandshake asks the server at `u` for a WebSocket over `conn`.
func wsHandshake(conn net.Conn, u *webURL) (*wsConn, error) {
	var nonce [16]byte
	rand.Read(nonce[:])
	key := base64.StdEncoding.EncodeToString(nonce[:])
	path, host := u.path, u.host
	if path == "" {
		path = "/"
	}
	if u.hasQuery {
		path += "?" + u.query
	}
	if u.port != "" {
		host += ":" + u.port
	}
	req := "GET " + path + " HTTP/1.1\r\nHost: " + host + "\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n" +
		"Sec-WebSocket-Key: " + key + "\r\nSec-WebSocket-Version: 13\r\n\r\n"
	if _, err := io.WriteString(conn, req); err != nil {
		return nil, err
	}
	r := bufio.NewReader(conn)
	res, err := http.ReadResponse(r, &http.Request{Method: "GET"})
	if err != nil {
		return nil, err
	}
	if res.StatusCode != 101 || res.Header.Get("Sec-WebSocket-Accept") != wsAccept(key) {
		return nil, errors.New("Unexpected server response: " + strconv.Itoa(res.StatusCode))
	}
	return &wsConn{conn: conn, r: r, client: true}, nil
}

// RpcListenTcp serves the group on TCP until the innermost owner closes.
func RpcListenTcp(fb *Fiber, options, group, handlers Value) Value {
	o := record(options)
	ser := serializationOf(o.Get("serialization"))
	if !ser.framed {
		return Fail(rpcServeError("TCP needs a serialization that frames its messages, such as `Rpc.ndjson`"))
	}
	srv := newRPCServer(fb, groupOf(group), rpcHandlers(groupOf(group), handlers))
	host, port := o.Get("host").(string), int(o.Get("port").(float64))
	l, err := net.Listen("tcp", net.JoinHostPort(host, strconv.Itoa(port)))
	if err != nil {
		return Fail(rpcServeError(err.Error()))
	}
	concurrent.Store(true)
	go func() {
		for {
			conn, err := l.Accept()
			if err != nil {
				return
			}
			go srv.serveSocket(ser, nil, &tcpSocket{conn: conn, buf: make([]byte, 64<<10)})
		}
	}()
	fb.owner.onClose(fb, func(*Fiber) { l.Close() })
	return &rpcListening{float64(l.Addr().(*net.TCPAddr).Port)}
}
