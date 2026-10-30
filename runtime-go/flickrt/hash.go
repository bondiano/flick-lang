package flickrt

import "sort"

// HashSet and HashMap: the elements or keys are kept in the order Effect's
// HAMT walks them on the JS target, by the witness's hash five bits at a
// time from the lowest, and by insertion among equal hashes.
//
// shortcut: a sorted copy-on-write slice, O(n) per insert; a HAMT once the go target is measured

// Witness is `EqHash w a`: the equality and hash of a collection.
type Witness struct{ eq, hash Value }

func (w *Witness) equal(fb *Fiber, a, b Value) bool { return Bool(apply2(fb, w.eq, a, b)) }
func (w *Witness) of(fb *Fiber, a Value) int32      { return Apply(fb, w.hash, a).(int32) }

// EqHashWith is `EqHash.with eq hash body`: `body` gets a fresh witness.
func EqHashWith(fb *Fiber, eq, hash, body Value) Value { return Apply(fb, body, &Witness{eq, hash}) }

// EqHashCanonical is the witness of a `Hash` instance.
func EqHashCanonical(fb *Fiber, d Value) Value {
	return &Witness{Method(fb, Super(fb, d, "Eq"), "eq"), Method(fb, d, "hash")}
}

type entry struct {
	key, value Value
	hash       int32
	order      uint64
}

// walk is where a hash puts an entry in the HAMT's walk: its five-bit
// chunks from the lowest, most significant first.
func walk(h int32) uint64 {
	u, key := uint32(h), uint64(0)
	for shift := 0; shift < 35; shift += 5 {
		key = key<<5 | uint64(u>>shift&31)
	}
	return key
}

// table is the entries of a set or map in walk order; never mutated.
type table struct {
	w       *Witness
	entries []entry
	next    uint64
}

// find is the index of the key equal to `k` among those of hash `h`, or
// where an entry of that hash goes, and whether it was found.
func (t *table) find(fb *Fiber, k Value, h int32) (int, bool) {
	at := walk(h)
	i := sort.Search(len(t.entries), func(i int) bool { return walk(t.entries[i].hash) >= at })
	for j := i; j < len(t.entries) && t.entries[j].hash == h; j++ {
		if t.w.equal(fb, k, t.entries[j].key) {
			return j, true
		}
	}
	for i < len(t.entries) && t.entries[i].hash == h {
		i++
	}
	return i, false
}

// set puts `k` and `v`: a new key replaces an equal one in its place, and
// with `keep` the old entry stays.
func (t *table) set(fb *Fiber, k, v Value, h int32, keep bool) *table {
	i, found := t.find(fb, k, h)
	if found && (keep || Equals(t.entries[i].value, v)) {
		return t
	}
	out := &table{w: t.w, next: t.next}
	if found {
		out.entries = append([]entry{}, t.entries...)
		out.entries[i] = entry{k, v, h, t.entries[i].order}
		return out
	}
	out.entries = make([]entry, 0, len(t.entries)+1)
	out.entries = append(append(append(out.entries, t.entries[:i]...), entry{k, v, h, t.next}), t.entries[i:]...)
	out.next++
	return out
}

func (t *table) remove(fb *Fiber, k Value) *table {
	i, found := t.find(fb, k, t.w.of(fb, k))
	if !found {
		return t
	}
	out := &table{w: t.w, next: t.next, entries: make([]entry, 0, len(t.entries)-1)}
	out.entries = append(append(out.entries, t.entries[:i]...), t.entries[i+1:]...)
	return out
}

func (t *table) get(fb *Fiber, k Value, h int32) (Value, bool) {
	if i, found := t.find(fb, k, h); found {
		return t.entries[i].value, true
	}
	return nil, false
}

func empty(w Value) *table { return &table{w: w.(*Witness)} }

// HashSet is `HashSet w a`.
type HashSet struct{ *table }

// HashMap is `HashMap w k v`.
type HashMap struct{ *table }

func setOf(t *table) Value { return HashSet{t} }
func mapOf(t *table) Value { return HashMap{t} }
func set(s Value) *table   { return s.(HashSet).table }
func hmap(m Value) *table  { return m.(HashMap).table }

func HashSetWith(w Value) Value             { return setOf(empty(w)) }
func HashSetEmpty(fb *Fiber, d Value) Value { return HashSetWith(EqHashCanonical(fb, d)) }
func HashSetSize(s Value) Value             { return int32(len(set(s).entries)) }
func HashSetIsEmpty(s Value) Value          { return len(set(s).entries) == 0 }

