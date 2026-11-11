package flickrt

import (
	"math/rand"
	"reflect"
	"runtime"
	"sort"
	"testing"
)

func TestASortKeepsTheOrderOfEqualElements(t *testing.T) {
	// pairs ordered by their first number only: the second tells where each was
	byFirst := Fn(func(_ *Fiber, a Value) Value {
		return Fn(func(_ *Fiber, b Value) Value {
			switch x, y := a.([]Value)[0].(int32), b.([]Value)[0].(int32); {
			case x < y:
				return LT
			case x > y:
				return GT
			}
			return EQ
		})
	})
	random := rand.New(rand.NewSource(7))
	for _, size := range []int{0, 1, 2, 7, 8, 9, 16, 17, 24, 33, 500, 1000} {
		xs := make([]Value, size)
		for i := range xs {
			xs[i] = []Value{int32(random.Intn(size/4 + 1)), int32(i)}
		}
		want := append([]Value{}, xs...)
		sort.SliceStable(want, func(i, j int) bool {
			return want[i].([]Value)[0].(int32) < want[j].([]Value)[0].(int32)
		})
		before := append([]Value{}, xs...)
		if got := ListSortBy(nil, byFirst, xs); !reflect.DeepEqual(got, Value(want)) {
			t.Errorf("%d elements: %v, want %v", size, got, want)
		}
		if !reflect.DeepEqual(xs, before) {
			t.Errorf("%d elements: the list given was changed", size)
		}
	}
}

func TestAnUpdateOfARecordIsItsFieldsThenTheNewOnes(t *testing.T) {
	base := Rec("b", int32(1), "a", int32(2), "10", int32(3), "c", int32(4), "d", int32(5))
	updates := []Value{
		Rec(),
		Rec("a", "A"),
		Rec("d", "D", "b", "B"),
		Rec("e", "E"),
		Rec("a", "A", "e", "E", "2", "two"),
		Rec("b", "B", "a", "A", "10", "ten", "c", "C", "d", "D"),
	}
	for _, fields := range updates {
		// the reference: every field put into a copy, one at a time
		want := record(base)
		f := record(fields)
		for i, k := range f.keys {
			want = want.set(k, f.vals[i])
		}
		got := record(Update(base, fields))
		if !reflect.DeepEqual(got.keys, want.keys) || !reflect.DeepEqual(got.vals, want.vals) {
			t.Errorf("update with %v: %v %v, want %v %v", f.keys, got.keys, got.vals, want.keys, want.vals)
		}
	}
	if b := record(base); !reflect.DeepEqual(b.vals, []Value{int32(3), int32(1), int32(2), int32(4), int32(5)}) {
		t.Errorf("the record updated was changed: %v", b.vals)
	}
	wide := Rec("a", 1, "b", 2, "c", 3, "d", 4, "e", 5, "f", 6, "g", 7, "h", 8, "i", 9)
	if got := record(Update(wide, Rec("i", "I"))); got.Get("i") != "I" || got.Get("a") != 1 || len(got.vals) != 9 {
		t.Errorf("a record of nine fields: %v", got.vals)
	}
}

func TestAnUpdatedRecordDoesNotKeepTheValuesItReplaced(t *testing.T) {
	// a record of eight megabytes, updated to hold nothing: with the first
	// record dropped, its payload must be free
	held := func() Value {
		before := Rec("id", int32(1), "payload", make([]byte, 8<<20))
		return Update(before, Rec("payload", ""))
	}
	var start, end runtime.MemStats
	runtime.GC()
	runtime.ReadMemStats(&start)
	kept := held()
	runtime.GC()
	runtime.ReadMemStats(&end)
	if grown := int64(end.HeapAlloc) - int64(start.HeapAlloc); grown > 1<<20 {
		t.Errorf("the updated record keeps %d bytes alive", grown)
	}
	runtime.KeepAlive(kept)
}

func TestTheCharactersOfAStringAreItsCodePoints(t *testing.T) {
	for _, s := range unitStrings {
		want := []Value{}
		for rest := s; len(rest) > 0; {
			_, n := decode(rest)
			want = append(want, rest[:n])
			rest = rest[n:]
		}
		if got := Chars(s); !reflect.DeepEqual(got, Value(want)) {
			t.Errorf("Chars of %q: %v, want %v", s, got, want)
		}
	}
}

func TestTheHashOfABoolAndOfUnitIsOfItsJsText(t *testing.T) {
	for value, text := range map[Value]string{true: "true", false: "false", Unit: "undefined"} {
		if got, want := hash(value), optimize(mix(hashString(text))); got != want {
			t.Errorf("hash of %v: %d, want %d", value, got, want)
		}
	}
}
