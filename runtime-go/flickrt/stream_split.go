package flickrt

import (
	"sync"
)

// Streams split for several consumers, as Effect's (Stream.ts): hubs of
// takes, shared runs, partitions and groups, and their bindings. A take
// (Effect's `Take`) is a chunk, or the `*Exit` the source ended with; the
// source runs in a fiber of the scope the split belongs to.

// takeToPull is Effect's `Take.toPull`.
func takeToPull(t Value) Value {
	if e, ok := t.(*Exit); ok {
		if e.cause == nil {
			return halt(e.value)
		}
		panic(&Failure{e.cause})
	}
	return t
}

// toPubSubTake runs the stream into a new hub of takes in a fiber of the
// owner's scope, ending the hub with how the stream ended.
func toPubSubTake(fb *Fiber, self *Stream, o *Record) *PubSub {
	h := makePubSub(fb, o)
	forkIn(fb, fb.owner, func(fb *Fiber) Value {
		_, c := catch(func() Value {
			return runForEach(fb, self.ch, func(fb *Fiber, chunk Value) { h.publish(fb, chunk) })
		})
		h.end(&Exit{value: Unit, cause: c})
		if c != nil {
			panic(&Failure{c})
		}
		return Unit
	})
	return h
}

// takeOf is the next take of the subscription, as `PubSub.take`.
func takeOf(sub *Subscription) Pull {
	return func(fb *Fiber) Value {
		sub.hub.mu.Lock()
		return takeToPull(sub.take(fb))
	}
}

// streamFromPubSubTake subscribes in the stream's scope when it runs.
func streamFromPubSubTake(h *PubSub) *Stream {
	return &Stream{channelUnwrap(func(fb *Fiber) *Channel { return channelFromPullOf(takeOf(h.subscribe(fb))) })}
}

func StreamBroadcast(fb *Fiber, options, s Value) Value {
	return streamFromPubSubTake(toPubSubTake(fb, streamOf(s), record(options)))
}

// StreamBroadcastN subscribes `n` consumers at once, each in a scope of
// its own that its stream's end closes.
func StreamBroadcastN(fb *Fiber, n, options, s Value) Value {
	h := makePubSub(fb, record(options))
	streams := make([]Value, max(0, int(n.(int32))))
	for i := range streams {
		scope := forkScope(fb, fb.owner)
		sub := h.subscribe(Within(fb, scope))
		streams[i] = &Stream{channelOnExit(channelFromPullOf(takeOf(sub)), func(fb *Fiber, exit *Exit) Value {
			closeScope(fb, scope, exit.cause)
			return Unit
		})}
	}
	source := streamOf(s)
	forkIn(fb, fb.owner, func(fb *Fiber) Value {
		_, c := catch(func() Value {
			return runForEach(fb, source.ch, func(fb *Fiber, chunk Value) { h.publish(fb, chunk) })
		})
		h.end(&Exit{value: Unit, cause: c})
		if c != nil {
			panic(&Failure{c})
		}
		return Unit
	})
	return streams
}

// shared is the RcRef `share` keeps its broadcast in: acquired with the
// first consumer, released `idle` after the last one ended.
type shared struct {
	mu      sync.Mutex
	stream  *Stream
	scope   *Owner
	refs    int
	version int // bumped by each acquisition, so a pending release knows it is stale
}

func (r *shared) release(fb *Fiber) {
	r.mu.Lock()
	scope := r.scope
	r.stream, r.scope = nil, nil
	r.mu.Unlock()
	if scope != nil {
		closeScope(fb, scope, nil)
	}
}

// StreamShare broadcasts the source while consumers run, as Effect's
// `unwrap(RcRef.get(ref))` of an RcRef of `broadcast`.
func StreamShare(fb *Fiber, options, idle, s Value) Value {
	o, source, parent := record(options), streamOf(s), fb.owner
	ttl, hasTTL := some(idle)
	r := &shared{}
	parent.add(fb, func(fb *Fiber, _ *Cause) Value {
		r.release(fb)
		return Unit
	})
	return &Stream{channelUnwrap(func(fb *Fiber) *Channel {
		r.mu.Lock()
		if r.stream == nil {
			r.scope = forkScope(fb, parent)
			r.stream = streamFromPubSubTake(toPubSubTake(Within(fb, r.scope), source, o))
		}
		r.refs++
		r.version++
		stream := r.stream
		r.mu.Unlock()
		fb.owner.add(fb, func(fb *Fiber, _ *Cause) Value {
			r.mu.Lock()
			r.refs--
			idle, version := r.refs == 0, r.version
			r.mu.Unlock()
			switch {
			case !idle:
			case !hasTTL || !DurationIsPositive(ttl).(bool):
				r.release(fb)
			default:
				forkIn(fb, parent, func(fb *Fiber) Value {
					sleepFor(fb, dur(ttl))
					r.mu.Lock()
					stale := r.refs > 0 || r.version != version
					r.mu.Unlock()
					if !stale {
						r.release(fb)
					}
					return Unit
				})
			}
			return Unit
		})
		return stream.ch
	})}
}

