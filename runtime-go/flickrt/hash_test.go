package flickrt

import (
	"math"
	"math/big"
	"strconv"
	"testing"
)

// The hashes the Effect library gives these values on the JS target.
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
		eq:   Curry2(func(fb *Fiber, a, b Value) Value { return a == b }),
		hash: Fn(func(fb *Fiber, a Value) Value { return a.(int32) % 3 }),
	}
	s := HashSetFromList(nil, w, []Value{int32(7), int32(4), int32(1), int32(9), int32(6)})
	if got := Show(ListShow(nil, Fn(func(_ *Fiber, v Value) Value { return Show(v) }), HashSetToList(s))); got != `"[9, 6, 7, 4, 1]"` {
		t.Errorf("equal hashes keep insertion order, by hash first: %v", got)
	}
}

func stringWitness() *Witness {
	return &Witness{
		eq:   Curry2(func(_ *Fiber, a, b Value) Value { return a == b }),
		hash: Fn(func(_ *Fiber, a Value) Value { return Hash(a) }),
	}
}

// Inserts, replacements and removals keep the walk order of the HAMT: by
// hash five bits at a time from the lowest, then by insertion.
func TestHashMapKeepsWalkOrderThroughChanges(t *testing.T) {
	w := stringWitness()
	// few distinct hashes, so many collide
	w.hash = Fn(func(_ *Fiber, a Value) Value { return Hash(a).(int32) & 0x8421 })
	m := HashMapWith(w)
	var keys []string
	for i := 0; i < 2000; i++ {
		k := strconv.Itoa(i * 7919 % 2003)
		m = HashMapInsert(nil, k, int32(i), m)
		keys = append(keys, k)
		if i%5 == 0 {
			m = HashMapRemove(nil, keys[i/2], m)
		}
	}
	entries := hmap(m).all()
	for i := 1; i < len(entries); i++ {
		a, b := walk(entries[i-1].hash), walk(entries[i].hash)
		if a > b {
			t.Fatalf("entry %d out of walk order", i)
		}
	}
	if int(hmap(m).size) != len(entries) {
		t.Fatalf("size %d, %d entries", hmap(m).size, len(entries))
	}
	for _, e := range entries {
		if v, ok := hmap(m).get(nil, e.key, e.hash); !ok || v != e.value {
			t.Fatalf("lost %v", e.key)
		}
	}
}

func BenchmarkHashMapInsert(b *testing.B) {
	keys := make([]Value, 10000)
	for i := range keys {
		keys[i] = "key-" + strconv.Itoa(i)
	}
	for n := 0; n < b.N; n++ {
		m := HashMapWith(stringWitness())
		for i, k := range keys {
			m = HashMapInsert(nil, k, int32(i), m)
		}
	}
}
