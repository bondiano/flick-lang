package flickrt

import (
	"context"
	"errors"
	"net/http"
	"sync"
	"time"
)

// HTTP requests are external children: admission ends at the server's C1
// hook, and every admitted request finishes its own owner before C5 can run.
type httpRequests struct {
	mu      sync.Mutex
	closing bool
	active  map[*servedRequest]struct{}
	done    chan struct{}
}

type servedRequest struct {
	state   fiberState
	fiber   Fiber
	source  requestSource
	marks   requestMarks
	request ServerRequest
}

func (q *servedRequest) interrupt() { q.state.interrupt(0) }

func newHTTPRequests() *httpRequests {
	return &httpRequests{active: map[*servedRequest]struct{}{}, done: make(chan struct{})}
}

func (g *httpRequests) admit(q *servedRequest) bool {
	g.mu.Lock()
	defer g.mu.Unlock()
	if g.closing {
		return false
	}
	g.active[q] = struct{}{}
	return true
}

func (g *httpRequests) ended(q *servedRequest) {
	g.mu.Lock()
	defer g.mu.Unlock()
	delete(g.active, q)
	if g.closing && len(g.active) == 0 {
		close(g.done)
	}
}

func (g *httpRequests) stop() {
	g.mu.Lock()
	defer g.mu.Unlock()
	if !g.closing {
		g.closing = true
		if len(g.active) == 0 {
			close(g.done)
		}
	}
}

func (g *httpRequests) interrupt() {
	g.mu.Lock()
	active := make([]*servedRequest, 0, len(g.active))
	for q := range g.active {
		active = append(active, q)
	}
	g.mu.Unlock()
	for _, q := range active {
		q.interrupt()
	}
}

func (g *httpRequests) serve(q *servedRequest, w http.ResponseWriter, r *http.Request, handler Value) {
	if !g.admit(q) {
		w.WriteHeader(http.StatusServiceUnavailable)
		return
	}
	defer g.ended(q)
	stop := context.AfterFunc(r.Context(), q.interrupt)
	defer stop()
	_, cause := catch(func() Value {
		return owned(&q.fiber, func(fb *Fiber, _ *Owner) Value {
			return q.respond(fb, w, handler)
		})
	})
	if cause != nil {
		// The response may already have started, including a streamed body.
		panic(http.ErrAbortHandler)
	}
}

func (q *servedRequest) respond(fb *Fiber, w http.ResponseWriter, handler Value) Value {
	out, cause := catch(func() Value { return Run(fb, Apply(fb, handler, &q.request)) })
	if cause != nil {
		out = causeResponse(cause)
	}
	q.marks.mu.Lock()
	pre := len(q.marks.preResponse) > 0
	q.marks.mu.Unlock()
	if pre {
		if sent, failed := catch(func() Value { return q.request.preResponded(fb, &q.marks, out) }); failed == nil {
			out = sent
		} else {
			out = causeResponse(failed)
		}
	}
	// The request owner stays live through pre-response hooks and body writes.
	if writeResponse(w, out.(*ServerResponse), fb) {
		panic(http.ErrAbortHandler)
	}
	return Unit
}

func (g *httpRequests) close(server *http.Server, timeout Value) {
	g.stop()
	ctx := context.Background()
	if grace, finite := httpGrace(timeout); finite {
		var cancel context.CancelFunc
		ctx, cancel = context.WithTimeout(ctx, grace)
		defer cancel()
	}
	err := server.Shutdown(ctx)
	if err != nil {
		g.interrupt()
		server.Close()
	}
	// The grace timeout limits ordinary work, never interruption cleanup.
	<-g.done
	server.Close()
	if err != nil && !errors.Is(err, context.DeadlineExceeded) {
		panic(Defect{err.Error()})
	}
}

func httpGrace(timeout Value) (time.Duration, bool) {
	d, ok := some(timeout)
	if !ok {
		return 20 * time.Second, true
	}
	if dur(d).infinite() {
		return 0, false
	}
	nanos := dur(d).toNanos()
	if !nanos.IsInt64() {
		return 0, false
	}
	return time.Duration(nanos.Int64()), true
}
