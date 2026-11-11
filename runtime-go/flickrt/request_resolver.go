package flickrt

// RequestResolver: the batching of a resolver, and its combinators, which
// make a resolver of the same batching otherwise.

import "slices"

// defaultBatchKey is the one key of a resolver that batches all its
// entries together.
var defaultBatchKey = new(byte)

func collectAll(*Fiber, []*requestEntry) bool { return true }

// newResolver batches every entry together, after a yield.
func newResolver(runAll func(fb *Fiber, entries []Value, key Value)) *resolver {
	return &resolver{
		delay:        yieldTurn,
		batchKey:     func(*Fiber, *requestEntry) Value { return defaultBatchKey },
		collectWhile: collectAll,
		runAll:       runAll,
	}
}

// running is `runAll` of the Flick function `f` of a batch.
func running(f Value) func(*Fiber, []Value, Value) {
	return func(fb *Fiber, entries []Value, _ Value) { Run(fb, Apply(fb, f, entries)) }
}

// runningKeyed is `runAll` of the Flick function `f` of a batch and its key.
func runningKeyed(f Value) func(*Fiber, []Value, Value) {
	return func(fb *Fiber, entries []Value, key Value) { Run(fb, Apply(fb, Apply(fb, f, entries), key)) }
}

// batchKeyOf is a batch key of the Flick function `f` of an entry.
func batchKeyOf(f Value) func(*Fiber, *requestEntry) Value {
	return func(fb *Fiber, e *requestEntry) Value { return Apply(fb, f, e) }
}

// yielding is a batch's delay that yields first, as the JS adapter's.
func yielding(delay Value) func(*Fiber) {
	return func(fb *Fiber) {
		yieldTurn(fb)
		Run(fb, delay)
	}
}

func RequestResolverMake(runAll Value) Value { return newResolver(running(runAll)) }

func RequestResolverMakeWith(options Value) Value {
	o := record(options)
	r := newResolver(runningKeyed(o.Get("runAll")))
	r.batchKey = batchKeyOf(o.Get("batchKey"))
	if f, ok := some(o.Get("preCheck")); ok {
		r.preCheck = func(fb *Fiber, e *requestEntry) bool { return Apply(fb, f, e).(bool) }
	}
	r.delay = yielding(o.Get("delay"))
	collect := o.Get("collectWhile")
	r.collectWhile = func(fb *Fiber, es []*requestEntry) bool { return Apply(fb, collect, batchEntries(es)).(bool) }
	return r
}

func RequestResolverMakeGrouped(options Value) Value {
	o := record(options)
	r := newResolver(runningKeyed(o.Get("resolver")))
	r.batchKey, r.equalKeys = batchKeyOf(o.Get("key")), true
	return r
}

func RequestResolverFromFunction(f Value) Value {
	return newResolver(func(fb *Fiber, entries []Value, _ Value) {
		for _, e := range entries {
			entryOf(e).complete(&Exit{value: Apply(fb, f, e)})
		}
	})
}

func RequestResolverFromFunctionBatched(f Value) Value {
	return newResolver(func(fb *Fiber, entries []Value, _ Value) {
		for i, v := range List(Apply(fb, f, entries)) {
			if i < len(entries) {
				entryOf(entries[i]).complete(&Exit{value: v})
			}
		}
	})
}

// RequestResolverFromEffect answers each entry in a fiber of its own.
func RequestResolverFromEffect(f Value) Value {
	return newResolver(func(fb *Fiber, entries []Value, _ Value) {
		owner := newOwner()
		children := make([]*child, len(entries))
		for i, e := range entries {
			children[i] = fork(fb, owner, Apply(fb, f, e), func(c *child) { entryOf(e).complete(c.exit()) })
		}
		_, cause := catch(func() Value {
			for _, c := range children {
				wait(fb, c.done)
			}
			return Unit
		})
		if cause != nil {
			stop(fb, children...)
			panic(&Failure{cause})
		}
	})
}

var RequestResolverNever Value = newResolver(func(fb *Fiber, _ []Value, _ Value) { EffectNever(fb) })

func RequestResolverSetDelayEffect(delay, self Value) Value {
	return resolverOf(self).with(func(r *resolver) { r.delay = yielding(delay) })
}

func RequestResolverSetDelay(d, self Value) Value {
	return resolverOf(self).with(func(r *resolver) { r.delay = func(fb *Fiber) { sleepFor(fb, dur(d)) } })
}

// RequestResolverAround runs `before` ahead of each batch and `after` with
// its result, uninterruptibly, once the batch is resolved.
func RequestResolverAround(before, after, self Value) Value {
	inner := resolverOf(self).runAll
	return resolverOf(self).with(func(r *resolver) {
		r.runAll = func(fb *Fiber, entries []Value, key Value) {
			outer := fb.state.mask
			a := masked(fb, outer+1, func() Value { return Run(fb, Apply(fb, before, entries)) })
			_, cause := catch(func() Value {
				return masked(fb, outer, func() Value { inner(fb, entries, key); return Unit })
			})
			_, released := catch(func() Value {
				return masked(fb, outer+1, func() Value { return Run(fb, Apply(fb, Apply(fb, after, entries), a)) })
			})
			if cause = finalized(cause, released); cause != nil { // then the failure of `after`
				panic(&Failure{cause})
			}
		}
	})
}