// partitionQueue runs the source into two queues of `capacity` (< 0:
// unbounded), what the filter passes and what it refuses, in a fiber of
// the owner's scope; how the source ends ends both.
func partitionQueue(fb *Fiber, self *Stream, filter func(fb *Fiber, a Value) Value, capacity int) (*Queue, *Queue) {
	scope := fb.owner
	pull := toPullScoped(fb, self.ch, scope)
	if capacity < 0 {
		capacity = unboundedCapacity
	}
	passes, fails := newQueue(capacity, queueSuspend), newQueue(capacity, queueSuspend)
	offerUnsafe := func(q *Queue, xs []Value) []Value {
		q.mu.Lock()
		defer q.mu.Unlock()
		return q.offerAllLocked(xs)
	}
	fibers := runIn(fb, scope)
	forkIn(fb, scope, func(fb *Fiber) Value {
		_, c := catch(func() Value {
			for {
				var excluded, satisfying []Value
				for _, a := range pull(fb).([]Value) {
					if r := filter(fb, a); isOk(r) {
						satisfying = append(satisfying, value(r))
					} else {
						excluded = append(excluded, value(r))
					}
				}
				var passing *child
				if len(satisfying) > 0 {
					if left := offerUnsafe(passes, satisfying); len(left) > 0 {
						passing = fibers.fork(fb, func(fb *Fiber) Value { return passes.offerAll(fb, left) })
					}
				}
				if len(excluded) > 0 {
					if left := offerUnsafe(fails, excluded); len(left) > 0 {
						fails.offerAll(fb, left)
					}
				}
				if passing != nil {
					wait(fb, passing.done)
					passing.result()
				}
			}
		})
		passes.failCause(c)
		fails.failCause(c)
		panic(&Failure{c})
	})
	return passes, fails
}

// streamFromQueueShutdownOnEnd takes from the queue, shutting it down
// when the stream ends.
func streamFromQueueShutdownOnEnd(q *Queue) *Stream {
	return &Stream{fromTransform(func(fb *Fiber, _ Pull, scope *Owner) Pull {
		scope.add(fb, func(*Fiber, *Cause) Value { q.shutdown(); return Unit })
		return q.takeAll
	})}
}

func partitionStreams(passes, fails *Queue) Value {
	return []Value{streamFromQueueShutdownOnEnd(passes), streamFromQueueShutdownOnEnd(fails)}
}

func StreamPartition(fb *Fiber, filter, s Value) Value {
	return partitionStreams(partitionQueue(fb, streamOf(s), func(fb *Fiber, a Value) Value { return Apply(fb, filter, a) }, 16))
}

func StreamPartitionEffect(fb *Fiber, filter, s Value) Value {
	results := streamMapEffect(streamOf(s), func(fb *Fiber, a Value) Value { return Run(fb, Apply(fb, filter, a)) }, 1, false)
	return partitionStreams(partitionQueue(fb, results, func(_ *Fiber, r Value) Value { return r }, streamChunkSize))
}

func StreamPartitionQueue(fb *Fiber, filter, s Value) Value {
	passes, fails := partitionQueue(fb, streamOf(s), func(fb *Fiber, a Value) Value { return Apply(fb, filter, a) }, streamChunkSize)
	return []Value{passes, fails}
}

// groups is the queues of a group by, by key in the order they came.
//
// shortcut: keys compared by `==` in a list, where Effect hashes them in a MutableHashMap; hash them if a program groups by many keys
type groups struct {
	mu     sync.Mutex // the source's fiber adds groups while the end may close them
	keys   []Value
	queues []*Queue
}

func (g *groups) get(k Value) *Queue {
	g.mu.Lock()
	defer g.mu.Unlock()
	for i, key := range g.keys {
		if Equals(key, k) {
			return g.queues[i]
		}
	}
	return nil
}

// streamGroupBy is Effect's `groupByImpl` with groups that live as long
// as the source runs: each new key's queue goes out with its key as a
// stream, and ends once the source has.
func streamGroupBy(self *Stream, f func(fb *Fiber, chunk []Value, queue func(fb *Fiber, k Value) *Queue)) *Stream {
	return streamTransformPullBracket(self, func(fb *Fiber, pull Pull, scope, forked *Owner) Pull {
		out := newQueue(unboundedCapacity, queueSuspend)
		scope.add(fb, func(*Fiber, *Cause) Value { out.shutdown(); return Unit })
		g := &groups{}
		forked.add(fb, func(*Fiber, *Cause) Value {
			g.mu.Lock()
			queues := g.queues
			g.mu.Unlock()
			for _, q := range queues {
				q.end()
			}
			return Unit
		})
		queue := func(fb *Fiber, k Value) *Queue {
			if q := g.get(k); q != nil {
				return q
			}
			q := newQueue(streamChunkSize, queueSuspend)
			g.mu.Lock()
			g.keys, g.queues = append(g.keys, k), append(g.queues, q)
			g.mu.Unlock()
			out.offer(fb, []Value{k, streamFromQueueShutdownOnEnd(q)})
			return q
		}
		forkIn(fb, scope, func(fb *Fiber) Value {
			_, c := catch(func() Value {
				for {
					f(fb, pull(fb).([]Value), queue)
				}
			})
			out.failCause(c)
			return Unit
		})
		return out.takeAll
	})
}

func StreamGroupBy(f, s Value) Value {
	return streamGroupBy(streamOf(s), func(fb *Fiber, chunk []Value, queue func(*Fiber, Value) *Queue) {
		for _, a := range chunk {
			pair := List(Run(fb, Apply(fb, f, a)))
			queue(fb, pair[0]).offer(fb, pair[1])
		}
	})
}

// StreamGroupByKey offers the elements of each key in a chunk at once.
func StreamGroupByKey(f, s Value) Value {
	return streamGroupBy(streamOf(s), func(fb *Fiber, chunk []Value, queue func(*Fiber, Value) *Queue) {
		var keys []Value
		var values [][]Value
		for _, a := range chunk {
			k := Apply(fb, f, a)
			i := 0
			for i < len(keys) && !Equals(keys[i], k) {
				i++
			}
			if i == len(keys) {
				keys = append(keys, k)
				values = append(values, nil)
			}
			values[i] = append(values[i], a)
		}
		for i, k := range keys {
			queue(fb, k).offerAll(fb, values[i])
		}
	})
}
