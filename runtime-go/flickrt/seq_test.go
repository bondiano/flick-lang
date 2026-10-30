package flickrt

import "testing"

func TestSeqsRestartAndStopEarly(t *testing.T) {
	pulled := 0
	counted := SeqMap(Fn(func(fb *Fiber, x Value) Value { pulled++; return x }), SeqFrom(int32(1)))
	taken := SeqTake(int32(3), counted)
	for range 2 {
		if got := Show(ListShow(nil, Fn(func(_ *Fiber, v Value) Value { return Show(v) }), SeqToList(nil, taken))); got != `"[1, 2, 3]"` {
			t.Fatalf("each iteration starts over: %v", got)
		}
	}
	if pulled != 6 {
		t.Errorf("take pulls none after the last: %d pulled", pulled)
	}
	zipped := SeqZip(SeqOfList([]Value{"a", "b"}), SeqFrom(int32(0)))
	if len(List(SeqToList(nil, zipped))) != 2 || len(List(SeqToList(nil, zipped))) != 2 {
		t.Error("zip restarts and stops at the shorter one")
	}
}
