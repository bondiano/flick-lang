package flickrt

// Graph set operations: nodes and edges matched by identity in a map that
// keeps the first equal key where it was put and the last value.

// eqMap is an insertion-ordered map of keys equal by `eq`.
type eqMap struct {
	keys, vals []Value
	at         map[int32][]int
	eq         func(a, b Value) bool
	hash       func(Value) int32
}

func newEqMap(eq func(a, b Value) bool, hash func(Value) int32) *eqMap {
	return &eqMap{at: map[int32][]int{}, eq: eq, hash: hash}
}

func (m *eqMap) find(k Value) (int, int32) {
	h := m.hash(k)
	for _, i := range m.at[h] {
		if m.eq(m.keys[i], k) {
			return i, h
		}
	}
	return -1, h
}

func (m *eqMap) set(k, v Value) {
	i, h := m.find(k)
	if i >= 0 {
		m.vals[i] = v
		return
	}
	m.at[h] = append(m.at[h], len(m.keys))
	m.keys, m.vals = append(m.keys, k), append(m.vals, v)
}

func (m *eqMap) has(k Value) bool { i, _ := m.find(k); return i >= 0 }

func (m *eqMap) get(k Value) (Value, bool) {
	if i, _ := m.find(k); i >= 0 {
		return m.vals[i], true
	}
	return nil, false
}

// identityHash buckets by the runtime's hash where it has one; equality
// decides either way.
func identityHash(v Value) (h int32) {
	defer func() {
		if recover() != nil {
			h = 0
		}
	}()
	return hash(v)
}

// edgeIdentity is the identity of an edge: the identities of its ends and
// its own; an undirected one matches either way round.
type edgeIdentity struct{ source, target, identity Value }

func edgeIdentities(kind string) *eqMap {
	return newEqMap(func(a, b Value) bool {
		x, y := a.(edgeIdentity), b.(edgeIdentity)
		if !Equals(x.identity, y.identity) {
			return false
		}
		same := Equals(x.source, y.source) && Equals(x.target, y.target)
		return same || kind == "undirected" && Equals(x.source, y.target) && Equals(x.target, y.source)
	}, func(v Value) int32 {
		x := v.(edgeIdentity)
		return identityHash(x.identity) ^ (identityHash(x.source) + identityHash(x.target))
	})
}

// identities are the `IdentityOptions` functions, the data itself by
// default.
type identities struct {
	node, edge func(Value) Value
}

func identitiesOf(fb *Fiber, options Value) identities {
	o := record(options)
	of := func(f Value) func(Value) Value {
		if g, ok := some(f); ok {
			return func(x Value) Value { return Apply(fb, g, x) }
		}
		return func(x Value) Value { return x }
	}
	return identities{of(o.Get("nodeIdentity")), of(o.Get("edgeIdentity"))}
}

// nodeMaps is the data by identity, and the identity of each node.
func (ids identities) nodeMaps(g *Graph) (*eqMap, map[int]Value) {
	byIdentity, byIndex := newEqMap(Equals, identityHash), map[int]Value{}
	for _, n := range g.nodes {
		id := ids.node(n.data)
		byIdentity.set(id, n.data)
		byIndex[n.index] = id
	}
	return byIdentity, byIndex
}

func (ids identities) edgeMap(g *Graph, byIndex map[int]Value) *eqMap {
	out := edgeIdentities(g.kind)
	for _, e := range g.edges {
		out.set(edgeIdentity{byIndex[e.source], byIndex[e.target], ids.edge(e.data)}, e.data)
	}
	return out
}

// build is a graph of the nodes, then of the edges whose ends are among
// them, under new indexes.
func build(kind string, nodes *eqMap, edges []Value, data []Value) *Graph {
	out := newGraph(kind)
	indexes := newEqMap(Equals, identityHash)
	for i, id := range nodes.keys {
		indexes.set(id, out.addNode(nodes.vals[i]))
	}
	for i, e := range edges {
		id := e.(edgeIdentity)
		s, okS := indexes.get(id.source)
		t, okT := indexes.get(id.target)
		if okS && okT {
			out.addEdge(s.(int), t.(int), data[i])
		}
	}
	return out
}

