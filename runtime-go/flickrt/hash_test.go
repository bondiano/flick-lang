package flickrt

import (
	"fmt"
	"math"
	"math/big"
	"math/rand"
	"slices"
	"strconv"
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

// walkKey is where a hash falls in the JS target's walk: its five-bit
// chunks from the lowest, the lowest most significant.
func walkKey(h int32) uint64 {
	u, key := uint32(h), uint64(0)
	for shift := 0; shift < 35; shift += 5 {
		key = key<<5 | uint64(u>>shift&31)
	}
	return key
}

// model is the reference: entries sorted by walk key, then by insertion.
type model []entry

func (m model) set(k, v Value, h int32) model {
	for i := range m {
		if m[i].key == k {
			m[i].value = v
			return m
		}
	}
	i := len(m)
	for i > 0 && walkKey(m[i-1].hash) > walkKey(h) {
		i--
	}
	return slices.Insert(m, i, entry{k, v, h})
}

func (m model) remove(k Value) model {
	return slices.DeleteFunc(m, func(e entry) bool { return e.key == k })
}

// checkNode fails on a node Effect's HashMap never builds.
func checkNode(t *testing.T, n node, shift uint) int {
	switch x := n.(type) {
	case *entry:
		return 1
	case *collision:
		if len(x.entries) < 2 {
			t.Fatalf("collision of %d entries", len(x.entries))
		}
		return len(x.entries)
	case *indexed:
		if len(x.children) > maxIndexed || len(x.children) != bitCount(x.bitmap) {
			t.Fatalf("indexed node of %d children, bitmap %b", len(x.children), x.bitmap)
		}
		size := 0
		for _, c := range x.children {
			size += checkNode(t, c, shift+levelBits)
		}
		return size
	case *array:
		size, count := 0, 0
		for _, c := range x.children {
			if c != nil {
				count++
				size += checkNode(t, c, shift+levelBits)
			}
		}
		if count != x.count || count < minArray {
			t.Fatalf("array node counts %d of %d children", x.count, count)
		}
		return size
	}
	return 0
}

func bitCount(b uint32) (n int) {
	for ; b != 0; b &= b - 1 {
		n++
	}
	return
}

// intWitness hashes Ints by `hash`; its keys compare by value.
func intWitness(hash func(int32) int32) *Witness {
	return &Witness{
		eq:   Curry2(func(a, b Value) Value { return a == b }),
		hash: Fn(func(a Value) Value { return hash(a.(int32)) }),
	}
}

// Random inserts, updates and removals, then removing every key, walk in
// the reference's order, over hashes that collide often, share long
// prefixes, or spread out.
func TestHashMapWalksAsEffect(t *testing.T) {
	hashes := map[string]func(int32) int32{
		"collide": func(k int32) int32 { return k % 7 },
		"prefix":  func(k int32) int32 { return k % 5 << 25 },
		"spread":  func(k int32) int32 { return int32(uint32(k) * 2654435761) },
		"full":    func(k int32) int32 { return k },
	}
	for name, hash := range hashes {
		t.Run(name, func(t *testing.T) {
			r := rand.New(rand.NewSource(1))
			m, ref := HashMapWith(intWitness(hash)), model(nil)
			check := func(step int) {
				if got := checkNode(t, hmap(m).root, 0); got != len(ref) || hmap(m).size != len(ref) {
					t.Fatalf("step %d: %d entries, size %d, want %d", step, got, hmap(m).size, len(ref))
				}
				want := make([]Value, len(ref))
				for i, e := range ref {
					want[i] = []Value{e.key, e.value}
				}
				if got := HashMapToList(m); !Equals(got, want) {
					t.Fatalf("step %d: walk %v, want %v", step, Show(got), Show(want))
				}
			}
			for step := 0; step < 4000; step++ {
				k := int32(r.Intn(600))
				if r.Intn(3) == 0 {
					m, ref = HashMapRemove(k, m), ref.remove(k)
				} else {
					v := int32(r.Intn(4))
					m, ref = HashMapInsert(k, v, m), ref.set(k, v, hash(k))
				}
				if step%97 == 0 {
					check(step)
				}
			}
			for step, k := range r.Perm(600) {
				m, ref = HashMapRemove(int32(k), m), ref.remove(int32(k))
				if step%7 == 0 || len(ref) < 20 {
					check(step)
				}
			}
			if hmap(m).root != nil {
				t.Fatal("an empty map keeps nodes")
			}
		})
	}
}

// An older map is untouched by updates of a newer one.
func TestHashMapIsPersistent(t *testing.T) {
	w := intWitness(func(k int32) int32 { return k % 40 })
	m := HashMapWith(w)
	for i := int32(0); i < 300; i++ {
		m = HashMapInsert(i, i, m)
	}
	before := Show(HashMapToList(m))
	next := m
	for i := int32(0); i < 300; i += 3 {
		next = HashMapRemove(i, HashMapInsert(i+1, int32(-1), next))
	}
	if Show(HashMapToList(m)) != before || HashMapSize(m) != int32(300) {
		t.Fatal("an update changed the map it was made from")
	}
	if HashMapGet(int32(1), next) == None || HashMapGet(int32(3), next) != None {
		t.Fatal("the updates are not in the new map")
	}
}

func stringKeys(n int) []Value {
	keys := make([]Value, n)
	for i := range keys {
		keys[i] = "key-" + strconv.Itoa(i)
	}
	return keys
}

var stringWitness = &Witness{
	eq:   Curry2(func(a, b Value) Value { return a.(string) == b.(string) }),
	hash: Fn(Hash),
}

// The cost per operation grows with the depth of the trie, a level per
// 32× more keys, not with the size.
func BenchmarkHashMap(b *testing.B) {
	for _, n := range []int{1000, 10000, 100000} {
		keys := stringKeys(n)
		full := HashMapWith(stringWitness)
		for i, k := range keys {
			full = HashMapInsert(k, int32(i), full)
		}
		b.Run(fmt.Sprintf("insert/%d", n), func(b *testing.B) {
			for range b.N {
				m := HashMapWith(stringWitness)
				for i, k := range keys {
					m = HashMapInsert(k, int32(i), m)
				}
			}
			b.ReportMetric(float64(b.Elapsed().Nanoseconds())/float64(b.N*n), "ns/key")
		})
		b.Run(fmt.Sprintf("get/%d", n), func(b *testing.B) {
			for range b.N {
				for _, k := range keys {
					HashMapGet(k, full)
				}
			}
			b.ReportMetric(float64(b.Elapsed().Nanoseconds())/float64(b.N*n), "ns/key")
		})
		b.Run(fmt.Sprintf("remove/%d", n), func(b *testing.B) {
			for range b.N {
				m := full
				for _, k := range keys {
					m = HashMapRemove(k, m)
				}
			}
			b.ReportMetric(float64(b.Elapsed().Nanoseconds())/float64(b.N*n), "ns/key")
		})
	}
}
