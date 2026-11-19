package flickrt

import (
	"fmt"
	"sync"
	"sync/atomic"
	"time"
)

// The run of a test, as the JS runtime's testing module: in a fiber and an
// owner of its own, inside the suites it is in, each set up once before its
// first selected test and closed after its last, with the hooks of the
// groups around it and within its timeout.

// groupHooks are the hooks of a group, each kind in the order written.
type groupHooks struct {
	beforeAll, beforeEach, afterEach, afterAll []Value
}

// hooksOf is the hooks among `tests`, nil when there are none.
func hooksOf(tests []Value) *groupHooks {
	var h groupHooks
	found := false
	for _, v := range tests {
		t := v.(*test)
		switch t.when {
		case "before_all":
			h.beforeAll = append(h.beforeAll, t.task)
		case "before_each":
			h.beforeEach = append(h.beforeEach, t.task)
		case "after_each":
			h.afterEach = append(h.afterEach, t.task)
		case "after_all":
			h.afterAll = append(h.afterAll, t.task)
		default:
			continue
		}
		found = true
	}
	if !found {
		return nil
	}
	return &h
}

// TestBeforeAll, TestBeforeEach, TestAfterEach and TestAfterAll are the
// hooks of Test: of the group they are in.
func TestBeforeAll(task Value) Value {
	return &test{name: "before_all", when: "before_all", task: task}
}
func TestBeforeEach(task Value) Value {
	return &test{name: "before_each", when: "before_each", task: task}
}
func TestAfterEach(task Value) Value {
	return &test{name: "after_each", when: "after_each", task: task}
}
func TestAfterAll(task Value) Value { return &test{name: "after_all", when: "after_all", task: task} }

// TestTimeout is Test.timeout: the test, and each test in it without one
// of its own, given `millis`.
func TestTimeout(millis, t Value) Value {
	copied := *t.(*test)
	copied.timeout, copied.timed = millis.(float64), true
	return &copied
}

// runHooks runs `tasks` in order in `fb`: the cause of the first that
// fails. An interrupted hook interrupts the test.
func runHooks(fb *Fiber, tasks []Value) *Cause {
	for _, task := range tasks {
		if _, cause := catch(func() Value { return Run(fb, task) }); cause != nil {
			if !cause.has(causeFail) && !cause.has(causeDie) {
				panic(&Failure{cause})
			}
			return cause
		}
	}
	return nil
}

// finalize makes the hooks `tasks` finalizers of the owner of `fb`, which
// runs them in the reverse order.
func finalize(fb *Fiber, tasks []Value) {
	for _, task := range tasks {
		fb.owner.add(fb, func(fb *Fiber, _ *Cause) Value { return Run(fb, task) })
	}
}

// suiteRun is a suite in a run: an owner of its own, set up before its
// first selected test, and which closes after its last.
type suiteRun struct {
	once     sync.Once
	fiber    Fiber
	services Services
	// what failed its layer, and what failed a before_all hook
	failure, setup *Cause
	mu             sync.Mutex
	remaining      int
}

// suiteRuns is the run of each suite of the selected tests.
func suiteRuns(fb *Fiber, selected []named) map[*test]*suiteRun {
	runs := map[*test]*suiteRun{}
	for _, t := range selected {
		for _, s := range t.suites {
			if runs[s] == nil {
				owner := newOwner()
				owner.defects = &defects{}
				runs[s] = &suiteRun{fiber: Fiber{state: newState(), env: fb.env, owner: owner, candidate: owner}}
			}
			runs[s].remaining++
		}
	}
	return runs
}

// setUp is the setup of suite `s`, in its owner: its after_all hooks are
// finalizers, whatever its before_all hooks do, then those run, then its
// layer is built.
func (r *suiteRun) setUp(s *test) {
	if s.hooks != nil {
		finalize(&r.fiber, s.hooks.afterAll)
		if r.setup = runHooks(&r.fiber, s.hooks.beforeAll); r.setup != nil {
			return
		}
	}
	if s.layer == nil {
		return
	}
	built, failure := catch(func() Value {
		return buildWith(&r.fiber, layerOf(s.layer), currentMemo(&r.fiber), r.fiber.owner)
	})
	if r.failure = failure; failure == nil {
		r.services = built.(Services)
	}
}

