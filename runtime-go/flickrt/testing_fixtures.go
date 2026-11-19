package flickrt

import (
	"strings"
	"sync"
)

// The fixtures of the tests of a suite, as the JS runtime's: a fixture is
// made by its initializer the first time code uses it, for each test or,
// shared, once for each run of the suite whose list shares it, in a scope
// of that test's or that run's own, which closes after the hooks that run
// when the test's or the suite's scope closes, as those may use it.

// fixture is a Test.Fixture: `init` makes it, `key` names it.
type fixture struct {
	key    string
	shared bool
	init   Value
}

// TestFixture and TestShared are Test.fixture and Test.shared.
func TestFixture(key, init Value) Value { return &fixture{key: key.(string), init: init} }
func TestShared(key, init Value) Value  { return &fixture{key: key.(string), shared: true, init: init} }

// TestShare is Test.share: the item that makes its group the owner of the
// shared fixture `f`.
func TestShare(f Value) Value { return &test{name: "share", share: f.(*fixture)} }

// sharesOf is the shared fixtures `tests` share, nil when there are none.
func sharesOf(tests []Value) map[*fixture]bool {
	var shares map[*fixture]bool
	for _, v := range tests {
		if f := v.(*test).share; f != nil {
			if shares == nil {
				shares = map[*fixture]bool{}
			}
			shares[f] = true
		}
	}
	return shares
}

// fixtureCells is the fixtures of a test or a suite's run, each made the
// first time code uses it, in `owner`, a scope of their own.
type fixtureCells struct {
	mu    sync.Mutex
	cells map[*fixture]*LazyCell
	owner *Owner
	// how a test failed that its scope's exit does not show: its setup
	failed *Cause
	// the first fixture whose initializer failed, and how: what makes its
	// case a setup failure
	broken      *fixture
	brokenCause *Cause
}

// cellsIn is the fixtures of a test or a suite's run whose scope is `outer`,
// which closes them from now on, after what it closes later, with how its
// body ended: what they own sees whether the test passed.
func cellsIn(fb *Fiber, outer *Owner) *fixtureCells {
	cells := &fixtureCells{cells: map[*fixture]*LazyCell{}, owner: newOwner()}
	cells.owner.defects = outer.defects
	outer.add(fb, func(fb *Fiber, exit *Cause) Value {
		if exit == nil {
			exit = cells.failed
		}
		if c := cells.owner.closeFinalizers(fb, exit); c != nil {
			panic(&Failure{c})
		}
		return Unit
	})
	return cells
}

// of is the cell of `f`, made the first time: its initializer runs in the
// scope of the cells, with it as the owner of what it acquires.
func (c *fixtureCells) of(f *fixture) *LazyCell {
	c.mu.Lock()
	defer c.mu.Unlock()
	if cell, ok := c.cells[f]; ok {
		return cell
	}
	owner := c.owner
	return c.cellWith(f, Eff(func(fb *Fiber) Value {
		return Run(Within(WithOwnerEvidence(fb, owner), owner), f.init)
	}))
}

// cellWith is the cell of `f` made by `init`, which notes its first failure;
// the caller holds the lock.
func (c *fixtureCells) cellWith(f *fixture, init Value) *LazyCell {
	cell := &LazyCell{task: Eff(func(fb *Fiber) Value {
		v, cause := catch(func() Value { return Run(fb, init) })
		if cause == nil {
			return v
		}
		if !interruptsOnly(cause) {
			c.mu.Lock()
			if c.broken == nil {
				c.broken, c.brokenCause = f, cause
			}
			c.mu.Unlock()
		}
		panic(&Failure{cause})
	})}
	c.cells[f] = cell
	return cell
}

// view is the cell through which a test sees the shared `cell` of `f`,
// which notes a failure for the test.
func (c *fixtureCells) view(f *fixture, shared *LazyCell) *LazyCell {
	c.mu.Lock()
	defer c.mu.Unlock()
	if cell, ok := c.cells[f]; ok {
		return cell
	}
	return c.cellWith(f, Eff(func(fb *Fiber) Value { return LazyForce(fb, shared) }))
}

// brokenSetup is the setup failure of a test whose body failed with `exit`
// after a fixture of it failed to be made, nil otherwise.
func (c *fixtureCells) brokenSetup(exit *Cause) *setupFailure {
	if c == nil || exit == nil || interruptsOnly(exit) {
		return nil
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.broken == nil {
		return nil
	}
	name := c.broken.key[strings.LastIndex(c.broken.key, " > ")+3:]
	return &setupFailure{text: "fixture " + name + ": " + (&setupFailure{cause: c.brokenCause}).message()}
}

// fixtures is what code finds its fixtures in: those of the test it runs
// for, if any, and the suites around it, outermost first, whose shared
// fixtures it sees.
type fixtures struct {
	test   *fixtureCells
	suites []*test
	runs   map[*test]*suiteRun
}

const fixturesKey = "flick/Test/fixtures"

// withFixtures is `task` with `fx` for the fixtures it uses.
func withFixtures(fx *fixtures, task Value) Value {
	return Eff(func(fb *Fiber) Value {
		inner := *fb
		inner.env = fb.env.with(fixturesKey, fx)
		return Run(&inner, task)
	})
}

// withEach is withFixtures of each of `tasks`.
func withEach(fx *fixtures, tasks []Value) []Value {
	out := make([]Value, len(tasks))
	for i, task := range tasks {
		out[i] = withFixtures(fx, task)
	}
	return out
}

// TestUse is Test.use: the Lazy of `f` for the test this code runs for, or
// for the run of the innermost suite around it that shares it.
func TestUse(fb *Fiber, f Value) Value {
	fixture := f.(*fixture)
	v, _ := fb.env.get(fixturesKey)
	fx, ok := v.(*fixtures)
	if !ok {
		panic(Defect{"the fixture " + fixture.key + " runs only in the code of a test or a suite that `flick test` runs"})
	}
	if fixture.shared {
		for i := len(fx.suites) - 1; i >= 0; i-- {
			if s := fx.suites[i]; s.shares[fixture] {
				shared := fx.runs[s].shared.of(fixture)
				// each test sees the shared cell through one of its own, which notes a failure for it
				if fx.test == nil {
					return shared
				}
				return fx.test.view(fixture, shared)
			}
		}
		panic(Defect{"the shared fixture " + fixture.key + ", used by code that no suite sharing it runs"})
	}
	if fx.test == nil {
		panic(Defect{"the fixture " + fixture.key + " of a test, used by code that runs for no test"})
	}
	return fx.test.of(fixture)
}
