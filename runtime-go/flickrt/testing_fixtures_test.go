package flickrt

import (
	"fmt"
	"testing"
)

// A fixture is made once for each test that uses it, a shared one once for
// the run of its suite, an unused one never; each is released after the
// hooks that close its scope, the fixtures of a test in reverse.
func TestFixturesAreMadeOnceAndReleasedAfterTheHooks(t *testing.T) {
	var log []string
	note := func(text string) Value {
		return Eff(func(*Fiber) Value { log = append(log, text); return Unit })
	}
	// what code of its own region acquires, as generated code does
	release := func(fb *Fiber, text string) {
		owner := ReadOwnerEvidence(fb)
		Within(fb, owner).owner.add(fb, func(*Fiber, *Cause) Value { log = append(log, text); return Unit })
	}
	made := func(name string, from Value) Value {
		return Eff(func(fb *Fiber) Value {
			made := "make " + name
			if from != nil {
				made += fmt.Sprint(" on ", LazyForce(fb, TestUse(fb, from)))
			}
			log = append(log, made)
			release(fb, "release "+name)
			return name
		})
	}
	sees := func(who string, f Value) Value {
		return Eff(func(fb *Fiber) Value {
			log = append(log, fmt.Sprint(who, " sees ", LazyForce(fb, TestUse(fb, f))))
			return Unit
		})
	}
	db := TestShared("M > s > db", made("db", nil))
	conn := TestFixture("M > s > conn", made("conn", db))
	tx := TestFixture("M > s > tx", made("tx", nil))
	TestFixture("M > s > unused", made("unused", nil))
	one := Eff(func(fb *Fiber) Value {
		LazyForce(fb, TestUse(fb, conn))
		LazyForce(fb, TestUse(fb, tx))
		return Run(fb, sees("one", conn))
	})
	got := runAll([]Value{TestSerial(TestGroup("s", []Value{
		TestShare(db), TestBeforeEach(note("before_each")), TestAfterEach(sees("after_each", conn)), TestAfterAll(sees("after_all", db)),
		TestEffect("one", one), TestEffect("two", note("two")),
	}))}, marks{})
	if want := "[M > s > one passed   M > s > two passed  ]"; fmt.Sprint(got) != want {
		t.Fatalf("results\n%v\nwant\n%s", got, want)
	}
	want := "[before_each make db make conn on db make tx one sees conn after_each sees conn release tx release conn " +
		"before_each two make conn on db after_each sees conn release conn after_all sees db release db]"
	if fmt.Sprint(log) != want {
		t.Errorf("fixtures ran as\n%v\nwant\n%s", log, want)
	}
}

// The scope of a test's fixtures closes with how the test ended: what it
// owns sees a failed body, a timeout and a failed setup as such.
func TestTheFixturesOfATestCloseWithHowItEnded(t *testing.T) {
	var log []string
	tx := TestFixture("M > tx", Eff(func(fb *Fiber) Value {
		Within(fb, ReadOwnerEvidence(fb)).owner.add(fb, func(_ *Fiber, exit *Cause) Value {
			if exit == nil {
				log = append(log, "commit")
			} else {
				log = append(log, "rollback")
			}
			return Unit
		})
		return "tx"
	}))
	using := func(name string, then func(fb *Fiber) Value) Value {
		return TestEffect(name, Eff(func(fb *Fiber) Value { LazyForce(fb, TestUse(fb, tx)); return then(fb) }))
	}
	got := runAll([]Value{
		using("passes", func(*Fiber) Value { return Unit }),
		using("fails", func(*Fiber) Value { return Fail("boom") }),
		TestTimeout(10.0, using("times out", func(fb *Fiber) Value { return EffectNever(fb) })),
		TestGroup("setup", []Value{
			TestBeforeEach(Eff(func(fb *Fiber) Value { LazyForce(fb, TestUse(fb, tx)); return Fail("no setup") })),
			TestEffect("never runs", Eff(func(*Fiber) Value { return Unit })),
		}),
	}, marks{})
	if len(got) != 4 {
		t.Fatalf("results %v", got)
	}
	if want := "[commit rollback rollback rollback]"; fmt.Sprint(log) != want {
		t.Errorf("closed as %v, want %s; results %v", log, want, got)
	}
}

// What the hooks of a suite acquire is released with the suite, after its
// last test, not with the test that set it up.
func TestTheHooksOfASuiteRunInItsScope(t *testing.T) {
	var log []string
	acquire := func(text string) Value {
		return Eff(func(fb *Fiber) Value {
			Within(fb, ReadOwnerEvidence(fb)).owner.add(fb, func(*Fiber, *Cause) Value { log = append(log, text); return Unit })
			return Unit
		})
	}
	note := func(text string) Value {
		return Eff(func(*Fiber) Value { log = append(log, text); return Unit })
	}
	runAll([]Value{TestGroup("s", []Value{
		TestBeforeAll(acquire("release before_all")), TestAfterAll(acquire("release after_all")),
		TestEffect("a", note("a")), TestEffect("b", note("b")),
	})}, marks{})
	if want := "[a b release before_all release after_all]"; fmt.Sprint(log) != want {
		t.Errorf("ran as %v, want %s", log, want)
	}
}

// The tests of a Test.serial are one job, in order, where the first of them
// stands; each other test is a job of its own.
func TestTheTestsOfASerialSuiteAreOneJob(t *testing.T) {
	quick := func(name string) Value { return TestEffect(name, Eff(func(*Fiber) Value { return Unit })) }
	selected := flatten("M", []Value{
		quick("a"), TestSerial(TestGroup("s", []Value{quick("b"), TestGroup("inner", []Value{quick("c")})})), quick("d"),
	}, nil, nil, marks{})
	if got := fmt.Sprint(jobsOf(selected)); got != "[[0] [1 2] [3]]" {
		t.Errorf("jobs %s", got)
	}
}
