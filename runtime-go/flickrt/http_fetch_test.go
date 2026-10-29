package flickrt

import (
	"net/http"
	"net/http/httptest"
	"testing"
	"time"
)

func TestInterruptionCancelsAFetch(t *testing.T) {
	arrived, cancelled := make(chan struct{}), make(chan struct{})
	server := httptest.NewServer(http.HandlerFunc(func(_ http.ResponseWriter, r *http.Request) {
		close(arrived)
		<-r.Context().Done()
		close(cancelled)
	}))
	defer server.Close()
	owned(rootFiber(), func(fb *Fiber, _ *Owner) Value {
		go func() {
			<-arrived
			fb.state.interrupt(0)
		}()
		_, cause := catch(func() Value {
			return HttpClientExecute(fb, clientOfTransport(fetchTransport), ClientRequestGet(server.URL))
		})
		if cause == nil || !cause.has(causeInterrupt) {
			t.Errorf("the request ends with the interruption, not %v", cause)
		}
		return Unit
	})
	select {
	case <-cancelled:
	case <-time.After(5 * time.Second):
		t.Fatal("the server never saw the request cancelled")
	}
}
