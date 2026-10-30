package flickrt

import "math"

// Schedule is Effect's: each run of it is a step that, for an input and
// the time, gives an output and the delay before the next recurrence, or
// that it is done, with a last output. A step fails as any code does, by
// panicking.
type Schedule struct {
	start func(fb *Fiber) step
}

type step func(fb *Fiber, now float64, input Value) (out Value, delay *Duration, done bool)

func scheduleOf(v Value) *Schedule { return v.(*Schedule) }

// info is Effect's metadata of an attempt, but its output and delay.
type info struct {
	input                         Value
	attempt                       int32
	start, now, elapsed, previous float64
}

// meta counts the attempts of a step, as Effect's metadataFn.
type meta struct {
	n               int32
	start, previous float64
}

func (m *meta) next(now float64, input Value) info {
	if m.n == 0 {
		m.start, m.previous = now, now
	}
	i := info{input: input, start: m.start, now: now, elapsed: now - m.start, previous: now - m.previous}
	m.previous = now
	m.n++
	i.attempt = m.n
	return i
}

// record is the metadata as Flick's `Schedule.Metadata`, keys in the order
// of Effect's object.
func (i info) record(out Value, d *Duration) Value {
	return Rec("input", i.input, "attempt", i.attempt, "start", i.start, "now", i.now,
		"elapsed", i.elapsed, "elapsedSincePrevious", i.previous, "output", out, "duration", d)
}

var zeroDuration = dur(DurationZero)

// millisOf is Effect's Duration.toMillis.
func millisOf(d *Duration) float64 {
	switch d.tag {
	case durationMillis:
		return d.millis
	case durationNanos:
		return bigToFloat(d.nanos) / 1e6
	case durationInfinite:
		return math.Inf(1)
	}
	return math.Inf(-1)
}

// withMeta is Effect's fromStepWithMetadata: each run makes a function of
// each attempt's metadata.
func withMeta(run func() func(fb *Fiber, i info) (Value, *Duration, bool)) Value {
	return &Schedule{func(*Fiber) step {
		var m meta
		f := run()
		return func(fb *Fiber, now float64, input Value) (Value, *Duration, bool) {
			return f(fb, m.next(now, input))
		}
	}}
}

// around is a schedule over `self` whose step `f` wraps the inner one,
// with metadata of its own.
func around(self Value, f func(fb *Fiber, inner step, m *meta, now float64, input Value) (Value, *Duration, bool)) Value {
	s := scheduleOf(self)
	return &Schedule{func(fb *Fiber) step {
		inner := s.start(fb)
		var m meta
		return func(fb *Fiber, now float64, input Value) (Value, *Duration, bool) {
			return f(fb, inner, &m, now, input)
		}
	}}
}

// The constructors of the `Schedule` module.

func ScheduleSpaced(d Value) Value {
	return withMeta(func() func(*Fiber, info) (Value, *Duration, bool) {
		return func(_ *Fiber, i info) (Value, *Duration, bool) { return i.attempt - 1, dur(d), false }
	})
}

var ScheduleForever = ScheduleSpaced(DurationZero)

func ScheduleRecurs(n Value) Value {
	times := n.(int32)
	return whileGo(ScheduleForever, func(_ *Fiber, i info, _ Value, _ *Duration) bool { return i.attempt <= times })
}

var ScheduleOnce = mapGo(ScheduleRecurs(int32(1)), func(*Fiber, info, Value, *Duration) Value { return Unit })

var scheduleIdentity Value = &Schedule{func(*Fiber) step {
	return func(_ *Fiber, _ float64, input Value) (Value, *Duration, bool) { return input, zeroDuration, false }
}}

func ScheduleIdentity() Value { return scheduleIdentity }

func ScheduleFixed(interval Value) Value {
	window := millisOf(dur(interval))
	return withMeta(func() func(*Fiber, info) (Value, *Duration, bool) {
		var start, last float64
		return func(_ *Fiber, i info) (Value, *Duration, bool) {
			if window == 0 {
				return i.attempt - 1, zeroDuration, false
			}
			if i.attempt == 1 {
				start, last = i.now, i.now+window
				return int32(0), fromMillis(window), false
			}
			behind := i.now > last+window
			delay := window - math.Mod(i.now-start, window)
			switch {
			case behind:
				delay = 0
			case delay == 0:
				delay = window
			}
			last = i.now + delay
			return i.attempt - 1, fromMillis(delay), false
		}
	})
}