func RequestResolverBatchN(n, self Value) Value {
	size := max(int(n.(int32)), 1)
	return resolverOf(self).with(func(r *resolver) {
		r.collectWhile = func(_ *Fiber, es []*requestEntry) bool { return len(es) < size }
	})
}

func RequestResolverGrouped(f, self Value) Value {
	return resolverOf(self).with(func(r *resolver) { r.batchKey, r.equalKeys = batchKeyOf(f), true })
}

// RequestResolverRace runs both resolvers on each batch, batching as
// `make` does; the first to succeed wins.
func RequestResolverRace(that, self Value) Value {
	a, b := resolverOf(self).runAll, resolverOf(that).runAll
	return newResolver(func(fb *Fiber, entries []Value, key Value) {
		RaceSuccess(fb,
			Eff(func(fb *Fiber) Value { a(fb, entries, key); return Unit }),
			Eff(func(fb *Fiber) Value { b(fb, entries, key); return Unit }))
	})
}

// RequestResolverWithSpan runs each batch in a span of the name, with the
// batch's size as its first attribute and links to the spans of the
// requesting fibers, each once.
func RequestResolverWithSpan(name, options, self Value) Value {
	inner := resolverOf(self).runAll
	o := record(options)
	return resolverOf(self).with(func(r *resolver) {
		r.runAll = func(fb *Fiber, entries []Value, key Value) {
			links := append([]Value{}, List(o.Get("links"))...)
			var seen []Value
			for _, e := range entries {
				s, ok := some(entryOf(e).span)
				if !ok || slices.Contains(seen, s) {
					continue
				}
				seen = append(seen, s)
				links = append(links, Rec("span", s, "attributes", []Value{}))
			}
			attributes := append([]Value{[]Value{"batchSize", int32(len(entries))}}, List(o.Get("attributes"))...)
			spanned := o.set("links", links).set("attributes", attributes)
			TracerWithSpan(fb, name, spanned, Eff(func(fb *Fiber) Value { inner(fb, entries, key); return Unit }))
		}
	})
}

// A cachedRequest is a request `with_cache` has seen: its first entry, how
// it ended, and the entries waiting for it meanwhile.
type cachedRequest struct {
	entry   *requestEntry
	exit    *Exit
	pending []*requestEntry
}

// RequestResolverWithCache answers a request equal to a cached one from
// the cache, keeping at most `capacity`, dropped by `strategy`.
func RequestResolverWithCache(_ *Fiber, options, self Value) Value {
	o := record(options)
	capacity := int(o.Get("capacity").(int32))
	lru := true
	if s, ok := some(o.Get("strategy")); ok {
		lru = s.(string) == "lru"
	}
	var cache ordMap[*cachedRequest]
	inner := resolverOf(self).runAll
	return resolverOf(self).with(func(r *resolver) {
		r.runAll = func(fb *Fiber, entries []Value, key Value) {
			defer func() {
				requests.Lock()
				if over := len(cache.keys) - capacity; over > 0 {
					cache.keys, cache.vals = cache.keys[over:], cache.vals[over:]
				}
				requests.Unlock()
			}()
			inner(fb, entries, key)
		}
		r.preCheck = func(_ *Fiber, e *requestEntry) bool {
			requests.Lock()
			i := cache.find(e.request)
			if i < 0 {
				cached := &cachedRequest{entry: e}
				cache.set(e.request, cached)
				previous := e.complete
				e.complete = func(exit *Exit) {
					requests.Lock()
					if exit.cause.has(causeInterrupt) {
						if j := cache.find(e.request); cached.exit == nil && j >= 0 && cache.vals[j] == cached {
							cache.removeAt(j)
						}
					} else {
						cached.exit = exit
					}
					pending := cached.pending
					cached.pending = nil
					requests.Unlock()
					previous(exit)
					for _, p := range pending {
						p.complete(exit)
					}
				}
				requests.Unlock()
				return true
			}
			cached := cache.vals[i]
			if cached.exit == nil {
				cached.entry.uninterruptible = true
				cached.pending = append(cached.pending, e)
				requests.Unlock()
				return false
			}
			if lru {
				cache.touch(i)
			}
			exit := cached.exit
			requests.Unlock()
			e.complete(exit)
			return false
		}
	})
}

// RequestResolverAsCache is a Cache whose lookup of a request runs it
// through the resolver.
func RequestResolverAsCache(fb *Fiber, options, self Value) Value {
	o := record(options)
	lookup := Fn(func(_ *Fiber, request Value) Value {
		return Eff(func(fb *Fiber) Value { return RequestRun(fb, self, request) })
	})
	return newCache(fb, lookup, o.Get("capacity"), entryTTL(o.Get("timeToLive"), true))
}
