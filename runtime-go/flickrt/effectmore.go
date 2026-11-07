package flickrt

import (
	"math"
	"math/big"
	"sync"
)

// raceAll runs the tasks: the first to succeed wins, or with `first` the
// first to end however it ends; the others are interrupted and awaited.
// When every one fails, their causes combine.
func raceAll(fb *Fiber, tasks []Value, first bool) Value {
	ended := make(chan *child, len(tasks))
	children := make([]*child, len(tasks))
	if len(tasks) > 0 { // no task, no wait: nothing would release the hold
		holdForks(fb.state)
	}
	for i, t := range tasks {
		children[i] = fork(fb, fb.owner, t, func(c *child) { ended <- c })
	}
	var failed *Cause
	signal := signalOf(fb)
	for range tasks {
		c := nextEnded(fb, ended, signal)
		if c == nil {
			stop(fb, children...)
			panic(fb.state.stopped())
		}
		if c.cause == nil || first {
			stop(fb, children...)
			return c.result()
		}
		failed = concat(failed, c.cause)
	}
	panic(&Failure{failed.electDefects()}) // as Effect's raceAll
}

func RaceAll(fb *Fiber, tasks Value) Value      { return raceAll(fb, List(tasks), false) }
func RaceAllFirst(fb *Fiber, tasks Value) Value { return raceAll(fb, List(tasks), true) }

func FiberId(fb *Fiber) Value { return float64(fb.state.id) }

// cache memoizes how a task ends, for `ttl` milliseconds of the clock of
// the fiber that runs it; the first fiber to run it computes, the others
// wait for that.
type cache struct {
	mu      sync.Mutex
	task    Value
	ttl     float64
	exit    *Exit
	expires float64
}

func (c *cache) run(fb *Fiber) Value {
	c.mu.Lock()
	clock := clockOf(fb)
	if c.exit == nil || !math.IsInf(c.expires, 1) && clock.millis() >= c.expires {
		c.exit = exitOf(fb, c.task)
		c.expires = clock.millis() + c.ttl
	}
	e := c.exit
	c.mu.Unlock()
	return Run(fb, e.outcome())
}

func (c *cache) invalidate() {
	c.mu.Lock()
	c.exit = nil
	c.mu.Unlock()
}

func ttlOf(d Value) float64 { return millisOf(dur(d)) }

func Cached(_ *Fiber, task Value) Value {
	c := &cache{task: task, ttl: math.Inf(1)}
	return Eff(c.run)
}

func CachedWithTTL(_ *Fiber, d, task Value) Value {
	c := &cache{task: task, ttl: ttlOf(d)}
	return Eff(c.run)
}

func CachedInvalidateWithTTL(_ *Fiber, d, task Value) Value {
	c := &cache{task: task, ttl: ttlOf(d)}
	invalidate := Eff(func(*Fiber) Value {
		c.invalidate()
		return Unit
	})
	return []Value{Eff(c.run), invalidate}
}

// withEnv runs `task` with `key` set in the fiber's environment, where
// fiber-local settings live beside the services.
func withEnv(fb *Fiber, key string, v, task Value) Value {
	inner := *fb
	inner.env = fb.env.with(key, v)
	return Run(&inner, task)
}

// Scheduler settings: Go's scheduler is not configurable, so a scheduler
// is an opaque value and the settings are only read back.
//
// shortcut: nothing observes yielding on the go target; schedule by these settings if a program needs it
type scheduler struct{}

const (
	schedulerKey = "$scheduler"
	maxOpsKey    = "$maxOpsBeforeYield"
)

var defaultScheduler Value = &scheduler{}

func SchedulerMakeMixed() Value { return &scheduler{} }

func SchedulerCurrent(fb *Fiber) Value {
	if s, ok := fb.env.get(schedulerKey); ok {
		return s
	}
	return defaultScheduler
}

func SchedulerWith(fb *Fiber, s, task Value) Value { return withEnv(fb, schedulerKey, s, task) }

func SchedulerMaxOps(fb *Fiber) Value {
	if n, ok := fb.env.get(maxOpsKey); ok {
		return n
	}
	return int32(2048)
}

func SchedulerWithMaxOps(fb *Fiber, n, task Value) Value       { return withEnv(fb, maxOpsKey, n, task) }
func SchedulerWithPreventYield(fb *Fiber, _, task Value) Value { return Run(fb, task) }

// planStep is a step of an execution plan: the layer each attempt runs
// with, how many attempts (0 when not set), the schedule of its retries
// and which errors it retries.
type planStep struct {
	provide  Value
	attempts int32
	schedule *Schedule
	while    Value
}

func PlanStep(layer Value) Value { return []*planStep{{provide: layer}} }

func withStep(plan Value, f func(*planStep)) Value {
	s := *plan.([]*planStep)[0]
	f(&s)
	return []*planStep{&s}
}

