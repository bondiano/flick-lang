package flickrt

import (
	"sync"
	"time"
)

// raceAll runs the tasks: the first to succeed wins, or with `first` the
// first to end however it ends; the others are interrupted and awaited.
// When every one fails, their causes combine.
func raceAll(fb *Fiber, tasks []Value, first bool) Value {
	ended := make(chan *child, len(tasks))
	children := make([]*child, len(tasks))
	for i, t := range tasks {
		children[i] = fork(fb, fb.owner, t, func(c *child) { ended <- c })
	}
	var failed *Cause
	for range tasks {
		var c *child
		if fb.state.mask > 0 {
			c = <-ended
		} else {
			select {
			case c = <-ended:
			case <-fb.state.signal:
				stop(fb, children...)
				panic(fb.state.stopped())
			}
		}
		if c.cause == nil || first {
			stop(fb, children...)
			return c.result()
		}
		failed = Parallel(failed, c.cause)
	}
	panic(&Failure{failed})
}

func RaceAll(fb *Fiber, tasks Value) Value      { return raceAll(fb, List(tasks), false) }
func RaceAllFirst(fb *Fiber, tasks Value) Value { return raceAll(fb, List(tasks), true) }

func FiberId(fb *Fiber) Value { return float64(fb.state.id) }

// cache memoizes how a task ends, for `ttl` when positive; the first
// fiber to run it computes, the others wait for that.
type cache struct {
	mu      sync.Mutex
	task    Value
	ttl     time.Duration
	exit    *Exit
	expires time.Time
}

func (c *cache) run(fb *Fiber) Value {
	c.mu.Lock()
	if c.exit == nil || c.ttl > 0 && time.Now().After(c.expires) {
		c.exit = exitOf(fb, c.task)
		c.expires = time.Now().Add(c.ttl)
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

func ttlOf(d Value) time.Duration {
	x := dur(d)
	if x.infinite() {
		return 0
	}
	return time.Duration(x.toNanos().Int64())
}

func Cached(_ *Fiber, task Value) Value {
	c := &cache{task: task}
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
	env := make(map[string]Value, len(fb.env)+1)
	for k, x := range fb.env {
		env[k] = x
	}
	env[key] = v
	inner := *fb
	inner.env = env
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
	if s, ok := fb.env[schedulerKey]; ok {
		return s
	}
	return defaultScheduler
}

func SchedulerWith(fb *Fiber, s, task Value) Value { return withEnv(fb, schedulerKey, s, task) }

func SchedulerMaxOps(fb *Fiber) Value {
	if n, ok := fb.env[maxOpsKey]; ok {
		return n
	}
	return int32(2048)
}

func SchedulerWithMaxOps(fb *Fiber, n, task Value) Value       { return withEnv(fb, maxOpsKey, n, task) }
func SchedulerWithPreventYield(fb *Fiber, _, task Value) Value { return Run(fb, task) }

// A Schedule is only what an execution plan reads of one.
//
// shortcut: Schedule is a marker for execution plans, retried without delay; port Effect's Schedule with its module
type schedule struct{}

var ScheduleForever Value = &schedule{}

// planStep is a step of an execution plan: the layer each attempt runs
// with, how many attempts (0 for one, or unbounded with a schedule) and
// whether a failure may try again.
type planStep struct {
	provide  Value
	attempts int
	schedule Value
	while    Value
}

func PlanStep(layer Value) Value { return []*planStep{{provide: layer}} }

func withStep(plan Value, f func(*planStep)) Value {
	s := *plan.([]*planStep)[0]
	f(&s)
	return []*planStep{&s}
}

func PlanWithAttempts(n, step Value) Value {
	return withStep(step, func(s *planStep) { s.attempts = int(n.(int32)) })
}

func PlanWithSchedule(sch, step Value) Value {
	return withStep(step, func(s *planStep) { s.schedule = sch })
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
	if m, ok := fb.env[metadataKey]; ok {
		return m
	}
	return Rec("attempt", int32(0), "stepIndex", int32(0))
}

var (
	attemptStart   = Tag("ExecutionPlan.Event.AttemptStart")
	attemptSuccess = Tag("ExecutionPlan.Event.AttemptSuccess")
	attemptFailure = Tag("ExecutionPlan.Event.AttemptFailure")
)

// WithExecutionPlan runs `task` with the services of the plan's steps in
// turn until an attempt succeeds, telling `onEvent` of each attempt.
func WithExecutionPlan(fb *Fiber, plan, onEvent, task Value) Value {
	var last *Cause
	attempt := 0
	for index, step := range plan.([]*planStep) {
		for stepAttempt := 1; ; stepAttempt++ {
			attempt++
			info := Rec("attempt", int32(attempt), "stepAttempt", int32(stepAttempt), "stepIndex", int32(index))
			Run(fb, Apply(onEvent, NewData(attemptStart, info)))
			started := time.Now()
			meta := Rec("attempt", int32(attempt), "stepIndex", int32(index))
			e := exitOf(fb, Eff(func(fb *Fiber) Value {
				return withEnv(fb, metadataKey, meta, Eff(func(fb *Fiber) Value {
					return Provide(fb, step.provide, task)
				}))
			}))
			took := fromMillis(float64(time.Since(started).Microseconds()) / 1000)
			if e.cause == nil {
				Run(fb, Apply(onEvent, NewData(attemptSuccess, info, took)))
				return e.value
			}
			Run(fb, Apply(onEvent, NewData(attemptFailure, info, took, failureOf(e.cause))))
			last = e.cause
			if e.cause.has(causeInterrupt) && !e.cause.has(causeFail) {
				panic(&Failure{e.cause})
			}
			limit := step.attempts
			if limit == 0 && step.schedule == nil {
				limit = 1
			}
			if limit > 0 && stepAttempt >= limit {
				break
			}
			if step.while != nil {
				if r := e.cause.firstOf(causeFail); r == nil || !Bool(Run(fb, Apply(step.while, r.err))) {
					break
				}
			}
		}
	}
	panic(&Failure{last})
}