func graphOperands(fb *Fiber, options, that, self Value) (identities, *Graph, *Graph) {
	a, b := graphOf(self), graphOf(that)
	assertSameKind(a, b)
	return identitiesOf(fb, options), a, b
}

func GraphCompose(fb *Fiber, options, that, self Value) Value {
	ids, a, b := graphOperands(fb, options, that, self)
	selfNodes, selfIndex := ids.nodeMaps(a)
	thatNodes, thatIndex := ids.nodeMaps(b)
	nodes := newEqMap(Equals, identityHash)
	edges := ids.edgeMap(a, selfIndex)
	for _, m := range []*eqMap{selfNodes, thatNodes} {
		for i, k := range m.keys {
			nodes.set(k, m.vals[i])
		}
	}
	more := ids.edgeMap(b, thatIndex)
	for i, k := range more.keys {
		edges.set(k, more.vals[i])
	}
	return build(a.kind, nodes, edges.keys, edges.vals)
}

func GraphIntersection(fb *Fiber, options, that, self Value) Value {
	ids, a, b := graphOperands(fb, options, that, self)
	selfNodes, selfIndex := ids.nodeMaps(a)
	thatNodes, thatIndex := ids.nodeMaps(b)
	nodes := newEqMap(Equals, identityHash)
	selfEdges := ids.edgeMap(a, selfIndex)
	thatEdges := edgeIdentities(b.kind)
	for i, k := range selfNodes.keys {
		if thatNodes.has(k) {
			nodes.set(k, selfNodes.vals[i])
		}
	}
	for _, e := range b.edges {
		s, t := thatIndex[e.source], thatIndex[e.target]
		if nodes.has(s) && nodes.has(t) {
			thatEdges.set(edgeIdentity{s, t, ids.edge(e.data)}, e.data)
		}
	}
	keys, data := []Value{}, []Value{}
	for i, k := range thatEdges.keys {
		if selfEdges.has(k) {
			keys, data = append(keys, k), append(data, thatEdges.vals[i])
		}
	}
	return build(a.kind, nodes, keys, data)
}

func GraphDifference(fb *Fiber, options, that, self Value) Value {
	ids, a, b := graphOperands(fb, options, that, self)
	selfNodes, selfIndex := ids.nodeMaps(a)
	_, thatIndex := ids.nodeMaps(b)
	thatEdges := ids.edgeMap(b, thatIndex)
	keys, data := []Value{}, []Value{}
	for _, e := range a.edges {
		id := edgeIdentity{selfIndex[e.source], selfIndex[e.target], ids.edge(e.data)}
		if !thatEdges.has(id) {
			keys, data = append(keys, id), append(data, e.data)
		}
	}
	return build(a.kind, selfNodes, keys, data)
}

func GraphSymmetricDifference(fb *Fiber, options, that, self Value) Value {
	ids, a, b := graphOperands(fb, options, that, self)
	selfNodes, selfIndex := ids.nodeMaps(a)
	thatNodes, thatIndex := ids.nodeMaps(b)
	nodes := newEqMap(Equals, identityHash)
	selfEdges, thatEdges := ids.edgeMap(a, selfIndex), ids.edgeMap(b, thatIndex)
	for _, m := range []*eqMap{selfNodes, thatNodes} {
		for i, k := range m.keys {
			nodes.set(k, m.vals[i])
		}
	}
	keys, data := []Value{}, []Value{}
	for _, pair := range [][2]*eqMap{{selfEdges, thatEdges}, {thatEdges, selfEdges}} {
		for i, k := range pair[0].keys {
			if !pair[1].has(k) {
				keys, data = append(keys, k), append(data, pair[0].vals[i])
			}
		}
	}
	return build(a.kind, nodes, keys, data)
}
