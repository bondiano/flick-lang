package flickrt

import (
	"math"
	"math/big"
	"testing"
)

// The hashes Effect's `Hash.hash` gives these values on the JS target.
func TestHashIsEffects(t *testing.T) {
	cases := []struct {
		value Value
		want  int32
	}{
		{int32(0), 0},
		{int32(-7), -7},
		{int32(2147483647), 1073741823},
		{1.5, -182423092},
		{math.Copysign(0, -1), 0},
		{1e300, -120444305},
		{math.NaN(), 389652371},
		{"", 5381},
		{"pear", 1014565411},
		{"hé\U0001F600", 285161945},
		{big.NewInt(12), 5859398},
		{true, 438880922},
		{false, 671588899},
		{Unit, 219449338},
		{[]Value{int32(1), "a"}, -813158543},
	}
	for _, c := range cases {
		if got := Hash(c.value); got != c.want {
			t.Errorf("hash %#v = %d, Effect gives %d", c.value, got, c.want)
		}
	}
}

func TestHashSetsWalkLowBitsFirst(t *testing.T) {
	w := &Witness{
		eq:   Curry2(func(a, b Value) Value { return a == b }),
		hash: Fn(func(a Value) Value { return a.(int32) % 3 }),
	}
	s := HashSetFromList(w, []Value{int32(7), int32(4), int32(1), int32(9), int32(6)})
	if got := Show(ListShow(Fn(Show), HashSetToList(s))); got != `"[9, 6, 7, 4, 1]"` {
		t.Errorf("equal hashes keep insertion order, by hash first: %v", got)
	}
}
