package flickrt

// Graph is Effect's immutable graph: its kind, its nodes and edges by
// increasing index (Effect's maps keep insertion order, and indexes only
// grow), each node's outgoing and incoming edges in Effect's adjacency
// order, and the next indexes. A change works on a copy, as `Graph.mutate`.
type Graph struct {
	kind               string
	nodes              []graphNode
	edges              []graphEdge
	adj, radj          map[int][]int
	nextNode, nextEdge int
}

type graphNode struct {
	index int
	data  Value
}

type graphEdge struct {
	index, source, target int
	data                  Value
}

func GraphNodeCount(g Value) Value { return int32(len(g.(*Graph).nodes)) }
func GraphEdgeCount(g Value) Value { return int32(len(g.(*Graph).edges)) }

// GraphJson is Effect's `Schema.Graph` as JSON: `{ type, nodes, edges }`,
// the indices strictly increasing and each edge's endpoints encoded nodes.
func GraphJson(kind, node, edge Value) Value {
	k := kind.(string)
	field := func(name string, c Value) []Value { return []Value{name, DslField(name, c)} }
	block := func(name string, fields ...[]Value) Value {
		var entries []Value
		for _, f := range fields {
			entries = append(entries, f...)
		}
		return CodecBlock(name, entries)
	}
	wire := block("Graph",
		field("type", CodecLiteral(k)),
		field("nodes", CodecList(block("IndexedNode", field("index", CodecNatural), field("data", node)))),
		field("edges", CodecList(block("IndexedEdge", field("index", CodecNatural), field("source", CodecNatural),
			field("target", CodecNatural), field("data", edge)))),
	)
	return &fallible{&stage{wire.(codec), func(_ *Fiber, v Value) (Value, []issue) { return graphDecode(k, record(v)) },
		func(_ *Fiber, v Value) (Value, []issue) {
			g, ok := v.(*Graph)
			if !ok || g.kind != k {
				return nil, fail("an invalid value")
			}
			return g.encoded(), nil
		}}}
}

// invalidAt is the issue of a part of the graph at `path`.
func invalidAt(path ...any) []issue {
	out := fail("an invalid value")
	for i := len(path) - 1; i >= 0; i-- {
		out = under(path[i], out)
	}
	return out
}

func graphDecode(kind string, r *Record) (Value, []issue) {
	g := &Graph{kind: kind}
	known := map[int]bool{}
	previous := -1
	for i, n := range List(r.Get("nodes")) {
		x := record(n)
		index := int(numberOf(x.Get("index")))
		if index <= previous {
			return nil, invalidAt("nodes", i, "index")
		}
		previous, known[index] = index, true
		g.nodes = append(g.nodes, graphNode{index, x.Get("data")})
	}
	previous = -1
	for i, e := range List(r.Get("edges")) {
		x := record(e)
		index := int(numberOf(x.Get("index")))
		source, target := int(numberOf(x.Get("source"))), int(numberOf(x.Get("target")))
		switch {
		case index <= previous:
			return nil, invalidAt("edges", i, "index")
		case !known[source]:
			return nil, invalidAt("edges", i, "source")
		case !known[target]:
			return nil, invalidAt("edges", i, "target")
		}
		previous = index
		g.edges = append(g.edges, graphEdge{index, source, target, x.Get("data")})
	}
	return g.hydrate(), nil
}

// encoded is the graph's snapshot, as the JSON codec's record.
func (g *Graph) encoded() Value {
	nodes := make([]Value, len(g.nodes))
	for i, n := range g.nodes {
		nodes[i] = Rec("index", float64(n.index), "data", n.data)
	}
	edges := make([]Value, len(g.edges))
	for i, e := range g.edges {
		edges[i] = Rec("index", float64(e.index), "source", float64(e.source), "target", float64(e.target), "data", e.data)
	}
	return Rec("type", g.kind, "nodes", nodes, "edges", edges)
}
