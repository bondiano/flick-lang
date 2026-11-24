package flickrt

import (
	"slices"
	"sort"
	"strconv"
)

// RecordView is the read-only record interface implemented by *Record and
// every compiler-generated closed record struct. Open-row code and the
// runtime read record fields through it without depending on concrete layout.
type RecordView interface {
	Get(k string) Value
	Lookup(k string) (Value, bool)
	Keys() []string
	Len() int
	At(i int) Value
}

// RecordEqual is an optional fast path for structural equality between two
// values of the same generated closed-record struct type, avoiding boxing
// scalar fields through RecordView methods.
type RecordEqual interface {
	RecordView
	EqualRecord(other Value, strictZero bool) (equal bool, sameType bool)
}

// Record is a record value: its labels in the order JS keeps an object's
// keys (integer-like ones first, ascending, then the others as added), so
// what lists them lists them as the JS target does. It is never mutated
// after it is built.
type Record struct {
	keys []string
	vals []Value
}

func (r *Record) Lookup(k string) (Value, bool) {
	if i := r.index(k); i >= 0 {
		return r.vals[i], true
	}
	return nil, false
}

func (r *Record) Keys() []string { return r.keys }
func (r *Record) Len() int       { return len(r.keys) }
func (r *Record) At(i int) Value { return r.vals[i] }

// AsRecord asserts that v implements RecordView.
func AsRecord(v Value) RecordView { return v.(RecordView) }

// materializeRecord copies a non-*Record RecordView into a fresh *Record.
func materializeRecord(rv RecordView) *Record {
	if r, ok := rv.(*Record); ok {
		return r
	}
	keys := rv.Keys()
	out := newRecord(len(keys))
	for i, k := range keys {
		out.keys = append(out.keys, k)
		out.vals = append(out.vals, rv.At(i))
	}
	return out
}

// copiedView returns a fresh *Record holding the same entries as v.
func copiedView(v Value) *Record {
	if r, ok := v.(*Record); ok {
		return r.copied()
	}
	return materializeRecord(v.(RecordView))
}

