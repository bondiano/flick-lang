package flickrt

import (
	"context"
	"io"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"
)

func TestHTTPShutdownJoinsRequestCleanup(t *testing.T) {
	for _, disconnect := range []bool{false, true} {
		name := "grace timeout"
		if disconnect {
			name = "client disconnect"
		}
		t.Run(name, func(t *testing.T) { checkHTTPShutdownCleanup(t, disconnect) })
	}
}

func checkHTTPShutdownCleanup(t *testing.T, disconnect bool) {
	t.Helper()
	started := make(chan struct{})
	cleaning := make(chan struct{})
	cleaned := make(chan struct{})
	release := make(chan struct{})
	closed := make(chan struct{})
	clientDone := make(chan struct{})
	go func() {
		defer close(closed)
		owned(rootFiber(), func(fb *Fiber, _ *Owner) Value {
			handler := Fn(func(fb *Fiber, _ Value) Value {
				return Eff(func(fb *Fiber) Value {
					return Scope(fb, func(fb *Fiber, owner, _ Value) Value {
						owner.(*Owner).add(fb, func(*Fiber, *Cause) Value {
							close(cleaning)
							<-release
							close(cleaned)
							return Unit
						})
						close(started)
						wait(fb, make(chan struct{}))
						return ResponseText("done", noOptions)
					})
				})
			})
			server := HttpServe(fb, Rec("host", "127.0.0.1", "port", 0.0,
				"gracefulShutdownTimeout", Some(fromMillis(20))), handler).(*HttpServer)
			ctx := context.Background()
			cancel := func() {}
			if disconnect {
				ctx, cancel = context.WithCancel(ctx)
			}
			go func() {
				defer close(clientDone)
				request, err := http.NewRequestWithContext(ctx, "GET", "http://"+server.listener.Addr().String(), nil)
				if err != nil {
					t.Error(err)
					return
				}
				response, err := http.DefaultClient.Do(request)
				if err == nil {
					io.Copy(io.Discard, response.Body)
					response.Body.Close()
				}
			}()
			<-started
			cancel()
			fb.owner.add(fb, func(*Fiber, *Cause) Value {
				select {
				case <-cleaned:
				default:
					t.Error("outer resources released before request cleanup")
				}
				return Unit
			})
			return Unit
		})
	}()
	select {
	case <-cleaning:
	case <-time.After(3 * time.Second):
		t.Fatal("request interruption did not reach its finalizer")
	}
	select {
	case <-closed:
		t.Error("server owner closed while the request finalizer was still blocked")
	case <-time.After(100 * time.Millisecond):
	}
	close(release)
	select {
	case <-cleaned:
	case <-time.After(3 * time.Second):
		t.Fatal("request finalizer did not finish after release")
	}
	select {
	case <-closed:
	case <-time.After(3 * time.Second):
		t.Fatal("server did not close after the request finalizer completed")
	}
	<-clientDone
}

func TestHTTPRequestsHaveTheirOwnResourceOwner(t *testing.T) {
	released := make(chan struct{})
	owned(rootFiber(), func(fb *Fiber, parent *Owner) Value {
		handler := Fn(func(fb *Fiber, _ Value) Value {
			return Eff(func(fb *Fiber) Value {
				if fb.owner == parent {
					t.Error("request acquisitions belong to the server's owner")
				}
				fb.owner.add(fb, func(*Fiber, *Cause) Value { close(released); return Unit })
				return ResponseText("done", noOptions)
			})
		})
		server := HttpServe(fb, Rec("host", "127.0.0.1", "port", 0.0,
			"gracefulShutdownTimeout", None), handler).(*HttpServer)
		response, err := http.Get("http://" + server.listener.Addr().String())
		if err != nil {
			t.Fatal(err)
		}
		io.Copy(io.Discard, response.Body)
		response.Body.Close()
		select {
		case <-released:
		case <-time.After(3 * time.Second):
			t.Error("request resources were not released while the server stayed open")
		}
		return Unit
	})
}

func TestHTTPClosingRejectsNewRequests(t *testing.T) {
	requests := newHTTPRequests()
	requests.stop()
	response := httptest.NewRecorder()
	requests.serve(&servedRequest{}, response, httptest.NewRequest("GET", "/", nil),
		Fn(func(*Fiber, Value) Value { t.Error("a late request ran"); return Unit }))
	if response.Code != http.StatusServiceUnavailable {
		t.Errorf("a late request returns 503, got %d", response.Code)
	}
}

func TestHTTPGraceDistinguishesDefaultZeroAndInfinite(t *testing.T) {
	for _, c := range []struct {
		timeout Value
		grace   time.Duration
		finite  bool
	}{
		{None, 20 * time.Second, true},
		{Some(DurationZero), 0, true},
		{Some(DurationInfinity), 0, false},
	} {
		if grace, finite := httpGrace(c.timeout); grace != c.grace || finite != c.finite {
			t.Errorf("timeout %v: got %v, %v, want %v, %v", c.timeout, grace, finite, c.grace, c.finite)
		}
	}
}
