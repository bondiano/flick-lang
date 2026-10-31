package flickrt

import "slices"

// Effect's `Trie`: a ternary search tree over the UTF-16 code units of the
// keys, persistent by path copying. Effect keeps the count on every node
// but reads only the root's, so here the trie holds it.

type trieNode struct {
	key              uint16
	value            *Value
	left, mid, right *trieNode
}

type Trie struct {
	root  *trieNode
	count int32
}

func trieOf(v Value) *Trie { return v.(*Trie) }

var TrieEmpty Value = &Trie{}

// trieStep is a node on the path to a key and the side taken from it:
// -1 left, 0 mid, 1 right.
type trieStep struct {
	node *trieNode
	side int
}

// rebuild copies the path above its last node, the last replaced by `last`
// (pruned when `prune` and it is empty), and returns the new root.
func rebuild(path []trieStep, last *trieNode, prune bool) *trieNode {
	child := last
	for s := len(path) - 2; s >= 0; s-- {
		n := *path[s].node
		if prune && child != nil && child.value == nil && child.left == nil && child.mid == nil && child.right == nil {
			child = nil
		}
		switch path[s].side {
		case -1:
			n.left = child
		case 1:
			n.right = child
		default:
			n.mid = child
		}
		child = &n
	}
	return child
}

func trieInsert(t *Trie, key []uint16, value Value) *Trie {
	if len(key) == 0 {
		return t
	}
	n := t.root
	if n == nil {
		n = &trieNode{key: key[0]}
	}
	count := t.count + 1
	path := []trieStep{}
	var last *trieNode
	for i := 0; i < len(key); {
		c := key[i]
		switch {
		case c > n.key:
			path = append(path, trieStep{n, 1})
			if n.right == nil {
				n = &trieNode{key: c}
			} else {
				n = n.right
			}
		case c < n.key:
			path = append(path, trieStep{n, -1})
			if n.left == nil {
				n = &trieNode{key: c}
			} else {
				n = n.left
			}
		default:
			if i == len(key)-1 {
				if n.value != nil {
					count--
				}
				copied := *n
				copied.value = &value
				path = append(path, trieStep{n, 0})
				last = &copied
			} else {
				path = append(path, trieStep{n, 0})
				if n.mid == nil {
					n = &trieNode{key: key[i+1]}
				} else {
					n = n.mid
				}
			}
			i++
		}
	}
	return &Trie{rebuild(path, last, false), count}
}

// triePath is the path to the node holding `key`, nil when it has none.
func triePath(t *Trie, key []uint16) []trieStep {
	n := t.root
	if n == nil || len(key) == 0 {
		return nil
	}
	path := []trieStep{}
	for i := 0; i < len(key); {
		c := key[i]
		switch {
		case c > n.key:
			if n.right == nil {
				return nil
			}
			path = append(path, trieStep{n, 1})
			n = n.right
		case c < n.key:
			if n.left == nil {
				return nil
			}
			path = append(path, trieStep{n, -1})
			n = n.left
		default:
			if i == len(key)-1 && n.value == nil || i < len(key)-1 && n.mid == nil {
				return nil
			}
			path = append(path, trieStep{n, 0})
			if i < len(key)-1 {
				n = n.mid
			}
			i++
		}
	}
	return path
}

func trieRemove(t *Trie, key []uint16) *Trie {
	path := triePath(t, key)
	if path == nil {
		return t
	}
	last := *path[len(path)-1].node
	last.value = nil
	return &Trie{rebuild(path, &last, true), t.count - 1}
}

func trieUnits(k Value) []uint16 { return utf16Units(k.(string)) }

func TrieFromList(xs Value) Value { return TrieInsertMany(xs, TrieEmpty) }

func TrieInsert(key, value, t Value) Value { return trieInsert(trieOf(t), trieUnits(key), value) }

func TrieInsertMany(xs, t Value) Value {
	trie := trieOf(t)
	for _, kv := range List(xs) {
		trie = trieInsert(trie, trieUnits(At(kv, 0)), At(kv, 1))
	}
	return trie
}

func TrieRemove(key, t Value) Value { return trieRemove(trieOf(t), trieUnits(key)) }

func TrieRemoveMany(keys, t Value) Value {
	trie := trieOf(t)
	for _, k := range List(keys) {
		trie = trieRemove(trie, trieUnits(k))
	}
	return trie
}

func TrieGet(key, t Value) Value {
	if path := triePath(trieOf(t), trieUnits(key)); path != nil {
		return Some(*path[len(path)-1].node.value)
	}
	return None
}

