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
