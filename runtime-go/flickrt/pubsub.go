package flickrt

// Effect's `PubSub`, as far as an unbounded hub goes: each subscription
// takes the messages published after it, until its scope closes.
//
// shortcut: unbounded hubs without replay, publish_all, subscribe and the stream of a subscription; port the rest of PubSub (G10e) as programs use it on Go

import "sync"

type pubSub struct {
	mu   sync.Mutex
	subs map[*subscription]bool
}

type subscription struct {
	mu     sync.Mutex
	queue  []Value
	signal chan struct{} // closed when messages come, then replaced
}

func (s *subscription) offer(xs []Value) {
	s.mu.Lock()
	s.queue = append(s.queue, xs...)
	close(s.signal)
	s.signal = make(chan struct{})
	s.mu.Unlock()
}

// take is the messages waiting, waiting for one when there are none.
func (s *subscription) take(fb *Fiber) []Value {
	for {
		s.mu.Lock()
		if len(s.queue) > 0 {
			out := s.queue
			s.queue = nil
			s.mu.Unlock()
			return out
		}
		signal := s.signal
		s.mu.Unlock()
		wait(fb, signal)
	}
}

func PubSubUnbounded(_ *Fiber, _ Value) Value { return &pubSub{subs: map[*subscription]bool{}} }

func PubSubPublishAll(_ *Fiber, xs, hub Value) Value {
	h := hub.(*pubSub)
	h.mu.Lock()
	defer h.mu.Unlock()
	for s := range h.subs {
		s.offer(List(xs))
	}
	return true
}

// PubSubSubscribe subscribes until the scope closes.
func PubSubSubscribe(fb *Fiber, hub Value) Value {
	h := hub.(*pubSub)
	s := &subscription{signal: make(chan struct{})}
	h.mu.Lock()
	h.subs[s] = true
	h.mu.Unlock()
	fb.owner.add(fb, func(*Fiber, *Cause) Value {
		h.mu.Lock()
		delete(h.subs, s)
		h.mu.Unlock()
		return Unit
	})
	return s
}

// StreamFromSubscription is the messages of the subscription, as they come.
func StreamFromSubscription(sub Value) Value {
	s := sub.(*subscription)
	return &Stream{open: func(*Fiber) pull {
		return func(fb *Fiber) ([]Value, bool) { return s.take(fb), true }
	}}
}