func ScheduleWindowed(interval Value) Value {
	window := millisOf(dur(interval))
	return withMeta(func() func(*Fiber, info) (Value, *Duration, bool) {
		return func(_ *Fiber, i info) (Value, *Duration, bool) {
			if window == 0 {
				return i.attempt - 1, zeroDuration, false
			}
			return i.attempt - 1, fromMillis(window - math.Mod(i.elapsed, window)), false
		}
	})
}

func ScheduleDuration(d Value) Value {
	return withMeta(func() func(*Fiber, info) (Value, *Duration, bool) {
		return func(_ *Fiber, i info) (Value, *Duration, bool) {
			if i.attempt == 1 {
				return d, dur(d), false
			}
			return DurationZero, nil, true
		}
	})
}

func ScheduleDuring(d Value) Value {
	limit := millisOf(dur(d))
	return withMeta(func() func(*Fiber, info) (Value, *Duration, bool) {
		return func(_ *Fiber, i info) (Value, *Duration, bool) {
			elapsed := fromMillis(i.elapsed)
			if i.elapsed > limit {
				return elapsed, nil, true
			}
			return elapsed, zeroDuration, false
		}
	})
}

func ScheduleExponential(base, factor Value) Value {
	millis, f := millisOf(dur(base)), factor.(float64)
	return withMeta(func() func(*Fiber, info) (Value, *Duration, bool) {
		return func(_ *Fiber, i info) (Value, *Duration, bool) {
			d := fromMillis(millis * math.Pow(f, float64(i.attempt-1)))
			return d, d, false
		}
	})
}

func ScheduleFibonacci(one Value) Value {
	first := millisOf(dur(one))
	return &Schedule{func(*Fiber) step {
		a, b := 0.0, first
		return func(*Fiber, float64, Value) (Value, *Duration, bool) {
			a, b = b, a+b
			d := fromMillis(b)
			return d, d, false
		}
	}}
}

// mapGo is Effect's map with a Go function of the metadata and output.
func mapGo(self Value, f func(fb *Fiber, i info, out Value, d *Duration) Value) Value {
	return around(self, func(fb *Fiber, inner step, m *meta, now float64, input Value) (Value, *Duration, bool) {
		out, d, done := inner(fb, now, input)
		if done {
			d = zeroDuration
		}
		return f(fb, m.next(now, input), out, d), d, done
	})
}

// whileGo is Effect's while with a Go predicate: done, with the output,
// on the first recurrence it refuses.
func whileGo(self Value, f func(fb *Fiber, i info, out Value, d *Duration) bool) Value {
	return around(self, func(fb *Fiber, inner step, m *meta, now float64, input Value) (Value, *Duration, bool) {
		out, d, done := inner(fb, now, input)
		if !done && !f(fb, m.next(now, input), out, d) {
			done = true
		}
		return out, d, done
	})
}

// modifyGo is Effect's modifyDelay with a Go function.
func modifyGo(self Value, f func(fb *Fiber, i info, out Value, d *Duration) *Duration) Value {
	return around(self, func(fb *Fiber, inner step, m *meta, now float64, input Value) (Value, *Duration, bool) {
		out, d, done := inner(fb, now, input)
		if done {
			return out, d, true
		}
		return out, f(fb, m.next(now, input), out, d), false
	})
}

// runWith runs an effectful Flick function of the metadata.
func runWith(f Value) func(*Fiber, info, Value, *Duration) Value {
	return func(fb *Fiber, i info, out Value, d *Duration) Value { return Run(fb, Apply(f, i.record(out, d))) }
}

func ScheduleMap(f, self Value) Value { return mapGo(self, runWith(f)) }

func ScheduleWhile(f, self Value) Value {
	run := runWith(f)
	return whileGo(self, func(fb *Fiber, i info, out Value, d *Duration) bool { return Bool(run(fb, i, out, d)) })
}

func ScheduleTap(f, self Value) Value {
	run := runWith(f)
	return around(self, func(fb *Fiber, inner step, m *meta, now float64, input Value) (Value, *Duration, bool) {
		out, d, done := inner(fb, now, input)
		if !done {
			run(fb, m.next(now, input), out, d)
		}
		return out, d, done
	})
}

