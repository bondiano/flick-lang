package flickrt

import (
	"bufio"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"
)

// gatedStream is the chunks "a" and then "b", the second once `gate` closes.
func gatedStream(gate <-chan struct{}) *Stream {
	return &Stream{open: func(*Fiber) pull {
		n := 0
		return func(fb *Fiber) ([]Value, bool) {
			n++
			switch n {
			case 1:
				return []Value{byteList([]byte("a"))}, true
			case 2:
				wait(fb, gate)
				return []Value{byteList([]byte("b"))}, true
			}
			return nil, false
		}
	}}
}

func TestServerStreamsChunksAsTheyCome(t *testing.T) {
	gate := make(chan struct{})
	owned(rootFiber(), func(fb *Fiber, _ *Owner) Value {
		handler := Fn(func(fb *Fiber, _ Value) Value {
			return Eff(func(*Fiber) Value { return ResponseStream(gatedStream(gate), noOptions) })
		})
		server := HttpServe(fb, Rec("host", "127.0.0.1", "port", 0.0, "gracefulShutdownTimeout", None), handler).(*HttpServer)
		res, err := http.Get("http://" + server.listener.Addr().String())
		if err != nil {
			t.Fatal(err)
		}
		defer res.Body.Close()
		r := bufio.NewReader(res.Body)
		if c, err := r.ReadByte(); err != nil || c != 'a' {
			t.Fatalf("first chunk %q %v", c, err)
		}
		close(gate)
		if c, err := r.ReadByte(); err != nil || c != 'b' {
			t.Fatalf("second chunk %q %v", c, err)
		}
		return Unit
	})
}

func TestClientReadsTheBodyAsItComes(t *testing.T) {
	gate := make(chan struct{})
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Write([]byte("a"))
		w.(http.Flusher).Flush()
		<-gate
		w.Write([]byte("b"))
	}))
	defer server.Close()
	defer close(gate)
	owned(rootFiber(), func(fb *Fiber, _ *Owner) Value {
		response := HttpClientExecute(fb, clientOfTransport(fetchTransport), ClientRequestGet(server.URL))
		next := ClientResponseStream(response).(*Stream).open(fb)
		chunk, ok := next(fb)
		if !ok || byteString(chunk[0]) != "a" {
			t.Fatalf("first chunk %v", chunk)
		}
		// an interruption stops a read the server holds back
		go func() {
			time.Sleep(10 * time.Millisecond)
			fb.state.interrupt(0)
		}()
		if _, cause := catch(func() Value { next(fb); return nil }); cause == nil || !cause.has(causeInterrupt) {
			t.Errorf("the read ends with the interruption, not %v", cause)
		}
		return Unit
	})
}

func TestDecodeTextAcrossChunks(t *testing.T) {
	var d textDecoder
	got := d.decode([]byte{0xef, 0xbb}) + d.decode([]byte{0xbf, 'a', 0xc3}) + d.decode([]byte{0xa9, 0xff})
	if got != "aé�" {
		t.Errorf("decoded %q", got)
	}
}
