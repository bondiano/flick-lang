package flickrt

import (
	"slices"
	"testing"
)

func TestOrderedMapEffectParKeepsTheOrderOfItsElements(t *testing.T) {
	second := make(chan struct{})
	// the first element ends only once the second has
	f := func(fb *Fiber, a Value) Value {
		if a == int32(1) {
			wait(fb, second)
		} else {
			close(second)
		}
		return a.(int32) * 10
	}
	s := streamMapEffect(chunkStream(ints(1, 2)), f, 2, false)
	if got := List(StreamRunCollect(rootFiber(), s)); !slices.Equal(got, ints(10, 20)) {
		t.Errorf("collected %v", got)
	}
}

func TestUnorderedMapEffectParEmitsAllOfItsElements(t *testing.T) {
	s := streamMapEffect(chunkStream(ints(1, 2, 3, 4)), func(_ *Fiber, a Value) Value { return a }, 3, true)
	got := List(StreamRunCollect(rootFiber(), s))
	slices.SortFunc(got, func(a, b Value) int { return int(a.(int32) - b.(int32)) })
	if !slices.Equal(got, ints(1, 2, 3, 4)) {
		t.Errorf("collected %v", got)
	}
}

func TestRaceAllEmitsTheStreamThatEmitsFirst(t *testing.T) {
	s := streamRaceAll([]Value{&Stream{channelNever}, chunkStream(ints(1, 2))})
	if got := List(StreamRunCollect(rootFiber(), s)); !slices.Equal(got, ints(1, 2)) {
		t.Errorf("collected %v", got)
	}
}

func TestZipLatestAllWaitsForEveryStream(t *testing.T) {
	s := streamZipLatestAll([]Value{chunkStream(ints(1)), chunkStream(ints(2))})
	got := List(StreamRunCollect(rootFiber(), s))
	if len(got) != 1 || !slices.Equal(List(got[0]), ints(1, 2)) {
		t.Errorf("collected %v", got)
	}
}

func TestSubscriptionRefChangesStartWithTheCurrentValue(t *testing.T) {
	fb := rootFiber()
	r := SubscriptionRefMake(fb, int32(1))
	SyncRefSet(fb, int32(2), r)
	s := StreamTake(int32(1), SubscriptionRefChanges(r))
	if got := List(StreamRunCollect(fb, s)); !slices.Equal(got, ints(2)) {
		t.Errorf("collected %v", got)
	}
}