func ScheduleModifyDelay(f, self Value) Value {
	run := runWith(f)
	return modifyGo(self, func(fb *Fiber, i info, out Value, d *Duration) *Duration { return dur(run(fb, i, out, d)) })
}

func ScheduleAddDelay(f, self Value) Value {
	run := runWith(f)
	return modifyGo(self, func(fb *Fiber, i info, out Value, d *Duration) *Duration {
		return dur(DurationAdd(run(fb, i, out, d), d))
	})
}

func ScheduleJittered(self Value) Value {
	return modifyGo(self, func(fb *Fiber, _ info, _ Value, d *Duration) *Duration {
		r, millis := randomOf(fb).next(), millisOf(d)
		return fromMillis(millis*0.8*(1-r) + millis*1.2*r)
	})
}

func SchedulePassthrough(self Value) Value {
	return around(self, func(fb *Fiber, inner step, _ *meta, now float64, input Value) (Value, *Duration, bool) {
		_, d, done := inner(fb, now, input)
		return input, d, done
	})
}

// ScheduleUpTo is `Schedule.up_to`: at most `times` recurrences, while no
// more than `duration` has passed.
func ScheduleUpTo(options, self Value) Value {
	r := record(options)
	times, limit := r.Get("times").(*Data), r.Get("duration").(*Data)
	return whileGo(self, func(_ *Fiber, i info, _ Value, _ *Duration) bool {
		return (times.Tag != "Some" || i.attempt <= times.Fields[0].(int32)) &&
			(limit.Tag != "Some" || durationOrder(fromMillis(i.elapsed), limit.Fields[0]) <= 0)
	})
}

// ScheduleConcatResult runs `self` until it is done, then `other`: the
// outputs of the one as `Err`, of the other as `Ok`.
func ScheduleConcatResult(other, self Value) Value {
	left := scheduleOf(mapGo(self, func(_ *Fiber, _ info, out Value, _ *Duration) Value { return Err(out) }))
	right := scheduleOf(mapGo(other, func(_ *Fiber, _ info, out Value, _ *Duration) Value { return Ok(out) }))
	return &Schedule{func(*Fiber) step {
		phase := 0 // not started, on the left, on the right
		var current step
		return func(fb *Fiber, now float64, input Value) (Value, *Duration, bool) {
			if phase == 0 {
				current, phase = left.start(fb), 1
			}
			if phase == 1 {
				out, d, done := current(fb, now, input)
				if !done {
					return out, d, false
				}
				current, phase = right.start(fb), 2
			}
			return current(fb, now, input)
		}
	}}
}

func ScheduleConcat(other, self Value) Value {
	return mapGo(ScheduleConcatResult(other, self), func(_ *Fiber, _ info, out Value, _ *Duration) Value {
		return out.(*Data).Fields[0]
	})
}

// combined is Effect's max (`all` true: done once any is) or min (done
// once all are): each step of every schedule, and the longest or the
// shortest delay of those going on, as the output too.
func combined(schedules Value, all bool) Value {
	in := List(schedules)
	return &Schedule{func(fb *Fiber) step {
		steps := make([]step, len(in))
		for i, s := range in {
			steps[i] = scheduleOf(s).start(fb)
		}
		return func(fb *Fiber, now float64, input Value) (Value, *Duration, bool) {
			var pick *Duration
			stopped := false
			for _, s := range steps {
				_, d, done := s(fb, now, input)
				switch {
				case done:
					stopped = true
				case pick == nil:
					pick = d
				case all && durationOrder(d, pick) > 0, !all && durationOrder(d, pick) < 0:
					pick = d
				}
			}
			if pick == nil || all && stopped {
				return DurationZero, nil, true
			}
			return pick, pick, false
		}
	}}
}

func ScheduleMax(schedules Value) Value { return combined(schedules, true) }
func ScheduleMin(schedules Value) Value { return combined(schedules, false) }

// ScheduleCron recurs at each date the cron expression matches.
func ScheduleCron(c Value) Value {
	return &Schedule{func(*Fiber) step {
		return func(_ *Fiber, now float64, _ Value) (Value, *Duration, bool) {
			if math.IsInf(now, 1) {
				return DurationZero, nil, true
			}
			d := fromMillis(cronNextMillis(c, now) - now)
			return d, d, false
		}
	}}
}
