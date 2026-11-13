package flickrt

import "sync"

// Queue is Effect's unbounded `Queue`: messages offered by some tasks and
// taken by others, a take waiting for one.
//
// shortcut: only `unbounded`, `offer` and the take of the prompts' events; port the rest of Effect's Queue (bounds, strategies, ending, failing) as programs use it on Go
type Queue struct {
	mu       sync.Mutex
	messages []Value
	offered  chan struct{} // closed when a message arrives
}

func QueueUnbounded(*Fiber) Value { return &Queue{offered: make(chan struct{})} }

func QueueOffer(_ *Fiber, message, queue Value) Value {
	q := queue.(*Queue)
	q.mu.Lock()
	q.messages = append(q.messages, message)
	close(q.offered)
	q.offered = make(chan struct{})
	q.mu.Unlock()
	return true
}

// take is the next message, waiting for one.
func (q *Queue) take(fb *Fiber) Value {
	for {
		q.mu.Lock()
		if len(q.messages) > 0 {
			m := q.messages[0]
			q.messages = q.messages[1:]
			q.mu.Unlock()
			return m
		}
		offered := q.offered
		q.mu.Unlock()
		wait(fb, offered)
	}
}
