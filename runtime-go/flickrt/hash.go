package flickrt

import (
	"math/bits"
	"slices"
)

// HashSet and HashMap: the elements or keys are kept in the order Effect's
// HAMT walks them on the JS target, by the witness's hash five bits at a
// time from the lowest, and by insertion among equal hashes.

// Witness is `EqHash w a`: the equality and hash of a collection.
type Witness struct{ eq, hash Value }

func (w *Witness) equal(a, b Value) bool { return Bool(apply2(w.eq, a, b)) }
func (w *Witness) of(a Value) int32      { return Apply(w.hash, a).(int32) }

// EqHashWith is `EqHash.with eq hash body`: `body` gets a fresh witness.
func EqHashWith(eq, hash, body Value) Value { return Apply(body, &Witness{eq, hash}) }

// EqHashCanonical is the witness of a `Hash` instance.
func EqHashCanonical(d Value) Value {
	return &Witness{Method(Super(d, "Eq"), "eq"), Method(d, "hash")}
}

// The tables of HashSet and HashMap are the hash array mapped trie of
// Effect's HashMap, which the JS target's collections are, node for node:
// a level takes five bits of the hash from the lowest, an indexed node keeps
// up to 16 children by a bitmap, a full array node takes over above that
// and gives way again below 8 children, and keys of one hash share a
// collision node in insertion order. Walking it child by child gives the
// order the JS target iterates in. Nodes are never mutated once built.

const (
	levelBits  = 5
	levelMask  = 1<<levelBits - 1
	maxIndexed = 16 // more children turn an indexed node into an array node
	minArray   = 8  // fewer turn an array node back into an indexed one
)

// entry is a leaf: a key, its value and its hash.
type entry struct {
	key, value Value
	hash       int32
}

// collision is the entries of keys whose hashes are all `hash`.
type collision struct {
	hash    int32
	entries []*entry
}

// indexed is the children present at a level, by the bits of `bitmap`.
type indexed struct {
	bitmap   uint32
	children []node
}

// array is a level with a slot for each of its 32 children.
type array struct {
	count    int
	children [1 << levelBits]node
}

// node is nil (empty), *entry, *collision, *indexed or *array.
type node = any

func chunk(h int32, shift uint) uint32 { return uint32(h) >> shift & levelMask }

// table is a set or map: its witness, trie and size; never mutated once
// shared.
type table struct {
	w    *Witness
	root node
	size int
}

func empty(w Value) *table { return &table{w: w.(*Witness)} }

// lookup is the entry of a key equal to `k`, of hash `h`, or nil.
func (t *table) lookup(k Value, h int32) *entry {
	n := t.root
	for shift := uint(0); ; shift += levelBits {
		switch x := n.(type) {
		case *entry:
			if x.hash == h && t.w.equal(x.key, k) {
				return x
			}
			return nil
		case *collision:
			if x.hash == h {
				for _, e := range x.entries {
					if t.w.equal(e.key, k) {
						return e
					}
				}
			}
			return nil
		case *indexed:
			bit := uint32(1) << chunk(h, shift)
			if x.bitmap&bit == 0 {
				return nil
			}
			n = x.children[bits.OnesCount32(x.bitmap&(bit-1))]
		case *array:
			n = x.children[chunk(h, shift)]
		default:
			return nil
		}
	}
}

func (t *table) get(k Value, h int32) (Value, bool) {
	if e := t.lookup(k, h); e != nil {
		return e.value, true
	}
	return nil, false
}

// add puts `e` into a table no one else holds yet: an entry of an equal
// key with an equal value, or any value with `keep`, stays; otherwise `e`
// replaces it in its place.
func (t *table) add(e *entry, keep bool) {
	added := false
	t.root = t.put(t.root, 0, e, keep, &added)
	if added {
		t.size++
	}
}

// set is the table with `k` and `v` put as `add` does.
func (t *table) set(k, v Value, h int32, keep bool) *table {
	out := *t
	out.add(&entry{k, v, h}, keep)
	if out.root == t.root {
		return t
	}
	return &out
}

func (t *table) remove(k Value) *table {
	root, removed := t.drop(t.root, 0, k, t.w.of(k))
	if !removed {
		return t
	}
	return &table{t.w, root, t.size - 1}
}

// all walks the entries in the JS target's order: `for e := range t.all`.
func (t *table) all(yield func(*entry) bool) { walkNode(t.root, yield) }

func walkNode(n node, yield func(*entry) bool) bool {
	switch x := n.(type) {
	case *entry:
		return yield(x)
	case *collision:
		for _, e := range x.entries {
			if !yield(e) {
				return false
			}
		}
	case *indexed:
		for _, c := range x.children {
			if !walkNode(c, yield) {
				return false
			}
		}
	case *array:
		for _, c := range x.children {
			if c != nil && !walkNode(c, yield) {
				return false
			}
		}
	}
	return true
}

