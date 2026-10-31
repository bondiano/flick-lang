package flickrt

import (
	"math"
	"math/rand/v2"
)

// Schedule is Effect 4's: each run of it starts a step function, which
// from the clock's time and an input decides the step's output and the
// delay before the next one, or ends the schedule with a last output.
type Schedule struct {
	step func(fb *Fiber) scheduleStep
}

// scheduleStep is a step of a run of a schedule: `done` is Effect's
// `Cause.done`, the end of the run, with the last output.
type scheduleStep func(fb *Fiber, now float64, input Value) (out Value, delay *Duration, done bool)

func scheduleOf(v Value) *Schedule { return v.(*Schedule) }

// start is Effect's `toStep`: a run whose start failed fails each step.
func (s *Schedule) start(fb *Fiber) scheduleStep {
	step, cause := catch(func() Value { return s.step(fb) })
	if cause != nil {
		return func(*Fiber, float64, Value) (Value, *Duration, bool) { panic(&Failure{cause}) }
	}
	return step.(scheduleStep)
}

// metadata is what a step of a run is told of itself (Effect's Metadata).
type metadata struct {
	input, output                      Value
	duration                           *Duration
	attempt                            int32
	start, now, elapsed, sincePrevious float64
}

func (m metadata) record() Value {
	return Rec("input", m.input, "output", m.output, "duration", m.duration, "attempt", m.attempt,
		"start", m.start, "now", m.now, "elapsed", m.elapsed, "elapsedSincePrevious", m.sincePrevious)
}

// counter is Effect's `metadataFn`: the attempts and times of a run's steps.
type counter struct {
	n               int32
	start, previous float64
}

func (c *counter) next(now float64, input Value) metadata {
	since := 0.0
	if c.n == 0 {
		c.start = now
	} else {
		since = now - c.previous
	}
	c.previous = now
	c.n++
	return metadata{input: input, attempt: c.n, start: c.start, now: now, elapsed: now - c.start, sincePrevious: since}
}

type metaStep func(fb *Fiber, m metadata) (Value, *Duration, bool)

// withMeta is Effect's `fromStepWithMetadata`.
func withMeta(start func() metaStep) *Schedule {
	return &Schedule{func(*Fiber) scheduleStep {
		f, count := start(), &counter{}
		return func(fb *Fiber, now float64, input Value) (Value, *Duration, bool) {
			return f(fb, count.next(now, input))
		}
	}}
}

func stateless(f metaStep) func() metaStep { return func() metaStep { return f } }

// stepAround makes a schedule of `self` whose steps `f` decides from its
// step's, with that step's metadata.
func stepAround(self *Schedule, f func(fb *Fiber, step scheduleStep, meta func() metadata, now float64, input Value) (Value, *Duration, bool)) *Schedule {
	return &Schedule{func(fb *Fiber) scheduleStep {
		step, count := self.start(fb), &counter{}
		return func(fb *Fiber, now float64, input Value) (Value, *Duration, bool) {
			return f(fb, step, func() metadata { return count.next(now, input) }, now, input)
		}
	}}
}

var noDelay = dur(DurationZero)

func ScheduleSpaced(d Value) Value {
	return withMeta(stateless(func(_ *Fiber, m metadata) (Value, *Duration, bool) { return m.attempt - 1, dur(d), false }))
}

var (
	ScheduleForever  = ScheduleSpaced(DurationZero)
	ScheduleOnce     = mapOutput(ScheduleRecurs(int32(1)).(*Schedule), func(*Fiber, metadata) Value { return Unit })
	identitySchedule = &Schedule{func(*Fiber) scheduleStep {
		return func(_ *Fiber, _ float64, input Value) (Value, *Duration, bool) { return input, noDelay, false }
	}}
	passthroughForever = passthrough(ScheduleForever.(*Schedule))
)

func ScheduleIdentity() Value { return identitySchedule }

func ScheduleRecurs(times Value) Value {
	n := times.(int32)
	return whileMeta(ScheduleForever.(*Schedule), func(_ *Fiber, m metadata) bool { return m.attempt <= n })
}

func ScheduleDuration(d Value) Value {
	return withMeta(stateless(func(_ *Fiber, m metadata) (Value, *Duration, bool) {
		if m.attempt == 1 {
			return d, dur(d), false
		}
		return noDelay, nil, true
	}))
}

func ScheduleDuring(d Value) Value {
	limit := millisOf(dur(d))
	return withMeta(stateless(func(_ *Fiber, m metadata) (Value, *Duration, bool) {
		elapsed := fromMillis(m.elapsed)
		return elapsed, noDelay, m.elapsed > limit
	}))
}

