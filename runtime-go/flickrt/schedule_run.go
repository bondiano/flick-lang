package flickrt

import (
	"runtime"
	"time"
)

// A run of a schedule, as Effect's toStepWithMetadata: each step reads the
// fiber's clock and gives the metadata of the recurrence.
type scheduleRun struct {
	next step
	m    meta
}

func startRun(fb *Fiber, s Value) *scheduleRun { return &scheduleRun{next: scheduleOf(s).start(fb)} }

// step is the metadata of the next recurrence and its delay, or, when
// the schedule is done, its last output.
func (r *scheduleRun) step(fb *Fiber, input Value) (Value, *Duration, bool) {
	now := clockOf(fb).millis()
	out, d, done := r.next(fb, now, input)
	if done {
		return out, nil, true
	}
	return r.m.next(now, input).record(out, d), d, false
}

// pause sleeps a delay of a schedule; none only lets others run, as
// Effect's sleep of zero yields.
func pause(fb *Fiber, d *Duration) {
	if d.negative() || DurationIsZero(d).(bool) {
		runtime.Gosched()
		Check(fb)
		return
	}
	sleep(fb, toGoDuration(d), d.infinite())
}

// sleepFor is pause with Go's duration, for delays not of a schedule.
func sleepFor(fb *Fiber, d time.Duration) {
	if d <= 0 {
		Check(fb)
		return
	}
	sleep(fb, d, false)
}

const scheduleMetaKey = "$scheduleMetadata"

// noMetadata is Effect's CurrentMetadata outside a repetition.
var noMetadata = Rec("input", Unit, "output", Unit, "duration", DurationZero, "attempt", int32(0),
	"start", 0.0, "now", 0.0, "elapsed", 0.0, "elapsedSincePrevious", 0.0)

func ScheduleCurrentMetadata(fb *Fiber) Value {
	if m, ok := fb.env.get(scheduleMetaKey); ok {
		return m
	}
	return noMetadata
}

// failsOnly is the first error of a cause of typed failures only, which
// is all a repetition recovers from.
func failsOnly(c *Cause) (Value, bool) {
	for _, r := range c.reasons() {
		if r.kind != causeFail {
			return nil, false
		}
	}
	if r := c.firstOf(causeFail); r != nil {
		return r.err, true
	}
	return nil, false
}

// repeatOrElse runs `task` again after each success while the schedule,
// given the value, goes on: its last output. A typed failure, of the task
// or of the schedule, goes to `orElse` with the metadata of the last
// recurrence, if any.
func repeatOrElse(fb *Fiber, s Value, orElse func(err, last Value) Value, task Value) Value {
	r := startRun(fb, s)
	m := noMetadata
	out, cause := catch(func() Value {
		for {
			v := withEnv(fb, scheduleMetaKey, m, task)
			next, d, done := r.step(fb, v)
			if done {
				return next
			}
			pause(fb, d)
			m = next
		}
	})
	if cause == nil {
		return out
	}
	err, ok := failsOnly(cause)
	if !ok {
		panic(&Failure{cause})
	}
	var last Value = None
	if record(m).Get("attempt").(int32) > 0 {
		last = Some(m)
	}
	return orElse(err, last)
}

// retryOrElse runs `task` again after each typed failure while the
// schedule, given the error, goes on; then `orElse` of the last error and
// the schedule's last output.
func retryOrElse(fb *Fiber, s Value, orElse func(err, out Value) Value, task Value) Value {
	r := startRun(fb, s)
	m := noMetadata
	for {
		v, cause := catch(func() Value { return withEnv(fb, scheduleMetaKey, m, task) })
		if cause == nil {
			return v
		}
		err, ok := failsOnly(cause)
		if !ok {
			panic(&Failure{cause})
		}
		next, d, done := r.step(fb, err)
		if done {
			return orElse(err, next)
		}
		pause(fb, d)
		m = next
	}
}

func refail(err, _ Value) Value { return Fail(err) }

func EffectRepeat(fb *Fiber, s, task Value) Value { return repeatOrElse(fb, s, refail, task) }
func EffectRetry(fb *Fiber, s, task Value) Value  { return retryOrElse(fb, s, refail, task) }

func EffectRepeatOrElse(fb *Fiber, s, orElse, task Value) Value {
	return repeatOrElse(fb, s, func(err, last Value) Value { return Run(fb, apply2(orElse, err, last)) }, task)
}

func EffectRetryOrElse(fb *Fiber, s, orElse, task Value) Value {
	return retryOrElse(fb, s, func(err, out Value) Value { return Run(fb, apply2(orElse, err, out)) }, task)
}

// EffectScheduleFrom steps the schedule with `initial`, then runs `task`
// after each recurrence, stepping with its value: the last output.
func EffectScheduleFrom(fb *Fiber, initial, s, task Value) Value {
	r := startRun(fb, s)
	input := initial
	for {
		m, d, done := r.step(fb, input)
		if done {
			return m
		}
		pause(fb, d)
		input = withEnv(fb, scheduleMetaKey, m, task)
	}
}

func EffectSchedule(fb *Fiber, s, task Value) Value { return EffectScheduleFrom(fb, Unit, s, task) }