// put is `n`, at the level `shift`, with `e`; as LeafNode.set and the
// other nodes' `set` of Effect's HashMap.
func (t *table) put(n node, shift uint, e *entry, keep bool, added *bool) node {
	switch x := n.(type) {
	case nil:
		*added = true
		return e
	case *entry:
		if x.hash != e.hash {
			*added = true
			return merge(shift, x.hash, x, e.hash, e)
		}
		if t.w.equal(x.key, e.key) {
			if keep || Equals(x.value, e.value) {
				return x
			}
			return e
		}
		*added = true
		return &collision{e.hash, []*entry{x, e}}
	case *collision:
		if x.hash != e.hash {
			*added = true
			return merge(shift, x.hash, x, e.hash, e)
		}
		for i, old := range x.entries {
			if t.w.equal(old.key, e.key) {
				if keep || Equals(old.value, e.value) {
					return x
				}
				entries := slices.Clone(x.entries)
				entries[i] = e
				return &collision{x.hash, entries}
			}
		}
		*added = true
		return &collision{x.hash, append(slices.Clip(x.entries), e)}
	case *indexed:
		bit := uint32(1) << chunk(e.hash, shift)
		i := bits.OnesCount32(x.bitmap & (bit - 1))
		if x.bitmap&bit != 0 {
			child := t.put(x.children[i], shift+levelBits, e, keep, added)
			if child == x.children[i] {
				return x
			}
			children := slices.Clone(x.children)
			children[i] = child
			return &indexed{x.bitmap, children}
		}
		*added = true
		if len(x.children) == maxIndexed {
			out := &array{count: maxIndexed + 1}
			j := 0
			for b := range out.children {
				if x.bitmap&(1<<b) != 0 {
					out.children[b] = x.children[j]
					j++
				}
			}
			out.children[chunk(e.hash, shift)] = e
			return out
		}
		children := make([]node, len(x.children)+1)
		copy(children, x.children[:i])
		children[i] = e
		copy(children[i+1:], x.children[i:])
		return &indexed{x.bitmap | bit, children}
	case *array:
		c := chunk(e.hash, shift)
		child := t.put(x.children[c], shift+levelBits, e, keep, added)
		if child == x.children[c] {
			return x
		}
		out := *x
		if x.children[c] == nil {
			out.count++
		}
		out.children[c] = child
		return &out
	}
	panic("flickrt: not a hash table node")
}

// merge is a level that holds two nodes of different hashes, with as many
// levels below as their hashes share chunks.
func merge(shift uint, h1 int32, n1 node, h2 int32, n2 node) node {
	c1, c2 := chunk(h1, shift), chunk(h2, shift)
	if c1 == c2 {
		return &indexed{1 << c1, []node{merge(shift+levelBits, h1, n1, h2, n2)}}
	}
	if c1 > c2 {
		n1, n2 = n2, n1
	}
	return &indexed{1<<c1 | 1<<c2, []node{n1, n2}}
}

// drop is `n`, at the level `shift`, without the key equal to `k`, nil
// when nothing is left, and whether there was one; as Effect's `remove`.
func (t *table) drop(n node, shift uint, k Value, h int32) (node, bool) {
	switch x := n.(type) {
	case *entry:
		if x.hash == h && t.w.equal(x.key, k) {
			return nil, true
		}
	case *collision:
		if x.hash != h {
			break
		}
		for i, e := range x.entries {
			if t.w.equal(e.key, k) {
				if len(x.entries) == 2 {
					return x.entries[1-i], true
				}
				return &collision{h, slices.Delete(slices.Clone(x.entries), i, i+1)}, true
			}
		}
	case *indexed:
		bit := uint32(1) << chunk(h, shift)
		if x.bitmap&bit == 0 {
			break
		}
		i := bits.OnesCount32(x.bitmap & (bit - 1))
		child, removed := t.drop(x.children[i], shift+levelBits, k, h)
		switch {
		case !removed:
			return x, false
		case child != nil:
			children := slices.Clone(x.children)
			children[i] = child
			return &indexed{x.bitmap, children}, true
		case x.bitmap == bit:
			return nil, true
		}
		// One leaf left takes its level's place, as in Effect.
		if len(x.children) == 2 {
			if leaf, ok := x.children[1-i].(*entry); ok {
				return leaf, true
			}
		}
		return &indexed{x.bitmap ^ bit, slices.Delete(slices.Clone(x.children), i, i+1)}, true
	case *array:
		c := chunk(h, shift)
		if x.children[c] == nil {
			break
		}
		child, removed := t.drop(x.children[c], shift+levelBits, k, h)
		if !removed {
			return x, false
		}
		out := *x
		if child == nil {
			out.count--
		}
		out.children[c] = child
		if out.count < minArray {
			return pack(&out), true
		}
		return &out, true
	}
	return n, false
}

