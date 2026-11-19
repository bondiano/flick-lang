package flickrt

import (
	"fmt"
	"testing"
	"time"
)

// runAll runs the tests of module M in order, as RunTests does with
// --jobs 1: their results, as name, status, kind and message.
func runAll(tests []Value, m marks) []string {
	selected := flatten("M", tests, nil, nil, m)
	runs := suiteRuns(rootFiber(), selected)
	var out []string
	for _, n := range selected {
		r := runOne(rootFiber(), n, 0, runs)
		out = append(out, fmt.Sprintf("%s %s %s %s", r.Name, r.Status, r.Kind, r.Message))
	}
	return out
}

// The hooks of a group run around its selected tests, outer to inner, and
// their cleanup in reverse; a group none of whose tests the run selects,
// here a skipped one, runs none.
func TestHooksRunAroundTheSelectedTests(t *testing.T) {
	var log []string
	note := func(text string) Value {
		return Eff(func(*Fiber) Value { log = append(log, text); return Unit })
	}
	hooks := func(level string) []Value {
		return []Value{
			TestBeforeAll(note("before_all " + level)), TestBeforeEach(note("before_each " + level)),
			TestAfterEach(note("after_each " + level)), TestAfterAll(note("after_all " + level)),
		}
	}
	inner := TestGroup("inner", append(hooks("inner"), TestEffect("one", note("one")), TestEffect("two", note("two"))))
	skipped := TestSkip(TestGroup("skipped", append(hooks("skipped"), TestEffect("four", note("four")))))
	outer := TestGroup("outer", append(hooks("outer"), inner, TestEffect("three", note("three")), skipped))
	var selected []named
	for _, n := range flatten("M", []Value{outer}, nil, nil, marks{}) {
		if !n.skip {
			selected = append(selected, n)
		}
	}
	runs := suiteRuns(rootFiber(), selected)
	for _, n := range selected {
		if r := runOne(rootFiber(), n, 0, runs); r.Status != "passed" {
			t.Fatalf("%s: %+v", n.name, r)
		}
	}
	each := func(body string) string {
		return fmt.Sprint("before_each outer before_each inner ", body, " after_each inner after_each outer ")
	}
	want := "before_all outer before_all inner " + each("one") + each("two") + "after_all inner before_each outer three after_each outer after_all outer"
	if got := fmt.Sprint(log); got != "["+want+"]" {
		t.Errorf("hooks ran as\n%s\nwant\n[%s]", got, want)
	}
}

// A failed before_all reports each test of its group as a setup failure
// and runs none; a failed before_each fails its test's setup and skips its
// body; the after hooks still run.
func TestAFailedSetupIsReportedAndCleanedUp(t *testing.T) {
	var log []string
	note := func(text string) Value {
		return Eff(func(*Fiber) Value { log = append(log, text); return Unit })
	}
	failing := func(err Value) Value { return Eff(func(*Fiber) Value { return Fail(err) }) }
	got := runAll([]Value{
		TestGroup("all", []Value{
			TestBeforeAll(failing(NewData(assertionTag, "no database"))), TestAfterAll(note("after_all")),
			TestEffect("a", note("a")), TestEffect("b", note("b")),
		}),
		TestGroup("each", []Value{TestAfterEach(note("after_each")), TestBeforeEach(failing("boom")), TestEffect("c", note("c"))}),
	}, marks{})
	want := []string{
		"M > all > a failed setup no database",
		"M > all > b failed setup no database",
		"M > each > c failed setup fail boom",
	}
	if fmt.Sprint(got) != fmt.Sprint(want) {
		t.Errorf("results %q, want %q", got, want)
	}
	if fmt.Sprint(log) != "[after_all after_each]" {
		t.Errorf("cleanup ran as %v", log)
	}
}

// A test out of time, its own or the run's, is interrupted, its after_each
// hooks run, and it is reported as a timeout.
func TestATestOutOfTimeIsInterruptedAndCleanedUp(t *testing.T) {
	var log []string
	slow := func(name string) Value {
		return TestEffect(name, Eff(func(fb *Fiber) Value {
			EffectSleep(fb, fromMillis(3_600_000))
			log = append(log, name+" woke")
			return Unit
		}))
	}
	after := TestAfterEach(Eff(func(*Fiber) Value { log = append(log, "after_each"); return Unit }))
	started := time.Now()
	got := runAll([]Value{
		TestGroup("g", []Value{after, TestTimeout(20.0, slow("own")), slow("default")}),
		TestTimeout(5000.0, TestEffect("quick", Eff(func(*Fiber) Value { return Unit }))),
	}, marks{timeout: 30, timed: true})
	want := []string{
		"M > g > own failed timeout timed out after 20 ms",
		"M > g > default failed timeout timed out after 30 ms",
		"M > quick passed  ",
	}
	if fmt.Sprint(got) != fmt.Sprint(want) {
		t.Errorf("results %q, want %q", got, want)
	}
	if fmt.Sprint(log) != "[after_each after_each]" {
		t.Errorf("cleanup ran as %v", log)
	}
	if time.Since(started) > 10*time.Second {
		t.Errorf("the run waited for the sleeps")
	}
}