// HashSetInsert adds an element; an equal one already there stays.
func HashSetInsert(fb *Fiber, x, s Value) Value {
	t := set(s)
	return setOf(t.set(fb, x, Unit, t.w.of(fb, x), true))
}

func HashSetFromList(fb *Fiber, w, xs Value) Value {
	t := empty(w)
	for _, x := range List(xs) {
		t = t.set(fb, x, Unit, t.w.of(fb, x), true)
	}
	return setOf(t)
}

func HashSetRemove(fb *Fiber, x, s Value) Value { return setOf(set(s).remove(fb, x)) }

func HashSetMember(fb *Fiber, x, s Value) Value {
	t := set(s)
	_, found := t.find(fb, x, t.w.of(fb, x))
	return found
}

func HashSetToList(s Value) Value {
	t := set(s)
	out := make([]Value, len(t.entries))
	for i, e := range t.entries {
		out[i] = e.key
	}
	return out
}

// HashSetUnion is the elements of `a`, then those of `b` it lacks.
func HashSetUnion(fb *Fiber, a, b Value) Value {
	t := set(a)
	for _, e := range set(b).entries {
		t = t.set(fb, e.key, Unit, t.w.of(fb, e.key), true)
	}
	return setOf(t)
}

func HashSetRehash(fb *Fiber, w, s Value) Value { return HashSetFromList(fb, w, HashSetToList(s)) }

func filterSet(s Value, keep func(Value) bool) Value {
	t := set(s)
	out := &table{w: t.w, next: t.next}
	for _, e := range t.entries {
		if keep(e.key) {
			out.entries = append(out.entries, e)
		}
	}
	return setOf(out)
}

func HashSetIntersection(fb *Fiber, that, s Value) Value {
	return filterSet(s, func(x Value) bool { return Bool(HashSetMember(fb, x, that)) })
}

func HashSetDifference(fb *Fiber, that, s Value) Value {
	return filterSet(s, func(x Value) bool { return !Bool(HashSetMember(fb, x, that)) })
}

func HashSetIsSubset(fb *Fiber, that, s Value) Value {
	for _, e := range set(s).entries {
		if !Bool(HashSetMember(fb, e.key, that)) {
			return false
		}
	}
	return true
}

func HashSetMap(fb *Fiber, w, f, s Value) Value {
	t := empty(w)
	for _, e := range set(s).entries {
		y := Apply(fb, f, e.key)
		t = t.set(fb, y, Unit, t.w.of(fb, y), true)
	}
	return setOf(t)
}

func HashSetFilter(fb *Fiber, f, s Value) Value {
	return filterSet(s, func(x Value) bool { return Bool(Apply(fb, f, x)) })
}

func HashSetSome(fb *Fiber, f, s Value) Value {
	for _, e := range set(s).entries {
		if Bool(Apply(fb, f, e.key)) {
			return true
		}
	}
	return false
}

func HashSetEvery(fb *Fiber, f, s Value) Value {
	for _, e := range set(s).entries {
		if !Bool(Apply(fb, f, e.key)) {
			return false
		}
	}
	return true
}

func HashSetReduce(fb *Fiber, zero, f, s Value) Value {
	acc := zero
	for _, e := range set(s).entries {
		acc = apply2(fb, f, acc, e.key)
	}
	return acc
}

// Redacted is a value `show` does not reveal.
type Redacted struct{ value Value }

func RedactedMake(v Value) Value   { return &Redacted{v} }
func RedactedReveal(r Value) Value { return r.(*Redacted).value }
func RedactedEquivalent(fb *Fiber, eq, a, b Value) Value {
	return apply2(fb, eq, a.(*Redacted).value, b.(*Redacted).value)
}

var continueTag = Tag("Loop.Step.Continue")

// Loop is `Loop.loop step state`: runs until `Done`, in constant stack.
func Loop(fb *Fiber, step, state Value) Value {
	for s := state; ; {
		next := Apply(fb, step, s).(*Data)
		if next.Tag != continueTag {
			return next.Fields[0]
		}
		s = next.Fields[0]
	}
}

// ValidationMap2 is `map2` of a `Validation`: the errors of both, combined,
// or `f` of both values.
func ValidationMap2(fb *Fiber, combine, f, x, y Value) Value {
	a, b := x.(*Data), y.(*Data)
	switch {
	case a.Tag == "Failure" && b.Tag == "Failure":
		return Err(apply2(fb, combine, a.Fields[0], b.Fields[0]))
	case a.Tag == "Failure":
		return a
	case b.Tag == "Failure":
		return b
	}
	return Ok(apply2(fb, f, a.Fields[0], b.Fields[0]))
}
