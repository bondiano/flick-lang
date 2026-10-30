package flickrt

import (
	"math/rand"
	"slices"
	"testing"
)

// The sort is stable and agrees with the standard library's.
func TestListSortByIsStable(t *testing.T) {
	byKey := Curry2(func(a, b Value) Value { return Compare(a.([]Value)[0], b.([]Value)[0]) })
	r := rand.New(rand.NewSource(1))
	for _, n := range []int{0, 1, 2, 12, 13, 100, 1000} {
		for _, keys := range []int{1, 3, n + 1} {
			in := make([]Value, n)
			for i := range in {
				in[i] = []Value{int32(r.Intn(keys)), int32(i)}
			}
			want := slices.Clone(in)
			slices.SortStableFunc(want, func(a, b Value) int { return int(a.([]Value)[0].(int32) - b.([]Value)[0].(int32)) })
			if got := ListSortBy(byKey, in); !Equals(got, want) {
				t.Fatalf("%d elements of %d keys: %v", n, keys, Show(got))
			}
		}
	}
}
