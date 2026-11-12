package flickrt

import "testing"

// A child closes `done` before its `ended` runs, so a resolver that waits
// for `done` must still see every entry complete when it returns; the gap
// is short, hence the many rounds.
func TestRequestResolverFromEffectCompletesEveryEntryItWaitedFor(t *testing.T) {
	fb := rootFiber()
	resolver := RequestResolverFromEffect(Fn(func(_ *Fiber, entry Value) Value {
		return Eff(func(*Fiber) Value { return RequestRequest(entry) })
	}))
	for round := range 5000 {
		owner := newOwner()
		children := make([]*child, 8)
		for i := range children {
			request := RequestOf(float64(i))
			children[i] = fork(fb, owner, Eff(func(fb *Fiber) Value { return RequestRun(fb, resolver, request) }), nil)
		}
		for i, c := range children {
			wait(fb, c.done)
			if c.cause != nil || c.value != float64(i) {
				t.Fatalf("round %d, request %d: value %v, cause %+v", round, i, c.value, c.cause)
			}
		}
	}
}