// pack is an array node's children in an indexed node.
func pack(x *array) *indexed {
	out := &indexed{children: make([]node, 0, x.count)}
	for i, c := range x.children {
		if c != nil {
			out.bitmap |= 1 << i
			out.children = append(out.children, c)
		}
	}
	return out
}

// HashSet is `HashSet w a`.
type HashSet struct{ *table }

// HashMap is `HashMap w k v`.
type HashMap struct{ *table }

func setOf(t *table) Value { return HashSet{t} }
func mapOf(t *table) Value { return HashMap{t} }
func set(s Value) *table   { return s.(HashSet).table }
func hmap(m Value) *table  { return m.(HashMap).table }

func HashSetWith(w Value) Value    { return setOf(empty(w)) }
func HashSetEmpty(d Value) Value   { return HashSetWith(EqHashCanonical(d)) }
func HashSetSize(s Value) Value    { return int32(set(s).size) }
func HashSetIsEmpty(s Value) Value { return set(s).size == 0 }

// HashSetInsert adds an element; an equal one already there stays.
func HashSetInsert(x, s Value) Value {
	t := set(s)
	return setOf(t.set(x, Unit, t.w.of(x), true))
}

func HashSetFromList(w, xs Value) Value {
	t := empty(w)
	for _, x := range List(xs) {
		t.add(&entry{x, Unit, t.w.of(x)}, true)
	}
	return setOf(t)
}

func HashSetRemove(x, s Value) Value { return setOf(set(s).remove(x)) }

func HashSetMember(x, s Value) Value {
	t := set(s)
	return t.lookup(x, t.w.of(x)) != nil
}

func HashSetToList(s Value) Value {
	t := set(s)
	out := make([]Value, 0, t.size)
	for e := range t.all {
		out = append(out, e.key)
	}
	return out
}

// HashSetUnion is the elements of `a`, then those of `b` it lacks.
func HashSetUnion(a, b Value) Value {
	t := *set(a)
	for e := range set(b).all {
		t.add(e, true)
	}
	return setOf(&t)
}

func HashSetRehash(w, s Value) Value { return HashSetFromList(w, HashSetToList(s)) }

func filterSet(s Value, keep func(Value) bool) Value {
	t := set(s)
	out := &table{w: t.w}
	for e := range t.all {
		if keep(e.key) {
			out.add(e, true)
		}
	}
	return setOf(out)
}

func HashSetIntersection(that, s Value) Value {
	return filterSet(s, func(x Value) bool { return Bool(HashSetMember(x, that)) })
}

func HashSetDifference(that, s Value) Value {
	return filterSet(s, func(x Value) bool { return !Bool(HashSetMember(x, that)) })
}

func HashSetIsSubset(that, s Value) Value {
	for e := range set(s).all {
		if !Bool(HashSetMember(e.key, that)) {
			return false
		}
	}
	return true
}

func HashSetMap(w, f, s Value) Value {
	t := empty(w)
	for e := range set(s).all {
		y := Apply(f, e.key)
		t.add(&entry{y, Unit, t.w.of(y)}, true)
	}
	return setOf(t)
}

func HashSetFilter(f, s Value) Value {
	return filterSet(s, func(x Value) bool { return Bool(Apply(f, x)) })
}

func HashSetSome(f, s Value) Value {
	for e := range set(s).all {
		if Bool(Apply(f, e.key)) {
			return true
		}
	}
	return false
}

func HashSetEvery(f, s Value) Value {
	for e := range set(s).all {
		if !Bool(Apply(f, e.key)) {
			return false
		}
	}
	return true
}

func HashSetReduce(zero, f, s Value) Value {
	acc := zero
	for e := range set(s).all {
		acc = apply2(f, acc, e.key)
	}
	return acc
}

// Redacted is a value `show` does not reveal.
type Redacted struct{ value Value }

func RedactedMake(v Value) Value   { return &Redacted{v} }
func RedactedReveal(r Value) Value { return r.(*Redacted).value }
func RedactedEquivalent(eq, a, b Value) Value {
	return apply2(eq, a.(*Redacted).value, b.(*Redacted).value)
}

var continueTag = Tag("Loop.Step.Continue")

// Loop is `Loop.loop step state`: runs until `Done`, in constant stack.
func Loop(step, state Value) Value {
	for s := state; ; {
		next := Apply(step, s).(*Data)
		if next.Tag != continueTag {
			return next.Fields[0]
		}
		s = next.Fields[0]
	}
}

// ValidationMap2 is `map2` of a `Validation`: the errors of both, combined,
// or `f` of both values.
func ValidationMap2(combine, f, x, y Value) Value {
	a, b := x.(*Data), y.(*Data)
	switch {
	case a.Tag == "Failure" && b.Tag == "Failure":
		return Err(apply2(combine, a.Fields[0], b.Fields[0]))
	case a.Tag == "Failure":
		return a
	case b.Tag == "Failure":
		return b
	}
	return Ok(apply2(f, a.Fields[0], b.Fields[0]))
}