func ScheduleExponential(base, factor Value) Value {
	baseMillis := millisOf(dur(base))
	return withMeta(stateless(func(_ *Fiber, m metadata) (Value, *Duration, bool) {
		d := fromMillis(baseMillis * math.Pow(factor.(float64), float64(m.attempt-1)))
		return d, d, false
	}))
}

func ScheduleFibonacci(one Value) Value {
	oneMillis := millisOf(dur(one))
	return &Schedule{func(*Fiber) scheduleStep {
		a, b := 0.0, oneMillis
		return func(*Fiber, float64, Value) (Value, *Duration, bool) {
			next := a + b
			a, b = b, next
			d := fromMillis(next)
			return d, d, false
		}
	}}
}

func ScheduleFixed(interval Value) Value {
	window := millisOf(dur(interval))
	return withMeta(func() metaStep {
		start, lastRun := 0.0, 0.0
		return func(_ *Fiber, m metadata) (Value, *Duration, bool) {
			if window == 0 {
				return m.attempt - 1, noDelay, false
			}
			if m.attempt == 1 {
				start, lastRun = m.now, m.now+window
				return int32(0), fromMillis(window), false
			}
			behind := m.now > lastRun+window
			boundary := window - math.Mod(m.now-start, window)
			delay := boundary
			switch {
			case behind:
				delay = 0
			case boundary == 0:
				delay = window
			}
			lastRun = m.now + delay
			if behind {
				lastRun = m.now
			}
			return m.attempt - 1, fromMillis(delay), false
		}
	})
}

func ScheduleWindowed(interval Value) Value {
	window := millisOf(dur(interval))
	return withMeta(stateless(func(_ *Fiber, m metadata) (Value, *Duration, bool) {
		if window == 0 {
			return m.attempt - 1, noDelay, false
		}
		return m.attempt - 1, fromMillis(window - math.Mod(m.elapsed, window)), false
	}))
}

// mapOutput is Effect's `map`: `f` also maps the last output.
func mapOutput(self *Schedule, f func(*Fiber, metadata) Value) *Schedule {
	return stepAround(self, func(fb *Fiber, step scheduleStep, meta func() metadata, now float64, input Value) (Value, *Duration, bool) {
		out, delay, done := step(fb, now, input)
		m := meta()
		m.output, m.duration = out, delay
		if done {
			m.duration = noDelay
		}
		return f(fb, m), delay, done
	})
}

func ScheduleMap(f, self Value) Value {
	return mapOutput(scheduleOf(self), func(fb *Fiber, m metadata) Value { return Run(fb, Apply(fb, f, m.record())) })
}

// whileMeta is Effect's `while`.
func whileMeta(self *Schedule, f func(*Fiber, metadata) bool) *Schedule {
	return stepAround(self, func(fb *Fiber, step scheduleStep, meta func() metadata, now float64, input Value) (Value, *Duration, bool) {
		out, delay, done := step(fb, now, input)
		if done {
			return out, delay, true
		}
		m := meta()
		m.output, m.duration = out, delay
		return out, delay, !f(fb, m)
	})
}

func ScheduleWhile(f, self Value) Value {
	return whileMeta(scheduleOf(self), func(fb *Fiber, m metadata) bool { return Bool(Run(fb, Apply(fb, f, m.record()))) })
}

func ScheduleTap(f, self Value) Value {
	return stepAround(scheduleOf(self), func(fb *Fiber, step scheduleStep, meta func() metadata, now float64, input Value) (Value, *Duration, bool) {
		out, delay, done := step(fb, now, input)
		if !done {
			m := meta()
			m.output, m.duration = out, delay
			Run(fb, Apply(fb, f, m.record()))
		}
		return out, delay, done
	})
}

// modifyDelay is Effect's: `f` replaces each delay.
func modifyDelay(self *Schedule, f func(*Fiber, metadata) *Duration) *Schedule {
	return stepAround(self, func(fb *Fiber, step scheduleStep, meta func() metadata, now float64, input Value) (Value, *Duration, bool) {
		out, delay, done := step(fb, now, input)
		if done {
			return out, delay, true
		}
		m := meta()
		m.output, m.duration = out, delay
		return out, f(fb, m), false
	})
}

func ScheduleModifyDelay(f, self Value) Value {
	return modifyDelay(scheduleOf(self), func(fb *Fiber, m metadata) *Duration { return dur(Run(fb, Apply(fb, f, m.record()))) })
}

