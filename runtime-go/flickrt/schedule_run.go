package flickrt

// Running effects by a schedule: Effect's repeat, retry and scheduleFrom.

// stepper is Effect's `toStepWithMetadata`: a run of the schedule that
// reads the clock for each input and sleeps the step's delay, giving the
// step's metadata, or with `done` the last output.
func (s *Schedule) stepper(fb *Fiber) func(fb *Fiber, input Value) (metadata, bool) {
	clock, step, count := clockOf(fb), s.start(fb), &counter{}
	return func(fb *Fiber, input Value) (metadata, bool) {
		now := clock.millis()
		out, delay, done := step(fb, now, input)
		if done {
			return metadata{output: out}, true
		}
		m := count.next(now, input)
		m.output, m.duration = out, delay
		sleepFor(fb, delay)
		return m, false
	}
}

const scheduleMetadataKey = "effect/Schedule/CurrentMetadata"

var defaultMetadata = metadata{input: Unit, output: Unit, duration: noDelay}

// ScheduleCurrentMetadata is the metadata of the step of the schedule
// that runs the task again.
func ScheduleCurrentMetadata(fb *Fiber) Value {
	if m, ok := fb.env.get(scheduleMetadataKey); ok {
		return m
	}
	return defaultMetadata.record()
}

func runWithMetadata(fb *Fiber, m metadata, task Value) Value {
	return withEnv(fb, scheduleMetadataKey, m.record(), task)
}

// errorOnly is the first error of a cause that holds only typed errors
// (Effect's `findErrorOnly`).
func errorOnly(c *Cause) (Value, bool) {
	reasons := c.reasons()
	for _, r := range reasons {
		if r.kind != causeFail {
			return nil, false
		}
	}
	if len(reasons) == 0 {
		return nil, false
	}
	return reasons[0].err, true
}

// repeatOrElse runs `task` again while the schedule, given each result,
// goes on: the schedule's last output. A typed failure of the task or the
// schedule goes to `orElse`, with the metadata of the last step if one ran.
func repeatOrElse(fb *Fiber, s *Schedule, orElse func(fb *Fiber, err, last Value) Value, task Value) Value {
	step, meta := s.stepper(fb), defaultMetadata
	out, cause := caught(fb, func() Value {
		for {
			m, done := step(fb, runWithMetadata(fb, meta, task))
			if done {
				return m.output
			}
			meta = m
		}
	})
	if cause == nil {
		return out
	}
	err, ok := errorOnly(cause)
	if !ok {
		panic(&Failure{cause})
	}
	last := None
	if meta.attempt != 0 {
		last = Some(meta.record())
	}
	return orElse(fb, err, last)
}

// retryOrElse runs `task` again after each typed failure while the
// schedule, given the error, goes on; then `orElse` gets the last error
// and the schedule's last output.
func retryOrElse(fb *Fiber, s *Schedule, orElse func(fb *Fiber, err, out Value) Value, task Value) Value {
	step, meta := s.stepper(fb), defaultMetadata
	for {
		out, cause := caught(fb, func() Value { return runWithMetadata(fb, meta, task) })
		if cause == nil {
			return out
		}
		err, ok := errorOnly(cause)
		if !ok {
			panic(&Failure{cause})
		}
		m, done := step(fb, err)
		if done {
			return orElse(fb, err, m.output)
		}
		meta = m
	}
}

func failWith(_ *Fiber, err, _ Value) Value { return Fail(err) }

// scheduleFrom steps the schedule with `initial`, then runs `task` and
// steps it with each result until it ends: its last output.
func scheduleFrom(fb *Fiber, s *Schedule, initial, task Value) Value {
	step := s.stepper(fb)
	m, done := step(fb, initial)
	for !done {
		m, done = step(fb, runWithMetadata(fb, m, task))
	}
	return m.output
}

func EffectRepeat(fb *Fiber, s, task Value) Value {
	return repeatOrElse(fb, scheduleOf(s), failWith, task)
}
func EffectRetry(fb *Fiber, s, task Value) Value {
	return retryOrElse(fb, scheduleOf(s), failWith, task)
}

func EffectRepeatOrElse(fb *Fiber, s, orElse, task Value) Value {
	return repeatOrElse(fb, scheduleOf(s), func(fb *Fiber, err, last Value) Value {
		return Run(fb, apply2(fb, orElse, err, last))
	}, task)
}

func EffectRetryOrElse(fb *Fiber, s, orElse, task Value) Value {
	return retryOrElse(fb, scheduleOf(s), func(fb *Fiber, err, out Value) Value {
		return Run(fb, apply2(fb, orElse, err, out))
	}, task)
}

func EffectSchedule(fb *Fiber, s, task Value) Value {
	return scheduleFrom(fb, scheduleOf(s), Unit, task)
}

func EffectScheduleFrom(fb *Fiber, initial, s, task Value) Value {
	return scheduleFrom(fb, scheduleOf(s), initial, task)
}

// fromOptions is Effect's `buildFromOptions` of a repeat or a retry: the
// inputs of `schedule` (or of `forever`), while `while` holds of them,
// at most `times` times when it is not negative.
func fromOptions(schedule *Schedule, while func(fb *Fiber, input Value) bool, times int32) *Schedule {
	s := passthroughForever
	if schedule != nil {
		s = passthrough(schedule)
	}
	if while != nil {
		s = whileMeta(s, func(fb *Fiber, m metadata) bool { return while(fb, m.input) })
	}
	if times >= 0 {
		s = whileMeta(s, func(_ *Fiber, m metadata) bool { return m.attempt <= times })
	}
	return s
}
