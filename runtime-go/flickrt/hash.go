package flickrt

// HashSet and HashMap: the elements or keys are kept in the order the JS
// target walks them, by the witness's hash five bits at a time from the
// lowest, and by insertion among equal hashes; a HAMT of their own keeps
// them so (hamt.go).

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

// table is the entries of a set or map; never mutated.
type table struct {
	w    *Witness
	root *hnode
	size int32
}

// set puts `k` and `v`: a new key replaces an equal one in its place, and
// with `keep` the old entry stays.
func (t *table) set(fb *Fiber, k, v Value, h int32, keep bool) *table {
	root, added := t.root.with(fb, t.w, entry{k, v, h}, 0, keep)
	if root == t.root {
		return t
	}
	size := t.size
	if added {
		size++
	}
	return &table{t.w, root, size}
}

func (t *table) remove(fb *Fiber, k Value) *table {
	root, removed := t.root.without(fb, t.w, k, t.w.of(fb, k), 0)
	if !removed {
		return t
	}
	return &table{t.w, root, t.size - 1}
}

func (t *table) get(fb *Fiber, k Value, h int32) (Value, bool) {
	e, found := t.root.find(fb, t.w, k, h, 0)
	return e.value, found
}

func (t *table) has(fb *Fiber, k Value) bool {
	_, found := t.root.find(fb, t.w, k, t.w.of(fb, k), 0)
	return found
}

// all is the entries in walk order.
func (t *table) all() []entry {
	out := make([]entry, 0, t.size)
	t.root.each(func(e entry) bool { out = append(out, e); return true })
	return out
}

// kept is the table of the entries `keep` keeps, each as it gives it, in
// the same order.
func (t *table) kept(keep func(entry) (entry, bool)) *table {
	out := empty(t.w)
	t.root.each(func(e entry) bool {
		if e, ok := keep(e); ok {
			out.root, _ = out.root.with(nil, nil, e, 0, false)
			out.size++
		}
		return true
	})
	return out
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
func HashSetSize(s Value) Value             { return set(s).size }
func HashSetIsEmpty(s Value) Value          { return set(s).size == 0 }

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
	return set(s).has(fb, x)
}

func HashSetToList(s Value) Value {
	out := make([]Value, 0, set(s).size)
	set(s).root.each(func(e entry) bool { out = append(out, e.key); return true })
	return out
}

// HashSetUnion is the elements of `a`, then those of `b` it lacks.
func HashSetUnion(fb *Fiber, a, b Value) Value {
	t := set(a)
	for _, e := range set(b).all() {
		t = t.set(fb, e.key, Unit, t.w.of(fb, e.key), true)
	}
	return setOf(t)
}

func HashSetRehash(fb *Fiber, w, s Value) Value { return HashSetFromList(fb, w, HashSetToList(s)) }

func filterSet(s Value, keep func(Value) bool) Value {
	return setOf(set(s).kept(func(e entry) (entry, bool) { return e, keep(e.key) }))
}

func HashSetIntersection(fb *Fiber, that, s Value) Value {
	return filterSet(s, func(x Value) bool { return Bool(HashSetMember(fb, x, that)) })
}

func HashSetDifference(fb *Fiber, that, s Value) Value {
	return filterSet(s, func(x Value) bool { return !Bool(HashSetMember(fb, x, that)) })
}

func HashSetIsSubset(fb *Fiber, that, s Value) Value {
	for _, e := range set(s).all() {
		if !Bool(HashSetMember(fb, e.key, that)) {
			return false
		}
	}
	return true
}

func HashSetMap(fb *Fiber, w, f, s Value) Value {
	t := empty(w)
	for _, e := range set(s).all() {
		y := Apply(fb, f, e.key)
		t = t.set(fb, y, Unit, t.w.of(fb, y), true)
	}
	return setOf(t)
}

func HashSetFilter(fb *Fiber, f, s Value) Value {
	return filterSet(s, func(x Value) bool { return Bool(Apply(fb, f, x)) })
}

func HashSetSome(fb *Fiber, f, s Value) Value {
	for _, e := range set(s).all() {
		if Bool(Apply(fb, f, e.key)) {
			return true
		}
	}
	return false
}

func HashSetEvery(fb *Fiber, f, s Value) Value {
	for _, e := range set(s).all() {
		if !Bool(Apply(fb, f, e.key)) {
			return false
		}
	}
	return true
}

func HashSetReduce(fb *Fiber, zero, f, s Value) Value {
	acc := zero
	for _, e := range set(s).all() {
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

// A Trampoline: its value when done, else the thunk of its next step.
type trampoline struct {
	done  bool
	value Value
}

func TrampolineDone(v Value) Value        { return &trampoline{true, v} }
func TrampolineSuspend(thunk Value) Value { return &trampoline{false, thunk} }

// TrampolineRun runs the steps until one is done, in constant stack.
func TrampolineRun(fb *Fiber, t Value) Value {
	cur := t.(*trampoline)
	for !cur.done {
		cur = Apply(fb, cur.value, Unit).(*trampoline)
	}
	return cur.value
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
