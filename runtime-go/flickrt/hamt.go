package flickrt

import "math/bits"

// A persistent hash array mapped trie, walked as Effect's HAMT on the JS
// target: a branch has its children by five bits of the hash, from the
// lowest, in order of those bits; a leaf has the entries of one hash, in
// insertion order. A leaf sits as high as its hash differs from the
// others', so a lookup reads a few nodes and a change copies only them.

type hnode struct {
	bitmap uint32
	kids   []*hnode
	leaf   []entry // the entries of a leaf, all of hash `hash`; nil for a branch
	hash   int32
}

func (n *hnode) isLeaf() bool { return n.leaf != nil }

func frag(h int32, shift uint) uint32 { return uint32(h) >> shift & 31 }

// slot is where the child of fragment `f` is among a branch's children.
func (n *hnode) slot(f uint32) int { return bits.OnesCount32(n.bitmap & (1<<f - 1)) }

func (n *hnode) find(fb *Fiber, w *Witness, k Value, h int32, shift uint) (entry, bool) {
	for n != nil {
		if n.isLeaf() {
			if n.hash == h {
				for _, e := range n.leaf {
					if w.equal(fb, k, e.key) {
						return e, true
					}
				}
			}
			return entry{}, false
		}
		f := frag(h, shift)
		if n.bitmap&(1<<f) == 0 {
			return entry{}, false
		}
		n, shift = n.kids[n.slot(f)], shift+5
	}
	return entry{}, false
}

// with puts `e` in the node: a key equal to one there replaces it in its
// place, or with `keep` leaves it. It gives the node, the same one when
// nothing changed, and whether the key was new. Without a witness the key
// is known to be new.
func (n *hnode) with(fb *Fiber, w *Witness, e entry, shift uint, keep bool) (*hnode, bool) {
	switch {
	case n == nil:
		return &hnode{leaf: []entry{e}, hash: e.hash}, true
	case n.isLeaf() && n.hash == e.hash:
		for i, old := range n.leaf {
			if w == nil || !w.equal(fb, e.key, old.key) {
				continue
			}
			if keep || Equals(old.value, e.value) {
				return n, false
			}
			leaf := append([]entry(nil), n.leaf...)
			leaf[i] = e
			return &hnode{leaf: leaf, hash: n.hash}, false
		}
		return &hnode{leaf: append(append(make([]entry, 0, len(n.leaf)+1), n.leaf...), e), hash: n.hash}, true
	case n.isLeaf():
		// another hash where a leaf is: a branch over both
		branch := &hnode{bitmap: 1 << frag(n.hash, shift), kids: []*hnode{n}}
		return branch.with(fb, w, e, shift, keep)
	}
	f := frag(e.hash, shift)
	i := n.slot(f)
	if n.bitmap&(1<<f) == 0 {
		kids := make([]*hnode, 0, len(n.kids)+1)
		kids = append(append(append(kids, n.kids[:i]...), &hnode{leaf: []entry{e}, hash: e.hash}), n.kids[i:]...)
		return &hnode{bitmap: n.bitmap | 1<<f, kids: kids}, true
	}
	kid, added := n.kids[i].with(fb, w, e, shift+5, keep)
	if kid == n.kids[i] {
		return n, false
	}
	kids := append([]*hnode(nil), n.kids...)
	kids[i] = kid
	return &hnode{bitmap: n.bitmap, kids: kids}, added
}

// without is the node without the key, the same one when it lacks it,
// and whether it had it.
func (n *hnode) without(fb *Fiber, w *Witness, k Value, h int32, shift uint) (*hnode, bool) {
	if n == nil {
		return nil, false
	}
	if n.isLeaf() {
		if n.hash != h {
			return n, false
		}
		for i, e := range n.leaf {
			if w.equal(fb, k, e.key) {
				if len(n.leaf) == 1 {
					return nil, true
				}
				leaf := append(append(make([]entry, 0, len(n.leaf)-1), n.leaf[:i]...), n.leaf[i+1:]...)
				return &hnode{leaf: leaf, hash: n.hash}, true
			}
		}
		return n, false
	}
	f := frag(h, shift)
	if n.bitmap&(1<<f) == 0 {
		return n, false
	}
	i := n.slot(f)
	kid, removed := n.kids[i].without(fb, w, k, h, shift+5)
	if !removed {
		return n, false
	}
	if kid != nil {
		kids := append([]*hnode(nil), n.kids...)
		kids[i] = kid
		return &hnode{bitmap: n.bitmap, kids: kids}, true
	}
	kids := append(append(make([]*hnode, 0, len(n.kids)-1), n.kids[:i]...), n.kids[i+1:]...)
	switch {
	case len(kids) == 0:
		return nil, true
	case len(kids) == 1 && kids[0].isLeaf():
		// a leaf alone in a branch takes its place, as when it was put
		return kids[0], true
	}
	return &hnode{bitmap: n.bitmap &^ (1 << f), kids: kids}, true
}

// each gives the entries in walk order, until `f` says to stop.
func (n *hnode) each(f func(entry) bool) bool {
	if n == nil {
		return true
	}
	if n.isLeaf() {
		for _, e := range n.leaf {
			if !f(e) {
				return false
			}
		}
		return true
	}
	for _, kid := range n.kids {
		if !kid.each(f) {
			return false
		}
	}
	return true
}