// asRecord returns v as a *Record if v implements RecordView.
func asRecord(v Value) (*Record, bool) {
	if r, ok := v.(*Record); ok {
		return r, true
	}
	if rv, ok := v.(RecordView); ok {
		return materializeRecord(rv), true
	}
	return nil, false
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

// copied is a record of the labels and values of `r` in an allocation of
// its own. Sharing the labels would be less to copy, but a small record
// holds its labels and values together, so the copy would keep every value
// of `r` alive for as long as it lives itself.
func (r *Record) copied() *Record {
	out := newRecord(len(r.keys))
	out.keys = append(out.keys, r.keys...)
	out.vals = append(out.vals, r.vals...)
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

// canonicalKeyLess orders structural record labels canonically: JS array-index
// keys first in ascending numeric order, then all other keys in UTF-16
// code-unit order (matching `Record.labels` and JS `Object.keys` on canonical
// records).
func canonicalKeyLess(a, b string) bool {
	na, oka := arrayIndex(a)
	nb, okb := arrayIndex(b)
	if oka && okb {
		return na < nb
	}
	if oka != okb {
		return oka
	}
	return CompareStrings(a, b) < 0
}

// putCanonical sets `k` to `v` in place in canonical key order: only for a
// structural record still being built.
func (r *Record) putCanonical(k string, v Value) {
	if i := r.index(k); i >= 0 {
		r.vals[i] = v
		return
	}
	at := sort.Search(len(r.keys), func(i int) bool {
		return !canonicalKeyLess(r.keys[i], k)
	})
	r.keys = slices.Insert(r.keys, at, k)
	r.vals = slices.Insert(r.vals, at, v)
}

// setCanonical is `r` with its fields plus `k` as `v` in canonical key order.
func (r *Record) setCanonical(k string, v Value) *Record {
	out := newRecord(len(r.keys) + 1)
	for i, key := range r.keys {
		out.putCanonical(key, r.vals[i])
	}
	out.putCanonical(k, v)
	return out
}

// shapeOf is the canonical key order of a structural record of `labels`, and
// where each label's value goes in it; nil when `labels` is already canonical.
func shapeOf(labels []string) ([]string, []int) {
	order := make([]int, len(labels))
	for i := range order {
		order[i] = i
	}
	sort.Slice(order, func(i, j int) bool {
		return canonicalKeyLess(labels[order[i]], labels[order[j]])
	})
	keys := make([]string, len(labels))
	pos := make([]int, len(labels))
	same := true
	for at, orig := range order {
		keys[at] = labels[orig]
		pos[orig] = at
		same = same && at == orig
	}
	if same {
		return keys, nil
	}
	return keys, pos
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

func record(v Value) *Record {
	if r, ok := v.(*Record); ok {
		return r
	}
	return materializeRecord(v.(RecordView))
}

// Get is the value of a label.
func (r *Record) Get(k string) Value {
	if i := r.index(k); i >= 0 {
		return r.vals[i]
	}
	panic(Defect{"record has no `" + k + "`"})
}

// Update is a copy of a record with some fields replaced or added.
func Update(base Value, fields Value) Value {
	b := copiedView(base)
	f := AsRecord(fields)
	// fields the record has replace their values in a copy of it, with no
	// room for more labels and no search for where a new one goes
	replaced := b
	for i, k := range f.Keys() {
		at := b.index(k)
		if at < 0 {
			replaced = nil
			break
		}
		replaced.vals[at] = f.At(i)
	}
	if replaced != nil {
		return replaced
	}
	src := AsRecord(base)
	out := newRecord(src.Len() + f.Len())
	for i, k := range src.Keys() {
		out.keys = append(out.keys, k)
		out.vals = append(out.vals, src.At(i))
	}
	for i, k := range f.Keys() {
		out.put(k, f.At(i))
	}
	return out
}

// With is `Update` of a record by the record of a shape, with that record
// never made: `vals` are the values of the shape's labels as `Shaped` takes
// them.
func With(base Value, s *Shape, vals ...Value) Value {
	out := copiedView(base)
	for i, v := range vals {
		k := i
		if s.pos != nil {
			k = s.pos[i]
		}
		at := out.index(s.keys[k])
		if at < 0 {
			// a label the record has not: where it goes is the update's to find
			return Update(base, Shaped(s, vals...))
		}
		out.vals[at] = v
	}
	return out
}

// sortedKeys is the labels of a record in UTF-16 order.
func sortedKeys(r *Record) []string {
	return sortedViewKeys(r)
}

// sortedViewKeys is the labels of a RecordView in UTF-16 order.
func sortedViewKeys(r RecordView) []string {
	keys := append([]string{}, r.Keys()...)
	sort.Slice(keys, func(i, j int) bool { return CompareStrings(keys[i], keys[j]) < 0 })
	return keys
}

// RecordToEntries is the label and value of each field, in key order.
func RecordToEntries(r Value) Value {
	x := AsRecord(r)
	keys := x.Keys()
	out := make([]Value, len(keys))
	for i, k := range keys {
		out[i] = []Value{k, x.At(i)}
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

// Shape is the labels of a record literal in the order a record keeps
// them, found once for every record the literal makes.
type Shape struct {
	keys []string
	pos  []int
}

// ShapeOf is the canonical shape of a structural record of `labels`. Its
// labels are shared by those records, so they have no room where a record
// that grows could write.
func ShapeOf(labels ...string) *Shape {
	keys, pos := shapeOf(labels)
	return &Shape{slices.Clip(keys), pos}
}

// Shaped is the record of a shape from the values of its labels, in the
// order of the literal: the values alone are made, in one allocation for
// up to eight.
func Shaped(s *Shape, vals ...Value) Value {
	n := len(vals)
	var out *Record
	switch {
	case n <= 2:
		b := &struct {
			r Record
			v [2]Value
		}{}
		b.r = Record{s.keys, b.v[:n]}
		out = &b.r
	case n <= 4:
		b := &struct {
			r Record
			v [4]Value
		}{}
		b.r = Record{s.keys, b.v[:n]}
		out = &b.r
	case n <= 8:
		b := &struct {
			r Record
			v [8]Value
		}{}
		b.r = Record{s.keys, b.v[:n]}
		out = &b.r
	default:
		out = &Record{s.keys, make([]Value, n)}
	}
	if s.pos == nil {
		copy(out.vals, vals)
		return out
	}
	for i, v := range vals {
		out.vals[s.pos[i]] = v
	}
	return out
}

// Over makes `r` the record of the shape `s` over `vals`: the values of its
// labels in the order the record keeps them, which the record takes over.
// Generated code makes so the record that lies in a value of a constructor.
func (r *Record) Over(s *Shape, vals []Value) { r.keys, r.vals = s.keys, vals }

// FieldAt is the value at place `i` of a record whose labels are known to
// be in the order of a shape: the record that lies in a value of a
// constructor, which generated code reads by place.
func FieldAt(r Value, i int) Value { return r.(RecordView).At(i) }

// Values fills `into` with the values the labels of `s` have in the record
// `from`, in the order a record of the shape keeps them.
func (s *Shape) Values(from Value, into []Value) {
	if from == nil {
		// no record: the runtime applies a constructor to nothing to learn
		// its tag (`tagOf`)
		return
	}
	if r, ok := from.(*Record); ok {
		if len(r.keys) == len(s.keys) && (len(s.keys) == 0 || &r.keys[0] == &s.keys[0]) {
			// a record of this very shape
			copy(into, r.vals)
			return
		}
		for i, k := range s.keys {
			into[i] = r.Get(k)
		}
		return
	}
	rv := AsRecord(from)
	for i, k := range s.keys {
		into[i] = rv.Get(k)
	}
}
