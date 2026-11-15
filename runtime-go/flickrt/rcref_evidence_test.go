package flickrt

import (
	"slices"
	"testing"
)

// The rank-polymorphic acquisition receives fresh evidence for its private owner,
// independently of the region in which the reference itself was made.
func acquireInFreshOwner(n *notes, region Value, name string) Value {
	return Eff(func(fb *Fiber) Value {
		owner := ReadOwnerEvidence(fb)
		if owner == region {
			n.add("acquisition reused the reference owner")
		}
		fb = Within(fb, owner)
		n.add("acquire " + name)
		name := name
		fb.owner.add(fb, func(*Fiber, *Cause) Value { n.add("release " + name); return Unit })
		return name
	})
}

func TestRcRefReleaseTimingUsesFreshAcquisitionEvidence(t *testing.T) {
	var n notes
	fb := &Fiber{state: newState(), owner: newOwner()}
	Scope(fb, func(fb *Fiber, region, _ Value) Value {
		fb = WithOwnerEvidence(fb, region)
		acquire := acquireInFreshOwner(&n, region, "shared")
		ref := RcRefMake(fb, Rec("acquire", acquire, "idleTimeToLive", None))
		Scope(fb, func(fb *Fiber, owner, _ Value) Value {
			fb = WithOwnerEvidence(fb, owner)
			a := RcRefGet(fb, ref).(string)
			b := RcRefGet(fb, ref).(string)
			n.add("rc ref: " + a + ", " + b)
			return Unit
		})
		n.add("holders closed")
		Scope(fb, func(fb *Fiber, owner, _ Value) Value {
			fb = WithOwnerEvidence(fb, owner)
			n.add("again: " + RcRefGet(fb, ref).(string))
			return Unit
		})
		RcRefInvalidate(fb, ref)
		return Unit
	})
	want := []string{
		"acquire shared",
		"rc ref: shared, shared",
		"release shared",
		"holders closed",
		"acquire shared",
		"again: shared",
		"release shared",
	}
	if got := n.get(); !slices.Equal(got, want) {
		t.Fatalf("got %#v\nwant %#v", got, want)
	}
}
