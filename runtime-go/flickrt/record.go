package flickrt

import (
	"slices"
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
	if k == "" || k[0] < '0' || k[0] > '9' {
		return 0, false
	}
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
		out := r.copied()
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

// copied is a record of the values of `r` that shares its labels; a small
// one is a single allocation.
func (r *Record) copied() *Record {
	n := len(r.vals)
	var out *Record
	switch {
	case n <= 2:
		b := &struct {
			r Record
			v [2]Value
		}{}
		b.r = Record{r.keys, b.v[:n]}
		out = &b.r
	case n <= 4:
		b := &struct {
			r Record
			v [4]Value
		}{}
		b.r = Record{r.keys, b.v[:n]}
		out = &b.r
	case n <= 8:
		b := &struct {
			r Record
			v [8]Value
		}{}
		b.r = Record{r.keys, b.v[:n]}
		out = &b.r
	default:
		out = &Record{r.keys, make([]Value, n)}
	}
	copy(out.vals, r.vals)
	return out
}

// put sets `k` to `v` in place: only for a record still being built.
func (r *Record) put(k string, v Value) {
	if i := r.index(k); i >= 0 {
		r.vals[i] = v
		return
	}
	at := len(r.keys)
	if n, ok := arrayIndex(k); ok {
		at = sort.Search(len(r.keys), func(i int) bool {
			m, isIndex := arrayIndex(r.keys[i])
			return !isIndex || m > n
		})
	}
	r.keys = slices.Insert(r.keys, at, k)
	r.vals = slices.Insert(r.vals, at, v)
}

// newRecord is an empty record with room for `n` fields; a small one is a
// single allocation, as records are built on every request.
func newRecord(n int) *Record {
	switch {
	case n <= 2:
		b := &struct {
			r Record
			k [2]string
			v [2]Value
		}{}
		b.r = Record{b.k[:0], b.v[:0]}
		return &b.r
	case n <= 4:
		b := &struct {
			r Record
			k [4]string
			v [4]Value
		}{}
		b.r = Record{b.k[:0], b.v[:0]}
		return &b.r
	case n <= 8:
		b := &struct {
			r Record
			k [8]string
			v [8]Value
		}{}
		b.r = Record{b.k[:0], b.v[:0]}
		return &b.r
	}
	return &Record{make([]string, 0, n), make([]Value, 0, n)}
}

// shapeOf is the key order of a record of `labels`, and where each label's
// value goes in it; nil when that is the labels' own order.
func shapeOf(labels []string) ([]string, []int) {
	r := newRecord(len(labels))
	for i, l := range labels {
		r.put(l, i)
	}
	pos := make([]int, len(labels))
	same := true
	for at, i := range r.vals {
		pos[i.(int)] = at
		same = same && at == i.(int)
	}
	if same {
		return r.keys, nil
	}
	return r.keys, pos
}

// shaped is the record of `keys` from shapeOf with the labels' values;
// it takes `vals` over.
func shaped(keys []string, pos []int, vals []Value) *Record {
	if pos == nil {
		return &Record{keys, vals}
	}
	out := make([]Value, len(vals))
	for i, v := range vals {
		out[pos[i]] = v
	}
	return &Record{keys, out}
}

// Rec is a record of labels and values, alternating.
func Rec(fields ...Value) Value {
	r := newRecord(len(fields) / 2)
	for i := 0; i < len(fields); i += 2 {
		r.put(fields[i].(string), fields[i+1])
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
	b, f := record(base), record(fields)
	// fields the record has keep its labels: only its values are copied
	replaced := b.copied()
	for i, k := range f.keys {
		at := b.index(k)
		if at < 0 {
			replaced = nil
			break
		}
		replaced.vals[at] = f.vals[i]
	}
	if replaced != nil {
		return replaced
	}
	out := newRecord(len(b.keys) + len(f.keys))
	out.keys = append(out.keys, b.keys...)
	out.vals = append(out.vals, b.vals...)
	for i, k := range f.keys {
		out.put(k, f.vals[i])
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
		r.put(pair[0].(string), pair[1])
	}
	return r
}
