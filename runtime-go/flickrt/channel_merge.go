package flickrt

import (
	"math"
	"slices"
	"sync"
)

// Merging channels, as Effect's `mergeAll` and `merge` (Channel.ts:8353,
// 8701): each channel runs in a fiber of its own and offers what it emits
// to a queue the merged pull takes from; they share the upstream one pull
// at a time.

// channelMergeAll runs the channels the channel emits, `concurrency` at
// once; with `switching`, a new one interrupts the oldest instead of
// waiting for room.
func channelMergeAll(channels *Channel, concurrency, bufferSize int, switching bool) *Channel {
	return fromTransformBracket(func(fb *Fiber, upstream Pull, scope, forked *Owner) Pull {
		n := max(1, concurrency)
		var permits *Semaphore
		if !switching {
			permits = &Semaphore{permits: n, wake: make(chan struct{})}
		}
		doneLatch := &Latch{open: true, wake: make(chan struct{})}
		var mu sync.Mutex
		var fibers []*child // running, oldest first
		queue := newQueue(bufferSize, queueSuspend)
		forked.add(fb, func(*Fiber, *Cause) Value { queue.shutdown(); return Unit })
		upstream = serial(upstream)
		pull := channels.transform(fb, upstream, scope)
		running := func() int {
			mu.Lock()
			defer mu.Unlock()
			return len(fibers)
		}
		var pulling *child
		run := func(fb *Fiber, channel *Channel) {
			childScope := forkScope(fb, forked)
			childPull := channel.transform(fb, upstream, childScope)
			mu.Lock()
			for len(fibers) >= n {
				oldest := fibers[0]
				fibers = fibers[1:]
				if len(fibers) == 0 {
					LatchOpen(fb, doneLatch)
				}
				mu.Unlock()
				stop(fb, oldest)
				mu.Lock()
			}
			defer mu.Unlock()
			var f *child
			f = fork(fb, fb.owner, Eff(func(fb *Fiber) Value {
				_, c := catch(func() Value {
					for {
						v := childPull(fb)
						EffectYieldNow(fb)
						queue.offer(fb, v)
					}
				})
				masked(fb, fb.state.mask+1, func() Value {
					exit := doneExit(c)
					_, closed := catch(func() Value { closeScope(fb, childScope, exit.cause); return nil })
					mu.Lock()
					defer mu.Unlock()
					if !slices.Contains(fibers, f) {
						return nil
					}
					if failure := Parallel(exit.cause, closed); failure != nil {
						queue.failCause(failure)
					}
					fibers = withoutOne(fibers, f)
					if permits != nil {
						permits.release(1)
					}
					if len(fibers) == 0 {
						LatchOpen(fb, doneLatch)
					}
					return nil
				})
				panic(&Failure{c})
			}), nil)
			LatchClose(fb, doneLatch)
			fibers = append(fibers, f)
		}
		loop := func(fb *Fiber) {
			for {
				pulling = nil
				if permits != nil {
					if running() < n {
						permits.take(fb, 1)
					} else {
						pulling = fork(fb, fb.owner, Eff(func(fb *Fiber) Value { return pull(fb) }), nil)
						takeOrFail(fb, permits, pulling)
					}
				}
				var channel Value
				if pulling == nil {
					channel = pull(fb)
				} else {
					wait(fb, pulling.done)
					channel = pulling.result()
				}
				run(fb, channelOf(channel))
			}
		}
		forkIn(fb, forked, func(fb *Fiber) Value {
			defer func() {
				mu.Lock()
				left := append([]*child{}, fibers...)
				mu.Unlock()
				if pulling != nil {
					left = append(left, pulling)
				}
				stop(fb, left...)
			}()
			_, c := catch(func() Value { loop(fb); return nil })
			if _, _, ok := filterDone(c); ok {
				LatchAwait(fb, doneLatch)
			}
			queue.failCause(c)
			return Unit
		})
		return queue.take
	})
}

// takeOrFail takes a permit, or fails as the fiber pulling meanwhile
// does.
func takeOrFail(fb *Fiber, permits *Semaphore, pulling *child) {
	joined := pulling.done
	for {
		permits.mu.Lock()
		if permits.permits-permits.taken >= 1 {
			permits.taken++
			permits.mu.Unlock()
			return
		}
		wake := permits.wake
		permits.mu.Unlock()
		var signal <-chan struct{}
		if fb.state.mask == 0 {
			signal = fb.state.signal
		}
		switch firstClosed(fb, wake, joined, signal) {
		case 1:
			if pulling.cause != nil {
				panic(&Failure{pulling.cause})
			}
			joined = nil
		case 2:
			panic(fb.state.stopped())
		}
	}
}

// channelMerge emits what both channels emit as it comes; `strategy` is
// the side whose end ends the merge: "left", "right", "both" or "either".
func channelMerge(left, right *Channel, strategy string) *Channel {
	return fromTransformBracket(func(fb *Fiber, upstream Pull, _, forked *Owner) Pull {
		queue := newQueue(0, queueSuspend)
		forked.add(fb, func(*Fiber, *Cause) Value { queue.shutdown(); return Unit })
		upstream = serial(upstream)
		var mu sync.Mutex
		ended := 0
		onExit := func(side string, c *Cause) {
			mu.Lock()
			ended++
			both := ended == 2
			mu.Unlock()
			if !isDoneCause(c) || strategy == "either" || strategy == side || (strategy == "both" && both) {
				queue.failCause(c)
			}
		}
		runSide := func(side string, channel *Channel, scope *Owner) {
			forkIn(fb, forked, func(fb *Fiber) Value {
				_, c := catch(func() Value {
					pull := channel.transform(fb, upstream, scope)
					for {
						queue.offer(fb, pull(fb))
					}
				})
				masked(fb, fb.state.mask+1, func() Value {
					exit := doneExit(c)
					_, closed := catch(func() Value { closeScope(fb, scope, exit.cause); return nil })
					switch {
					case exit.cause != nil:
						onExit(side, Sequential(exit.cause, closed))
					case closed != nil:
						onExit(side, closed)
					default:
						onExit(side, c)
					}
					return nil
				})
				panic(&Failure{c})
			})
		}
		runSide("left", left, forkScope(fb, forked))
		runSide("right", right, forkScope(fb, forked))
		return queue.take
	})
}

// channelInterruptWhen ends the channel with what the task gives, once it
// gives it.
func channelInterruptWhen(self *Channel, task func(fb *Fiber) Value) *Channel {
	right := fromPull(func(*Fiber) Pull { return func(fb *Fiber) Value { return halt(task(fb)) } })
	return channelMerge(self, right, "either")
}

// ChannelMergeAll runs every inner channel at once when `concurrency` is
// `None`.
func ChannelMergeAll(options, channels Value) Value {
	o := record(options)
	concurrency, bufferSize := math.MaxInt, 16
	if c, ok := some(o.Get("concurrency")); ok {
		concurrency = int(c.(int32))
	}
	if b, ok := some(o.Get("bufferSize")); ok {
		bufferSize = int(b.(int32))
	}
	return channelMergeAll(channelOf(channels), concurrency, bufferSize, o.Get("switch").(bool))
}

func ChannelMerge(strategy, right, left Value) Value {
	return channelMerge(channelOf(left), channelOf(right), strategy.(string))
}

func ChannelInterruptWhen(task, self Value) Value {
	return channelInterruptWhen(channelOf(self), func(fb *Fiber) Value { return Run(fb, task) })
}
