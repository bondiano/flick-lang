package flickrt

import (
	"reflect"
	"testing"
)

// A record keeps its labels as JavaScript keeps the keys of an object:
// those that are array indices first, ascending, then the others as written.
func TestARecordOfAShapeKeepsItsLabelsAsJavaScriptKeepsKeys(t *testing.T) {
	cases := []struct {
		labels []string
		keys   []string
		vals   []Value
	}{
		{[]string{"a"}, []string{"a"}, []Value{0}},
		{[]string{"b", "a"}, []string{"b", "a"}, []Value{0, 1}},
		{[]string{"b", "10", "a", "2"}, []string{"2", "10", "b", "a"}, []Value{3, 1, 0, 2}},
		{[]string{"1", "0"}, []string{"0", "1"}, []Value{1, 0}},
		{[]string{"01", "1", "-1", "1.5"}, []string{"1", "01", "-1", "1.5"}, []Value{1, 0, 2, 3}},
		{
			[]string{"i", "h", "g", "f", "e", "d", "c", "b", "a", "0"},
			[]string{"0", "i", "h", "g", "f", "e", "d", "c", "b", "a"},
			[]Value{9, 0, 1, 2, 3, 4, 5, 6, 7, 8},
		},
	}
	for _, c := range cases {
		vals := make([]Value, len(c.labels))
		for i := range vals {
			vals[i] = i
		}
		got := record(Shaped(ShapeOf(c.labels...), vals...))
		if !reflect.DeepEqual(got.keys, c.keys) || !reflect.DeepEqual(got.vals, c.vals) {
			t.Errorf("%v: %v %v, want %v %v", c.labels, got.keys, got.vals, c.keys, c.vals)
		}
	}
}

func TestARecordOfAShapeIsTheRecordOfItsLabelsAndValues(t *testing.T) {
	literals := [][]string{
		{"a"},
		{"b", "a"},
		{"b", "10", "a", "2"},
		{"a", "b", "c", "d", "e"},
		{"i", "h", "g", "f", "e", "d", "c", "b", "a", "0"},
	}
	for _, labels := range literals {
		shape := ShapeOf(labels...)
		vals, fields := make([]Value, len(labels)), []Value{}
		for i, label := range labels {
			vals[i] = int32(i)
			fields = append(fields, label, int32(i))
		}
		want := record(Rec(fields...))
		same := func(when string) {
			got := record(Shaped(shape, vals...))
			if !reflect.DeepEqual(got.keys, want.keys) || !reflect.DeepEqual(got.vals, want.vals) {
				t.Errorf("%v %s: %v %v, want %v %v", labels, when, got.keys, got.vals, want.keys, want.vals)
			}
		}
		same("made")
		// records that grow from one of the shape leave its labels as they were
		first := Shaped(shape, vals...)
		grown := record(Update(first, Rec("zz", "new", "1", "one")))
		if len(grown.keys) != len(labels)+2 || grown.Get("zz") != "new" || grown.Get(labels[0]) != int32(0) {
			t.Errorf("%v grown: %v %v", labels, grown.keys, grown.vals)
		}
		record(first).set("yy", "new")
		record(first).set(labels[0], "set")
		same("after other records grew from it")
	}
}

// `{ r with … }` gives the record an update by a record gives, and makes
// nothing but that record.
func TestARecordWithNewValuesIsOneRecord(t *testing.T) {
	base := Rec("b", "B", "10", "ten", "a", "A", "2", "two")
	cases := []struct {
		labels []string
		vals   []Value
		keys   []string
		want   []Value
	}{
		{[]string{"a"}, []Value{"x"}, []string{"2", "10", "b", "a"}, []Value{"two", "ten", "B", "x"}},
		// the labels of the update in another order than the record keeps them
		{[]string{"a", "2", "b"}, []Value{"x", "y", "z"}, []string{"2", "10", "b", "a"}, []Value{"y", "ten", "z", "x"}},
		// labels the record has not: added as an object of JavaScript adds keys
		{[]string{"zz", "a", "1"}, []Value{"x", "y", "z"}, []string{"1", "2", "10", "b", "a", "zz"}, []Value{"z", "two", "ten", "B", "y", "x"}},
	}
	for _, c := range cases {
		got := record(With(base, ShapeOf(c.labels...), c.vals...))
		if !reflect.DeepEqual(got.keys, c.keys) || !reflect.DeepEqual(got.vals, c.want) {
			t.Errorf("with %v: %v %v, want %v %v", c.labels, got.keys, got.vals, c.keys, c.want)
		}
	}
	if got := record(base); !reflect.DeepEqual(got.vals, []Value{"two", "ten", "B", "A"}) {
		t.Errorf("the record itself changed: %v", got.vals)
	}
	shape, x, y := ShapeOf("a", "2"), Value("x"), Value("y")
	if n := testing.AllocsPerRun(100, func() { With(base, shape, x, y) }); n != 1 {
		t.Errorf("%v allocations, want 1", n)
	}
}

// The record that lies in a value of a constructor: its header over the
// values beside it, filled from any record of the same labels.
func TestARecordOverValuesOfItsOwner(t *testing.T) {
	shape := ShapeOf("w", "h", "name")
	owner := struct {
		rec Record
		f   [3]Value
	}{f: [3]Value{int32(1), int32(2), "n"}}
	owner.rec.Over(shape, owner.f[:])
	if !reflect.DeepEqual(owner.rec.keys, []string{"w", "h", "name"}) || Field(&owner.rec, "h") != int32(2) {
		t.Errorf("%v %v", owner.rec.keys, owner.rec.vals)
	}
	// its values are the owner's: one set there is the record's
	owner.f[0] = int32(7)
	if Field(&owner.rec, "w") != int32(7) {
		t.Errorf("w is %v, want 7", Field(&owner.rec, "w"))
	}
	var into [3]Value
	shape.Values(Rec("name", "x", "w", int32(5), "h", int32(6)), into[:])
	if into != [3]Value{int32(5), int32(6), "x"} {
		t.Errorf("from a record in another order: %v", into)
	}
	shape.Values(&owner.rec, into[:])
	if into != [3]Value{int32(7), int32(2), "n"} {
		t.Errorf("from a record of the shape: %v", into)
	}
	// a constructor applied to nothing, as the runtime applies one to learn its tag
	var none [3]Value
	shape.Values(nil, none[:])
	if none != [3]Value{} {
		t.Errorf("from no record: %v", none)
	}
	if n := testing.AllocsPerRun(100, func() { shape.Values(&owner.rec, into[:]) }); n != 0 {
		t.Errorf("%v allocations, want 0", n)
	}
}
