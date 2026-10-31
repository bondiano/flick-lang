package flickrt

import (
	"math"
	"testing"
)

// The histogram counts a value equal to a boundary in that boundary's
// bucket and one past the last in none of them, as Effect's does.
func TestHistogramBuckets(t *testing.T) {
	c := metricContext{rootFiber(), noAttributes}
	h := newHistogram("test_histogram_buckets", None, nil, []Value{10.0, 1.0, 5.0})
	for _, v := range []float64{1, 1.5, 5, 10, 11} {
		h.update(c, v)
	}
	state := record(h.value(c))
	got := Format(state.Get("buckets"))
	if want := "[[1,1],[5,3],[10,4]]"; got != want {
		t.Errorf("buckets: got %s, want %s", got, want)
	}
	if state.Get("count") != 5.0 || state.Get("min") != 1.0 || state.Get("max") != 11.0 {
		t.Errorf("state: %s", Format(state))
	}
}

// A summary keeps its last `maxSize` values and reads its quantiles as
// Effect's does: the value at ceil(q * n) - 1 of the sorted samples.
func TestSummaryQuantiles(t *testing.T) {
	c := metricContext{rootFiber(), noAttributes}
	now := clockOf(c.fb).millis()
	s := newSummary("test_summary_quantiles", None, nil, math.Inf(1), 4, []Value{1.0, 0.5, 0.0})
	for _, v := range []float64{100, 3, 1, 4, 2} {
		s.update(c, []Value{v, now})
	}
	got := Format(record(s.value(c)).Get("quantiles"))
	if want := Format([]Value{[]Value{0.0, Some(1.0)}, []Value{0.5, Some(2.0)}, []Value{1.0, Some(4.0)}}); got != want {
		t.Errorf("quantiles: got %s, want %s", got, want)
	}
}

func TestEscapeDoubleQuotes(t *testing.T) {
	for in, want := range map[string]string{`a "b"`: `"a \"b\""`, `\"x`: `"\"x"`, `end\`: `"end\"`} {
		if got := escapeDoubleQuotes(in); got != want {
			t.Errorf("%s: got %s, want %s", in, got, want)
		}
	}
}
