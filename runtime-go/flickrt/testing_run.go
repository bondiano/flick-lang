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

// TestSerial is Test.serial: the test, and each test in it, run one at a time.
func TestSerial(t Value) Value {
	copied := *t.(*test)
	copied.serial = true
	return &copied
}

// jobsOf is the selected tests by job, as indexes: the tests of one
// Test.serial in one job, run one at a time, where the first of them
// stands, and each other test in a job of its own.
func jobsOf(selected []named) [][]int {
	var jobs [][]int
	serial := map[*test]int{}
	for i, t := range selected {
		if t.serial == nil {
			jobs = append(jobs, []int{i})
			continue
		}
		if at, ok := serial[t.serial]; ok {
			jobs[at] = append(jobs[at], i)
			continue
		}
		serial[t.serial] = len(jobs)
		jobs = append(jobs, []int{i})
	}
	return jobs
}

// TestTimeout is Test.timeout: the test, and each test in it without one
// of its own, given `millis`.
func TestTimeout(millis, t Value) Value {
	copied := *t.(*test)
	copied.timeout, copied.timed = millis.(float64), true
	return &copied
}

// setupFailure is a failed before_* hook: the cause it failed with, or
// the text of one that ran out of time.
type setupFailure struct {
	cause *Cause
	text  string
}

func (s *setupFailure) message() string {
	if s.cause == nil {
		return s.text
	}
	_, text := outcome("", false, s.cause)
	return text
}

func hookTimedOut(what string, millis float64) string {
	return fmt.Sprintf("%s timed out after %s ms", what, ShowNumber(millis))
}

// hookWithin runs the hook `task` in `fb`, within `millis` when `timed`: false
// when they ran out, which interrupted it. It runs on a state of its own,
// which a finalizer's mask does not keep from the interruption.
func hookWithin(fb *Fiber, task Value, millis float64, timed bool) bool {
	if !timed {
		Run(fb, task)
		return true
	}
	hook := *fb
	hook.state = newState()
	var out atomic.Bool
	timer := time.AfterFunc(time.Duration(millis*float64(time.Millisecond)), func() {
		out.Store(true)
		hook.state.interrupt(0)
	})
	defer timer.Stop()
	_, cause := catch(func() Value { return Run(&hook, task) })
	switch {
	case cause == nil:
		return true
	case out.Load() && !cause.has(causeFail) && !cause.has(causeDie):
		return false
	}
	panic(&Failure{cause})
}

// runHooks runs `tasks` in order in `fb`, each within `millis` when
// `timed`: the failure of the first that fails. An interrupted hook
// interrupts the test.
func runHooks(fb *Fiber, tasks []Value, what string, millis float64, timed bool) *setupFailure {
	for _, task := range tasks {
		done := false
		_, cause := catch(func() Value { done = hookWithin(fb, task, millis, timed); return Unit })
		switch {
		case cause != nil && !cause.has(causeFail) && !cause.has(causeDie):
			panic(&Failure{cause})
		case cause != nil:
			return &setupFailure{cause: cause}
		case !done:
			return &setupFailure{text: hookTimedOut(what, millis)}
		}
	}
	return nil
}

// finalize makes the hooks `tasks` finalizers of the owner of `fb`, which
// runs them in the reverse order, each within `millis` when `timed`: one out
// of time fails the owner's cleanup.
func finalize(fb *Fiber, tasks []Value, what string, millis float64, timed bool) {
	for _, task := range tasks {
		fb.owner.add(fb, func(fb *Fiber, _ *Cause) Value {
			if !hookWithin(fb, task, millis, timed) {
				Fail(hookTimedOut(what, millis))
			}
			return Unit
		})
	}
}

// suiteRun is a suite in a run: an owner of its own, set up before its
// first selected test, and which closes after its last.
type suiteRun struct {
	once     sync.Once
	fiber    Fiber
	services Services
	// what failed its layer, and what failed a before_all hook
	failure   *Cause
	setup     *setupFailure
	mu        sync.Mutex
	remaining int
	// its shared fixtures, from its setup on
	shared *fixtureCells
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
// layer is built; its hooks have its timeout, if it has one.
func (r *suiteRun) setUp(s *test, fx *fixtures) {
	// its shared fixtures close after its after_all hooks, which may use them
	r.shared = cellsIn(&r.fiber, r.fiber.owner)
	if s.hooks != nil {
		finalize(&r.fiber, withEach(fx, s.hooks.afterAll), "after_all", s.timeout, s.timed)
		if r.setup = runHooks(&r.fiber, withEach(fx, s.hooks.beforeAll), "before_all", s.timeout, s.timed); r.setup != nil {
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
// up on first use, or a failed before_all hook; a failed build fails the
// test.
func enter(inner *Fiber, suites []*test, runs map[*test]*suiteRun) (*Fiber, *setupFailure) {
	for i, s := range suites {
		run := runs[s]
		// the hooks of a suite see its shared fixtures and those of the suites around it
		run.once.Do(func() { run.setUp(s, &fixtures{suites: suites[:i+1], runs: runs}) })
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
// their after_each hooks are finalizers of the test's owner: the first that
// fails. The test's timeout bounds them and its body together.
func eachHooks(fb *Fiber, suites []*test, cells *fixtureCells, runs map[*test]*suiteRun) *setupFailure {
	for i, s := range suites {
		if s.hooks == nil {
			continue
		}
		fx := &fixtures{test: cells, suites: suites[:i+1], runs: runs}
		finalize(fb, withEach(fx, s.hooks.afterEach), "after_each", s.timeout, s.timed)
		if failed := runHooks(fb, withEach(fx, s.hooks.beforeEach), "before_each", 0, false); failed != nil {
			return failed
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
	var setup *setupFailure
	var outOfTime atomic.Bool
	_, exit := catch(func() Value {
		fiber, failed := enter(&inner, t.suites, runs)
		if setup = failed; failed != nil {
			return Unit
		}
		// the timeout is of the before_each hooks and the body, not of the suites' setup
		if t.timed {
			timer := time.AfterFunc(time.Duration(t.timeout*float64(time.Millisecond)), func() {
				outOfTime.Store(true)
				inner.state.interrupt(0)
			})
			defer timer.Stop()
		}
		// the fixtures of the test, made the first time its code uses each
		cells := cellsIn(fiber, owner)
		if setup = eachHooks(fiber, t.suites, cells, runs); setup != nil {
			cells.failed = setup.cause
			if cells.failed == nil {
				cells.failed = &Cause{kind: causeFail, err: setup.text}
			}
			return Unit
		}
		if t.test.task != nil {
			return Run(fiber, withFixtures(&fixtures{test: cells, suites: t.suites, runs: runs}, t.test.task))
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
		kind, text = "setup", setup.message()
	case kind == "interrupted" && outOfTime.Load():
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
