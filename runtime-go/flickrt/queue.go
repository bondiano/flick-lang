package flickrt

import "sync"

// Queue is Effect's queue of messages, as far as the prompts' events need
// it: unbounded, offered to and taken from.
// shortcut: no bounded strategies, ending or shutdown; port the rest of
// `Queue` here when a Go program needs it.
type Queue struct {
	mu    sync.Mutex
	items []Value
	ready chan struct{} // closed by the next offer, for takers that wait
}

func QueueUnbounded() Value { return &Queue{} }

func QueueOffer(v, q Value) Value {
	x := q.(*Queue)
	x.mu.Lock()
	x.items = append(x.items, v)
	if x.ready != nil {
		close(x.ready)
		x.ready = nil
	}
	x.mu.Unlock()
	return true
}

func (q *Queue) poll() (Value, bool) {
	q.mu.Lock()
	defer q.mu.Unlock()
	if len(q.items) == 0 {
		return nil, false
	}
	v := q.items[0]
	q.items = q.items[1:]
	return v, true
}

// take is the first message, waiting for one; interruptible.
func (q *Queue) take(fb *Fiber) Value {
	for {
		q.mu.Lock()
		if len(q.items) > 0 {
			v := q.items[0]
			q.items = q.items[1:]
			q.mu.Unlock()
			return v
		}
		if q.ready == nil {
			q.ready = make(chan struct{})
		}
		ready := q.ready
		q.mu.Unlock()
		wait(fb, ready)
	}
}