func PlanWithAttempts(n, step Value) Value {
	return withStep(step, func(s *planStep) { s.attempts = n.(int32) })
}

func PlanWithSchedule(sch, step Value) Value {
	return withStep(step, func(s *planStep) { s.schedule = scheduleOf(sch) })
}

func PlanWithWhile(f, step Value) Value {
	return withStep(step, func(s *planStep) { s.while = f })
}

func PlanMake(steps Value) Value {
	var out []*planStep
	for _, s := range List(steps) {
		out = append(out, s.([]*planStep)...)
	}
	return out
}

func PlanMerge(plans Value) Value { return PlanMake(plans) }

const metadataKey = "$executionPlanMetadata"

func PlanCurrentMetadata(fb *Fiber) Value {
	if m, ok := fb.env.get(metadataKey); ok {
		return m
	}
	return Rec("attempt", int32(0), "stepIndex", int32(0))
}

var (
	attemptStart   = Tag("ExecutionPlan.Event.AttemptStart")
	attemptSuccess = Tag("ExecutionPlan.Event.AttemptSuccess")
	attemptFailure = Tag("ExecutionPlan.Event.AttemptFailure")
	scheduleOnce   = ScheduleRecurs(int32(1)).(*Schedule)
)

// retries is Effect's `scheduleFromStep`: the schedule that retries the
// step, nil for none. A step after the first replays the previous step's
// error to it first.
func (s *planStep) retries(first bool) *Schedule {
	var while func(*Fiber, Value) bool
	if s.while != nil {
		while = func(fb *Fiber, input Value) bool { return Bool(Run(fb, Apply(fb, s.while, input))) }
	}
	times := s.attempts
	if times == 0 {
		times = -1
	}
	if !first {
		sch := s.schedule
		if sch == nil && s.attempts == 0 {
			sch = scheduleOnce
		}
		return fromOptions(sch, while, times)
	}
	if s.attempts == 1 || s.schedule == nil && s.attempts == 0 {
		return nil
	}
	if times > 0 {
		times--
	}
	return fromOptions(s.schedule, while, times)
}

// WithExecutionPlan runs `task` with the services of the plan's steps in
// turn, each retried by its schedule, until one succeeds, telling
// `onEvent` of each attempt.
func WithExecutionPlan(fb *Fiber, plan, onEvent, task Value) Value {
	steps := plan.([]*planStep)
	attempt, lastStepIndex, stepAttempt := int32(0), int32(-1), int32(0)
	emit := func(fb *Fiber, event Value) { catch(func() Value { return Run(fb, Apply(fb, onEvent, event)) }) }
	// an attempt with its metadata and events, as Effect's `provideMeta` and `instrument`
	attemptAt := func(index int32, step *planStep) Value {
		return Eff(func(fb *Fiber) Value {
			attempt++
			meta := Rec("attempt", attempt, "stepIndex", index)
			return withEnv(fb, metadataKey, meta, Eff(func(fb *Fiber) Value {
				outer := fb.state.mask
				return masked(fb, outer+1, func() Value {
					if index != lastStepIndex {
						lastStepIndex, stepAttempt = index, 0
					}
					stepAttempt++
					info := Rec("attempt", attempt, "stepAttempt", stepAttempt, "stepIndex", index)
					clock := clockOf(fb)
					emit(fb, NewData(attemptStart, info))
					started := clock.monotonic()
					v, c := catch(func() Value {
						return masked(fb, outer, func() Value { return Provide(fb, step.provide, task) })
					})
					took := fromNanos(new(big.Int).Sub(clock.monotonic(), started))
					if c == nil {
						emit(fb, NewData(attemptSuccess, info, took))
						return v
					}
					emit(fb, NewData(attemptFailure, info, took, failureOf(c)))
					panic(&Failure{c})
				})
			}))
		})
	}
	// how the last step ended: a typed error is kept, anything else ends the plan
	var value, err Value
	failed := false
	for i := 0; i < len(steps) && (i == 0 || failed); i++ {
		step := steps[i]
		next := attemptAt(int32(i), step)
		if i > 0 {
			attempted, wrapped, previous := false, next, err
			next = Eff(func(fb *Fiber) Value {
				if attempted {
					return Run(fb, wrapped)
				}
				attempted = true
				return Fail(previous)
			})
			next = retrying(step.retries(false), next)
		} else if s := step.retries(true); s != nil {
			next = retrying(s, next)
		}
		v, c := caught(fb, func() Value { return Run(fb, next) })
		value, failed = v, c != nil
		if failed {
			r := c.firstOf(causeFail)
			if r == nil {
				panic(&Failure{c})
			}
			err = r.err
		}
	}
	if failed {
		return Fail(err)
	}
	return value
}

func retrying(s *Schedule, task Value) Value {
	return Eff(func(fb *Fiber) Value { return retryOrElse(fb, s, failWith, task) })
}