func ScheduleAddDelay(f, self Value) Value {
	return modifyDelay(scheduleOf(self), func(fb *Fiber, m metadata) *Duration {
		return dur(DurationAdd(Run(fb, Apply(fb, f, m.record())), m.duration))
	})
}

// ScheduleJittered scales each delay by a random factor in [0.8, 1.2].
//
// shortcut: the factor comes from Go's global source, not the fiber's Random service, which `Random.with_seed` sets; draw from that service once Random is on the go target
func ScheduleJittered(self Value) Value {
	return modifyDelay(scheduleOf(self), func(_ *Fiber, m metadata) *Duration {
		r, millis := rand.Float64(), millisOf(m.duration)
		return fromMillis(millis*0.8*(1-r) + millis*1.2*r)
	})
}

func passthrough(self *Schedule) *Schedule {
	return stepAround(self, func(fb *Fiber, step scheduleStep, _ func() metadata, now float64, input Value) (Value, *Duration, bool) {
		_, delay, done := step(fb, now, input)
		return input, delay, done
	})
}

func SchedulePassthrough(self Value) Value { return passthrough(scheduleOf(self)) }

// ScheduleConcatResult runs `self` to its end, then `other`: outputs `Err`
// of the first and `Ok` of the second.
func ScheduleConcatResult(other, self Value) Value {
	left := mapOutput(scheduleOf(self), func(_ *Fiber, m metadata) Value { return Err(m.output) })
	right := mapOutput(scheduleOf(other), func(_ *Fiber, m metadata) Value { return Ok(m.output) })
	return &Schedule{func(*Fiber) scheduleStep {
		side := 0
		var current scheduleStep
		var recur scheduleStep
		recur = func(fb *Fiber, now float64, input Value) (Value, *Duration, bool) {
			if current != nil {
				return current(fb, now, input)
			}
			side++
			if side == 1 {
				step := left.start(fb)
				current = func(fb *Fiber, now float64, input Value) (Value, *Duration, bool) {
					out, delay, done := step(fb, now, input)
					if !done {
						return out, delay, false
					}
					current = nil
					return recur(fb, now, input)
				}
				return current(fb, now, input)
			}
			current = right.start(fb)
			return current(fb, now, input)
		}
		return recur
	}}
}

func ScheduleConcat(other, self Value) Value {
	return mapOutput(ScheduleConcatResult(other, self).(*Schedule), func(_ *Fiber, m metadata) Value {
		return m.output.(*Data).Fields[0]
	})
}

// combined is Effect's `max` and `min`: each step runs every schedule's
// and `pick` chooses among their delays, nil where one ended.
func combined(schedules Value, pick func([]*Duration) *Duration) *Schedule {
	all := List(schedules)
	return &Schedule{func(fb *Fiber) scheduleStep {
		steps := make([]scheduleStep, len(all))
		for i, s := range all {
			steps[i] = scheduleOf(s).start(fb)
		}
		return func(fb *Fiber, now float64, input Value) (Value, *Duration, bool) {
			delays := make([]*Duration, len(steps))
			for i, step := range steps {
				if _, delay, done := step(fb, now, input); !done {
					delays[i] = delay
				}
			}
			d := pick(delays)
			if d == nil {
				return noDelay, nil, true
			}
			return d, d, false
		}
	}}
}

func ScheduleMax(schedules Value) Value {
	return combined(schedules, func(ds []*Duration) *Duration {
		most := ds[0]
		for _, d := range ds[1:] {
			if d == nil || most == nil {
				return nil
			}
			if durationOrder(most, d) < 0 {
				most = d
			}
		}
		return most
	})
}

func ScheduleMin(schedules Value) Value {
	return combined(schedules, func(ds []*Duration) *Duration {
		var least *Duration
		for _, d := range ds {
			if d != nil && (least == nil || durationOrder(least, d) > 0) {
				least = d
			}
		}
		return least
	})
}

// ScheduleUpTo is `Schedule.up_to`: the bounds are options.
func ScheduleUpTo(options, self Value) Value {
	o := record(options)
	limit, hasLimit := some(o.Get("duration"))
	times, hasTimes := some(o.Get("times"))
	return whileMeta(scheduleOf(self), func(_ *Fiber, m metadata) bool {
		return (!hasTimes || m.attempt <= times.(int32)) &&
			(!hasLimit || durationOrder(fromMillis(m.elapsed), limit) <= 0)
	})
}