func TrieHas(key, t Value) Value { return triePath(trieOf(t), trieUnits(key)) != nil }
func TrieSize(t Value) Value     { return trieOf(t).count }
func TrieIsEmpty(t Value) Value  { return trieOf(t).count == 0 }

func TrieModify(fb *Fiber, key, f, t Value) Value {
	trie := trieOf(t)
	path := triePath(trie, trieUnits(key))
	if path == nil {
		return trie
	}
	last := *path[len(path)-1].node
	v := Apply(fb, f, *last.value)
	last.value = &v
	return &Trie{rebuild(path, &last, false), trie.count}
}

// trieEach visits the entries in Effect's order (left, the node, mid,
// right: the keys' UTF-16 order) whose keys start with `prefix`.
func trieEach(t *Trie, prefix []uint16, f func(key string, v Value)) {
	var visit func(n *trieNode, keyString []uint16)
	visit = func(n *trieNode, keyString []uint16) {
		if n.left != nil {
			visit(n.left, keyString)
		}
		key := append(keyString[:len(keyString):len(keyString)], n.key)
		if n.value != nil && len(key) >= len(prefix) && slices.Equal(key[:len(prefix)], prefix) {
			f(fromUnits(key), *n.value)
		}
		if n.mid != nil {
			visit(n.mid, key)
		}
		if n.right != nil {
			visit(n.right, keyString)
		}
	}
	if t.root != nil {
		visit(t.root, nil)
	}
}

func trieList(t Value, prefix Value, f func(key string, v Value) Value) Value {
	var p []uint16
	if prefix != nil {
		p = trieUnits(prefix)
	}
	out := []Value{}
	trieEach(trieOf(t), p, func(k string, v Value) { out = append(out, f(k, v)) })
	return out
}

func trieKey(k string, _ Value) Value   { return k }
func trieValue(_ string, v Value) Value { return v }
func trieEntry(k string, v Value) Value { return []Value{k, v} }

func TrieKeys(t Value) Value                      { return trieList(t, nil, trieKey) }
func TrieValues(t Value) Value                    { return trieList(t, nil, trieValue) }
func TrieEntries(t Value) Value                   { return trieList(t, nil, trieEntry) }
func TrieKeysWithPrefix(prefix, t Value) Value    { return trieList(t, prefix, trieKey) }
func TrieValuesWithPrefix(prefix, t Value) Value  { return trieList(t, prefix, trieValue) }
func TrieEntriesWithPrefix(prefix, t Value) Value { return trieList(t, prefix, trieEntry) }

func TrieLongestPrefixOf(key, t Value) Value {
	n := trieOf(t).root
	units := trieUnits(key)
	if n == nil || len(units) == 0 {
		return None
	}
	longest := None
	for i := 0; i < len(units); {
		c := units[i]
		var next *trieNode
		switch {
		case c > n.key:
			next = n.right
		case c < n.key:
			next = n.left
		default:
			if n.value != nil {
				longest = Some([]Value{fromUnits(units[:i+1]), *n.value})
			}
			next = n.mid
			i++
		}
		if next == nil {
			break
		}
		n = next
	}
	return longest
}

func TrieReduce(fb *Fiber, zero, f, t Value) Value {
	acc := zero
	trieEach(trieOf(t), nil, func(k string, v Value) { acc = Apply(fb, apply2(fb, f, acc, v), k) })
	return acc
}

// trieRebuilt is the trie of the entries `f` keeps, inserted in order as
// Effect's `map` and filters do.
func trieRebuilt(t Value, f func(k string, v Value) (Value, bool)) Value {
	out := &Trie{}
	trieEach(trieOf(t), nil, func(k string, v Value) {
		if w, keep := f(k, v); keep {
			out = trieInsert(out, utf16Units(k), w)
		}
	})
	return out
}

func TrieMap(fb *Fiber, f, t Value) Value {
	return trieRebuilt(t, func(k string, v Value) (Value, bool) { return apply2(fb, f, v, k), true })
}

func TrieFilter(fb *Fiber, f, t Value) Value {
	return trieRebuilt(t, func(k string, v Value) (Value, bool) { return v, Bool(apply2(fb, f, v, k)) })
}

func TrieFilterMap(fb *Fiber, f, t Value) Value {
	return trieRebuilt(t, func(k string, v Value) (Value, bool) {
		r := apply2(fb, f, v, k)
		if isOk(r) {
			return value(r), true
		}
		return nil, false
	})
}

func TrieCompact(t Value) Value {
	return trieRebuilt(t, func(_ string, o Value) (Value, bool) {
		if isSome(o) {
			return value(o), true
		}
		return nil, false
	})
}
