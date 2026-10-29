package flickrt

import (
	"sort"
	"strconv"
)

// Record is a record value: its labels in the order JS keeps an object's
// keys (integer-like ones first, ascending, then the others as added), so
// what lists them lists them as the JS target does. It is never mutated
// after it is built.
type Record struct {
	keys []string
	vals []Value
}

// arrayIndex is whether `k` is a key JS orders as an array index.
func arrayIndex(k string) (uint32, bool) {
	n, err := strconv.ParseUint(k, 10, 32)
	if err != nil || n == 1<<32-1 || strconv.FormatUint(n, 10) != k {
		return 0, false
	}
	return uint32(n), true
}

func (r *Record) index(k string) int {
	for i, key := range r.keys {
		if key == k {
			return i
		}
	}
	return -1
}

// set is `r` with `k` as `v`: a key it has keeps its place.
func (r *Record) set(k string, v Value) *Record {
	if i := r.index(k); i >= 0 {
		out := &Record{r.keys, append([]Value{}, r.vals...)}
		out.vals[i] = v
		return out
	}
	at := len(r.keys)
	if n, ok := arrayIndex(k); ok {
		at = sort.Search(len(r.keys), func(i int) bool {
			m, isIndex := arrayIndex(r.keys[i])
			return !isIndex || m > n
		})
	}
	out := &Record{make([]string, 0, len(r.keys)+1), make([]Value, 0, len(r.vals)+1)}
	out.keys = append(append(append(out.keys, r.keys[:at]...), k), r.keys[at:]...)
	out.vals = append(append(append(out.vals, r.vals[:at]...), v), r.vals[at:]...)
	return out
}

// Rec is a record of labels and values, alternating.
func Rec(fields ...Value) Value {
	r := &Record{}
	for i := 0; i < len(fields); i += 2 {
		r = r.set(fields[i].(string), fields[i+1])
	}
	return r
}

func record(v Value) *Record { return v.(*Record) }

// Get is the value of a label.
func (r *Record) Get(k string) Value {
	if i := r.index(k); i >= 0 {
		return r.vals[i]
	}
	panic(Defect{"record has no `" + k + "`"})
}

// Update is a copy of a record with some fields replaced or added.
func Update(base Value, fields Value) Value {
	out := record(base)
	f := record(fields)
	for i, k := range f.keys {
		out = out.set(k, f.vals[i])
	}
	return out
}

// sortedKeys is the labels of a record in UTF-16 order.
func sortedKeys(r *Record) []string {
	keys := append([]string{}, r.keys...)
	sort.Slice(keys, func(i, j int) bool { return CompareStrings(keys[i], keys[j]) < 0 })
	return keys
}

// RecordToEntries is the label and value of each field, in key order.
func RecordToEntries(r Value) Value {
	x := record(r)
	out := make([]Value, len(x.keys))
	for i, k := range x.keys {
		out[i] = []Value{k, x.vals[i]}
	}
	return out
}

// RecordFromEntries is the record of label and value pairs; a later pair
// replaces an earlier one with its label.
func RecordFromEntries(entries Value) Value {
	r := &Record{}
	for _, e := range List(entries) {
		pair := e.([]Value)
		r = r.set(pair[0].(string), pair[1])
	}
	return r
}
