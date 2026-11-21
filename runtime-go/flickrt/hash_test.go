package flickrt

import (
	"iter"
	"math"
	"math/big"
	"strconv"
	"strings"
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

func intIdentityWitness() *Witness {
	return &Witness{
		eq:   Curry2(func(_ *Fiber, a, b Value) Value { return a == b }),
		hash: Fn(func(_ *Fiber, a Value) Value { return a.(int32) }),
	}
}

func caseBlindWitness() *Witness {
	return &Witness{
		eq: Curry2(func(_ *Fiber, a, b Value) Value {
			return strings.EqualFold(a.(string), b.(string))
		}),
		hash: Fn(func(_ *Fiber, a Value) Value {
			return Hash(strings.ToLower(a.(string)))
		}),
	}
}

func TestHashMapAndSetSeqEmptyRepeatPartialCollisionsSnapshots(t *testing.T) {
	w := intIdentityWitness()

	// Empty collections
	if got := List(SeqToList(nil, HashMapToSeq(HashMapWith(w)))); len(got) != 0 {
		t.Fatalf("empty map to_seq = %v", got)
	}
	if got := List(SeqToList(nil, HashSetToSeq(HashSetWith(w)))); len(got) != 0 {
		t.Fatalf("empty set to_seq = %v", got)
	}
	if got := HashMapFromSeq(nil, w, SeqRange(int32(0), int32(0))); hmap(got).size != 0 || hmap(got).w != w {
		t.Fatalf("empty map from_seq size=%d witness=%v", hmap(got).size, hmap(got).w == w)
	}
	if got := HashSetFromSeq(nil, w, SeqRange(int32(0), int32(0))); set(got).size != 0 || set(got).w != w {
		t.Fatalf("empty set from_seq size=%d witness=%v", set(got).size, set(got).w == w)
	}

	// Single-pass consumption + custom witness + duplicates
	mapRuns, mapPulls := 0, 0
	mapPairs := []Value{
		[]Value{"A", int32(1)},
		[]Value{"b", int32(2)},
		[]Value{"a", int32(10)},
		[]Value{"B", int32(20)},
		[]Value{"c", int32(3)},
	}
	cw := caseBlindWitness()
	mapInput := seq(func(_ *Fiber, yield func(Value) bool) {
		mapRuns++
		if mapRuns > 1 {
			t.Fatalf("HashMapFromSeq iterated input %d times", mapRuns)
		}
		for _, pair := range mapPairs {
			mapPulls++
			if !yield(pair) {
				return
			}
		}
	})
	m := HashMapFromSeq(nil, cw, mapInput)
	if mapRuns != 1 || mapPulls != 5 {
		t.Fatalf("map input runs=%d pulls=%d", mapRuns, mapPulls)
	}
	if hmap(m).w != cw || hmap(m).size != 3 {
		t.Fatalf("custom map witness=%v size=%d", hmap(m).w == cw, hmap(m).size)
	}
	if got := HashMapGet(nil, "A", m); !Equals(got, Some(int32(10))) {
		t.Fatalf("last-value-wins failed for A: %v", got)
	}
	if got := HashMapGet(nil, "b", m); !Equals(got, Some(int32(20))) {
		t.Fatalf("last-value-wins failed for b: %v", got)
	}

	setRuns, setPulls := 0, 0
	setItems := []Value{"Ada", "ada", "Bob", "BOB", "Cid"}
	setInput := seq(func(_ *Fiber, yield func(Value) bool) {
		setRuns++
		if setRuns > 1 {
			t.Fatalf("HashSetFromSeq iterated input %d times", setRuns)
		}
		for _, item := range setItems {
			setPulls++
			if !yield(item) {
				return
			}
		}
	})
	s := HashSetFromSeq(nil, cw, setInput)
	if setRuns != 1 || setPulls != 5 {
		t.Fatalf("set input runs=%d pulls=%d", setRuns, setPulls)
	}
	if set(s).w != cw || set(s).size != 3 {
		t.Fatalf("custom set witness=%v size=%d", set(s).w == cw, set(s).size)
	}
	if !Bool(HashSetMember(nil, "ADA", s)) || !Bool(HashSetMember(nil, "bob", s)) {
		t.Fatalf("custom set lost equality")
	}

	// Collisions + repeat + partial + interleaved iterators + persistent snapshots
	mod3 := &Witness{
		eq:   Curry2(func(_ *Fiber, a, b Value) Value { return a == b }),
		hash: Fn(func(_ *Fiber, a Value) Value { return a.(int32) % 3 }),
	}
	collSet := HashSetFromSeq(nil, mod3, []Value{int32(7), int32(4), int32(1), int32(9), int32(6), int32(2), int32(5), int32(8), int32(3)})
	collSeq := HashSetToSeq(collSet)
	wantPrefix := []Value{int32(9), int32(6), int32(3), int32(7)}
	wantFull := []Value{int32(9), int32(6), int32(3), int32(7), int32(4), int32(1), int32(2), int32(5), int32(8)}
	if got := SeqToList(nil, SeqTake(int32(4), collSeq)); !Equals(got, wantPrefix) {
		t.Fatalf("collision prefix = %v, want %v", got, wantPrefix)
	}
	if got := SeqToList(nil, collSeq); !Equals(got, wantFull) {
		t.Fatalf("collision full repeat = %v, want %v", got, wantFull)
	}

	// Interleaved pull iterators over the same Seq
	nextA, stopA := iter.Pull(Iter(nil, collSeq))
	defer stopA()
	nextB, stopB := iter.Pull(Iter(nil, collSeq))
	defer stopB()
	v1, _ := nextA()
	v2, _ := nextA()
	w1, _ := nextB()
	if v1 != int32(9) || v2 != int32(6) || w1 != int32(9) {
		t.Fatalf("interleaved iterators got (%v, %v, %v)", v1, v2, w1)
	}

	// Persistent snapshot isolation after owned construction (`fromSeq`)
	m0 := HashMapFromSeq(nil, w, []Value{
		[]Value{int32(0), "zero"},
		[]Value{int32(32), "thirty-two"},
		[]Value{int32(1), "one"},
		[]Value{int32(2), "two"},
	})
	m0Seq := HashMapToSeq(m0)
	_ = SeqToList(nil, SeqTake(int32(2), m0Seq))
	m1 := HashMapInsert(nil, int32(33), "thirty-three", HashMapInsert(nil, int32(0), "ZERO", HashMapRemove(nil, int32(32), m0)))
	wantM0 := []Value{
		[]Value{int32(0), "zero"},
		[]Value{int32(32), "thirty-two"},
		[]Value{int32(1), "one"},
		[]Value{int32(2), "two"},
	}
	wantM1 := []Value{
		[]Value{int32(0), "ZERO"},
		[]Value{int32(1), "one"},
		[]Value{int32(33), "thirty-three"},
		[]Value{int32(2), "two"},
	}
	if got := SeqToList(nil, m0Seq); !Equals(got, wantM0) {
		t.Fatalf("snapshot m0 mutated: got %v, want %v", got, wantM0)
	}
	if got := SeqToList(nil, HashMapToSeq(m1)); !Equals(got, wantM1) {
		t.Fatalf("derived m1 mismatch: got %v, want %v", got, wantM1)
	}
}

func TestHashSeqPrefixAvoidsEagerTraversal(t *testing.T) {
	w := intIdentityWitness()
	buildMap := func(n int32) Value {
		pairs := seq(func(_ *Fiber, yield func(Value) bool) {
			for i := int32(0); i < n; i++ {
				if !yield([]Value{i, i * 2}) {
					return
				}
			}
		})
		return HashMapFromSeq(nil, w, pairs)
	}
	smallMap := buildMap(64)
	largeMap := buildMap(4096)
	smallSet := HashSetFromSeq(nil, w, SeqRange(int32(0), int32(64)))
	largeSet := HashSetFromSeq(nil, w, SeqRange(int32(0), int32(4096)))

	wantMapPrefix := []Value{
		[]Value{int32(0), int32(0)},
		[]Value{int32(1024), int32(2048)},
		[]Value{int32(2048), int32(4096)},
	}
	if got := SeqToList(nil, SeqTake(int32(3), HashMapToSeq(largeMap))); !Equals(got, wantMapPrefix) {
		t.Fatalf("large map prefix = %v, want %v", got, wantMapPrefix)
	}
	wantSetPrefix := []Value{int32(0), int32(1024), int32(2048)}
	if got := SeqToList(nil, SeqTake(int32(3), HashSetToSeq(largeSet))); !Equals(got, wantSetPrefix) {
		t.Fatalf("large set prefix = %v, want %v", got, wantSetPrefix)
	}

	// Allocations for consuming a 3-element prefix must stay O(1) and not grow with collection size.
	smallMapAllocs := testing.AllocsPerRun(50, func() {
		_ = SeqToList(nil, SeqTake(int32(3), HashMapToSeq(smallMap)))
	})
	largeMapAllocs := testing.AllocsPerRun(50, func() {
		_ = SeqToList(nil, SeqTake(int32(3), HashMapToSeq(largeMap)))
	})
	if largeMapAllocs > 32 || largeMapAllocs > smallMapAllocs+4 {
		t.Fatalf("HashMap.to_seq prefix looks eager: small=%.1f allocs, large=%.1f allocs", smallMapAllocs, largeMapAllocs)
	}

	smallSetAllocs := testing.AllocsPerRun(50, func() {
		_ = SeqToList(nil, SeqTake(int32(3), HashSetToSeq(smallSet)))
	})
	largeSetAllocs := testing.AllocsPerRun(50, func() {
		_ = SeqToList(nil, SeqTake(int32(3), HashSetToSeq(largeSet)))
	})
	if largeSetAllocs > 32 || largeSetAllocs > smallSetAllocs+4 {
		t.Fatalf("HashSet.to_seq prefix looks eager: small=%.1f allocs, large=%.1f allocs", smallSetAllocs, largeSetAllocs)
	}
}

var benchmarkHashSeqSink Value

func BenchmarkHashSeq(b *testing.B) {
	w := intIdentityWitness()
	for _, size := range []int32{1024, 4096} {
		pairs := seq(func(_ *Fiber, yield func(Value) bool) {
			for i := int32(0); i < size; i++ {
				if !yield([]Value{i, i * 2}) {
					return
				}
			}
		})
		ints := SeqRange(int32(0), size)
		m := HashMapFromSeq(nil, w, pairs)
		s := HashSetFromSeq(nil, w, ints)

		b.Run(strconv.Itoa(int(size)), func(b *testing.B) {
			b.Run("MapPrefix5Lazy", func(b *testing.B) {
				b.ReportAllocs()
				for range b.N {
					benchmarkHashSeqSink = SeqToList(nil, SeqTake(int32(5), HashMapToSeq(m)))
				}
			})
			b.Run("MapPrefix5Eager", func(b *testing.B) {
				b.ReportAllocs()
				for range b.N {
					benchmarkHashSeqSink = SeqToList(nil, SeqTake(int32(5), SeqOfList(HashMapToList(m))))
				}
			})
			b.Run("SetPrefix5Lazy", func(b *testing.B) {
				b.ReportAllocs()
				for range b.N {
					benchmarkHashSeqSink = SeqToList(nil, SeqTake(int32(5), HashSetToSeq(s)))
				}
			})
			b.Run("SetPrefix5Eager", func(b *testing.B) {
				b.ReportAllocs()
				for range b.N {
					benchmarkHashSeqSink = SeqToList(nil, SeqTake(int32(5), SeqOfList(HashSetToList(s))))
				}
			})
			b.Run("MapFromSeq", func(b *testing.B) {
				b.ReportAllocs()
				for range b.N {
					benchmarkHashSeqSink = HashMapFromSeq(nil, w, pairs)
				}
			})
			b.Run("MapFromListOfSeq", func(b *testing.B) {
				b.ReportAllocs()
				for range b.N {
					t := empty(w)
					for _, kv := range List(SeqToList(nil, pairs)) {
						pair := kv.([]Value)
						t = t.set(nil, pair[0], pair[1], t.w.of(nil, pair[0]), false)
					}
					benchmarkHashSeqSink = mapOf(t)
				}
			})
			b.Run("SetFromSeq", func(b *testing.B) {
				b.ReportAllocs()
				for range b.N {
					benchmarkHashSeqSink = HashSetFromSeq(nil, w, ints)
				}
			})
			b.Run("SetFromListOfSeq", func(b *testing.B) {
				b.ReportAllocs()
				for range b.N {
					t := empty(w)
					for _, x := range List(SeqToList(nil, ints)) {
						t = t.set(nil, x, Unit, t.w.of(nil, x), true)
					}
					benchmarkHashSeqSink = setOf(t)
				}
			})
		})
	}
}