// enter is the fiber `inner` with the services of these suites, each set
// up on first use, or the cause of a failed before_all hook; a failed
// build fails the test.
func enter(inner *Fiber, suites []*test, runs map[*test]*suiteRun) (*Fiber, *Cause) {
	for _, s := range suites {
		run := runs[s]
		run.once.Do(func() { run.setUp(s) })
		switch {
		case run.failure != nil:
			panic(&Failure{run.failure})
		case run.setup != nil:
			return inner, run.setup
		case s.layer != nil:
			inner = provided(inner, run.services)
		}
	}
	return inner, nil
}

// eachHooks runs the before_each hooks of these suites, outermost first, once
// their after_each hooks are finalizers of the test's owner: the cause of
// the first that fails.
func eachHooks(fb *Fiber, suites []*test) *Cause {
	for _, s := range suites {
		if s.hooks == nil {
			continue
		}
		finalize(fb, s.hooks.afterEach)
		if cause := runHooks(fb, s.hooks.beforeEach); cause != nil {
			return cause
		}
	}
	return nil
}

// leave closes the suites this test ended; why one failed to close.
func leave(suites []*test, runs map[*test]*suiteRun) string {
	failed := ""
	for i := len(suites) - 1; i >= 0; i-- {
		if released := runs[suites[i]].release(false); released != nil {
			failed = describeReasons(released.reasons())
		}
	}
	return failed
}

// release closes the suite's owner after its last test, or at once when
// the run stopped early.
func (r *suiteRun) release(stopped bool) *Cause {
	r.mu.Lock()
	if !stopped {
		r.remaining--
	}
	last := r.remaining == 0 || stopped && r.remaining > 0
	if stopped {
		r.remaining = 0
	}
	r.mu.Unlock()
	if !last {
		return nil
	}
	return r.fiber.owner.closeFinalizers(&r.fiber, nil)
}

// runOne runs a test in a fiber and an owner of its own.
func runOne(fb *Fiber, t named, seed int64, runs map[*test]*suiteRun) testResult {
	result := runLeaf(fb, t, seed, runs)
	if len(t.suites) == 0 {
		return result
	}
	// the suite's release fails the test that ended it
	if failed := leave(t.suites, runs); failed != "" {
		if result.Status == "passed" {
			result.Kind, result.Message = "cleanup", failed
		}
		result.Status, result.Cleanup = "failed", failed
	}
	return result
}

func runLeaf(fb *Fiber, t named, seed int64, runs map[*test]*suiteRun) testResult {
	owner := newOwner()
	owner.defects = &defects{}
	inner := Fiber{state: newState(), env: fb.env, owner: owner, candidate: owner}
	// stopping the run interrupts each running test
	done := make(chan struct{})
	defer close(done)
	go func() {
		select {
		case <-fb.state.signal:
			inner.state.interrupt(fb.state.id)
		case <-done:
		}
	}()
	var message string
	var bad bool
	var setup *Cause
	var timedOut atomic.Bool
	_, exit := catch(func() Value {
		fiber, failed := enter(&inner, t.suites, runs)
		if setup = failed; failed != nil {
			return Unit
		}
		// the timeout is of the before_each hooks and the body, not of the suites' setup
		if t.timed {
			timer := time.AfterFunc(time.Duration(t.timeout*float64(time.Millisecond)), func() {
				timedOut.Store(true)
				inner.state.interrupt(0)
			})
			defer timer.Stop()
		}
		if setup = eachHooks(fiber, t.suites); setup != nil {
			return Unit
		}
		if t.test.task != nil {
			return Run(fiber, t.test.task)
		}
		message, bad = t.test.checks(fb, seed)
		return Unit
	})
	released := owner.closeFinalizers(&inner, exit)
	// a supervisor that failed during the test fails it, as it fails a program
	exit = Sequential(exit, owner.defects.all())
	kind, text := outcome(message, bad, exit)
	switch {
	case kind == "" && setup != nil:
		_, text = outcome("", false, setup)
		kind = "setup"
	case kind == "interrupted" && timedOut.Load():
		kind, text = "timeout", fmt.Sprintf("timed out after %s ms", ShowNumber(t.timeout))
	}
	cleanup := ""
	if released != nil {
		cleanup = describeReasons(released.reasons())
	}
	switch {
	case kind == "" && cleanup == "":
		return testResult{Name: t.name, Status: "passed"}
	case kind == "":
		kind, text = "cleanup", cleanup
	}
	return testResult{Name: t.name, Status: "failed", Kind: kind, Message: text, Cleanup: cleanup}
}
