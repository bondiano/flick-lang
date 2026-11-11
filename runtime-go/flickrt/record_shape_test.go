package flickrt

import (
	"reflect"
	"testing"
)

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
