package flickrt

import (
	"sort"
	"strconv"
)

// Effect's Graph: building and changing graphs. Each change copies the
// graph and runs Effect's mutation on the copy, as the JS adapter's
// `Graph.mutate`; a `GraphError` Effect throws is the `Err` of a result.

var graphErrorTag = Tag("Graph.Types.GraphError.GraphError")

// graphError is a `GraphError` thrown by Effect's code, caught by attempt.
type graphError string

func graphFail(message string) { panic(graphError(message)) }

func missingNode(node int) { graphFail("Node " + strconv.Itoa(node) + " does not exist") }

// graphAttempt is `f` as a result: the `Err` of a `GraphError` it throws.
func graphAttempt(f func() Value) (out Value) {
	defer func() {
		if r := recover(); r != nil {
			e, ok := r.(graphError)
			if !ok {
				panic(r)
			}
			out = Err(NewData(graphErrorTag, string(e)))
		}
	}()
	return Ok(f())
}

func graphOf(v Value) *Graph { return v.(*Graph) }

// index is a node or edge index of Flick, an `Int`.
func index(v Value) int { return int(v.(int32)) }

func indexList(xs []int) Value {
	out := make([]Value, len(xs))
	for i, x := range xs {
		out[i] = int32(x)
	}
	return out
}

func newGraph(kind string) *Graph {
	return &Graph{kind: kind, adj: map[int][]int{}, radj: map[int][]int{}}
}

// hydrate is Effect's `hydrate`: the adjacency of the nodes and edges.
func (g *Graph) hydrate() *Graph {
	g.adj, g.radj = map[int][]int{}, map[int][]int{}
	for _, n := range g.nodes {
		g.adj[n.index], g.radj[n.index] = []int{}, []int{}
	}
	for _, e := range g.edges {
		g.adj[e.source] = append(g.adj[e.source], e.index)
		g.radj[e.target] = append(g.radj[e.target], e.index)
		if g.kind == "undirected" {
			g.adj[e.target] = append(g.adj[e.target], e.index)
			g.radj[e.source] = append(g.radj[e.source], e.index)
		}
	}
	g.nextNode, g.nextEdge = 0, 0
	if len(g.nodes) > 0 {
		g.nextNode = g.nodes[len(g.nodes)-1].index + 1
	}
	if len(g.edges) > 0 {
		g.nextEdge = g.edges[len(g.edges)-1].index + 1
	}
	return g
}

func cloneAdjacency(m map[int][]int) map[int][]int {
	out := make(map[int][]int, len(m))
	for k, v := range m {
		out[k] = append([]int{}, v...)
	}
	return out
}

// clone is Effect's `beginMutation`: a copy to change.
func (g *Graph) clone() *Graph {
	out := *g
	out.nodes = append([]graphNode{}, g.nodes...)
	out.edges = append([]graphEdge{}, g.edges...)
	out.adj, out.radj = cloneAdjacency(g.adj), cloneAdjacency(g.radj)
	return &out
}

// nodeAt is the position of the node `i`, -1 for none.
func (g *Graph) nodeAt(i int) int {
	p := sort.Search(len(g.nodes), func(k int) bool { return g.nodes[k].index >= i })
	if p < len(g.nodes) && g.nodes[p].index == i {
		return p
	}
	return -1
}

func (g *Graph) edgeAt(i int) int {
	p := sort.Search(len(g.edges), func(k int) bool { return g.edges[k].index >= i })
	if p < len(g.edges) && g.edges[p].index == i {
		return p
	}
	return -1
}

func (g *Graph) hasNode(i int) bool { return g.nodeAt(i) >= 0 }

func (g *Graph) edge(i int) graphEdge { return g.edges[g.edgeAt(i)] }

func (g *Graph) addNode(data Value) int {
	i := g.nextNode
	g.nodes = append(g.nodes, graphNode{i, data})
	g.adj[i], g.radj[i] = []int{}, []int{}
	g.nextNode++
	return i
}

func (g *Graph) addEdge(source, target int, data Value) int {
	if !g.hasNode(source) {
		missingNode(source)
	}
	if !g.hasNode(target) {
		missingNode(target)
	}
	i := g.nextEdge
	g.edges = append(g.edges, graphEdge{i, source, target, data})
	g.adj[source] = append(g.adj[source], i)
	g.radj[target] = append(g.radj[target], i)
	if g.kind == "undirected" {
		g.adj[target] = append(g.adj[target], i)
		g.radj[source] = append(g.radj[source], i)
	}
	g.nextEdge++
	return i
}

// without is `xs` without the first `x`, as `splice(indexOf(x), 1)`.
func withoutEdge(xs []int, x int) []int {
	for i, y := range xs {
		if y == x {
			return append(xs[:i:i], xs[i+1:]...)
		}
	}
	return xs
}

func (g *Graph) removeEdge(i int) bool {
	p := g.edgeAt(i)
	if p < 0 {
		return false
	}
	e := g.edges[p]
	g.adj[e.source] = withoutEdge(g.adj[e.source], i)
	g.radj[e.target] = withoutEdge(g.radj[e.target], i)
	if g.kind == "undirected" {
		g.adj[e.target] = withoutEdge(g.adj[e.target], i)
		g.radj[e.source] = withoutEdge(g.radj[e.source], i)
	}
	g.edges = append(g.edges[:p:p], g.edges[p+1:]...)
	return true
}

// removeNode is Effect's `removeNodeInternal`: the node and its edges.
func (g *Graph) removeNode(i int) bool {
	p := g.nodeAt(i)
	if p < 0 {
		return false
	}
	seen := map[int]bool{}
	edges := []int{}
	for _, e := range append(append([]int{}, g.adj[i]...), g.radj[i]...) {
		if !seen[e] {
			seen[e] = true
			edges = append(edges, e)
		}
	}
	for _, ei := range edges {
		e := g.edge(ei)
		if e.source != i {
			g.adj[e.source] = withoutEdge(g.adj[e.source], ei)
			if g.kind == "undirected" {
				g.radj[e.source] = withoutEdge(g.radj[e.source], ei)
			}
		}
		if e.target != i {
			g.radj[e.target] = withoutEdge(g.radj[e.target], ei)
			if g.kind == "undirected" {
				g.adj[e.target] = withoutEdge(g.adj[e.target], ei)
			}
		}
		q := g.edgeAt(ei)
		g.edges = append(g.edges[:q:q], g.edges[q+1:]...)
	}
	g.nodes = append(g.nodes[:p:p], g.nodes[p+1:]...)
	delete(g.adj, i)
	delete(g.radj, i)
	return true
}

// edit is the graph changed by `f` on a copy.
func edit(v Value, f func(m *Graph)) Value {
	m := graphOf(v).clone()
	f(m)
	return m
}

func GraphMake(kind Value) Value { return newGraph(kind.(string)) }
func GraphDirected() Value       { return newGraph("directed") }
func GraphUndirected() Value     { return newGraph("undirected") }

// GraphFromSnapshot is Effect's `fromSnapshot`, its checks in order.
func GraphFromSnapshot(s Value) Value {
	return graphAttempt(func() Value {
		r := record(s)
		kind := r.Get("graph_type").(string)
		if kind != "directed" && kind != "undirected" {
			graphFail("Snapshot type must be directed or undirected")
		}
		g := newGraph(kind)
		previous := -1
		known := map[int]bool{}
		for i, n := range List(r.Get("nodes")) {
			x := record(n)
			at := index(x.Get("index"))
			if at < 0 {
				graphFail("Node index at position " + strconv.Itoa(i) + " must be a non-negative safe integer")
			}
			if at <= previous {
				graphFail("Node indexes must be strictly increasing")
			}
			previous, known[at] = at, true
			g.nodes = append(g.nodes, graphNode{at, x.Get("data")})
		}
		previous = -1
		for i, e := range List(r.Get("edges")) {
			x := record(e)
			at, source, target := index(x.Get("index")), index(x.Get("source")), index(x.Get("target"))
			position := strconv.Itoa(i)
			switch {
			case at < 0:
				graphFail("Edge index at position " + position + " must be a non-negative safe integer")
			case at <= previous:
				graphFail("Edge indexes must be strictly increasing")
			case source < 0:
				graphFail("Edge source at position " + position + " must be a non-negative safe integer")
			case !known[source]:
				graphFail("Edge source " + strconv.Itoa(source) + " does not reference a node")
			case target < 0:
				graphFail("Edge target at position " + position + " must be a non-negative safe integer")
			case !known[target]:
				graphFail("Edge target " + strconv.Itoa(target) + " does not reference a node")
			}
			previous = at
			g.edges = append(g.edges, graphEdge{at, source, target, x.Get("data")})
		}
		return g.hydrate()
	})
}

func indexedNodes(g *Graph) []Value {
	out := make([]Value, len(g.nodes))
	for i, n := range g.nodes {
		out[i] = Rec("index", int32(n.index), "data", n.data)
	}
	return out
}

func indexedEdges(g *Graph) []Value {
	out := make([]Value, len(g.edges))
	for i, e := range g.edges {
		out[i] = Rec("index", int32(e.index), "source", int32(e.source), "target", int32(e.target), "data", e.data)
	}
	return out
}

func GraphToSnapshot(v Value) Value {
	g := graphOf(v)
	return Rec("graph_type", g.kind, "nodes", indexedNodes(g), "edges", indexedEdges(g))
}

func GraphAddNode(data, v Value) Value {
	var i int
	next := edit(v, func(m *Graph) { i = m.addNode(data) })
	return []Value{int32(i), next}
}

func GraphAddEdge(source, target, data, v Value) Value {
	return graphAttempt(func() Value {
		var i int
		next := edit(v, func(m *Graph) { i = m.addEdge(index(source), index(target), data) })
		return []Value{int32(i), next}
	})
}

func GraphRemoveNode(i, v Value) Value {
	return edit(v, func(m *Graph) { m.removeNode(index(i)) })
}

func GraphRemoveNodes(is, v Value) Value {
	return edit(v, func(m *Graph) {
		for _, i := range List(is) {
			m.removeNode(index(i))
		}
	})
}

func GraphRemoveEdge(i, v Value) Value {
	return edit(v, func(m *Graph) { m.removeEdge(index(i)) })
}

func GraphRemoveEdges(is, v Value) Value {
	return edit(v, func(m *Graph) {
		for _, i := range List(is) {
			m.removeEdge(index(i))
		}
	})
}

func GraphUpdateNode(fb *Fiber, i, f, v Value) Value {
	return edit(v, func(m *Graph) {
		if p := m.nodeAt(index(i)); p >= 0 {
			m.nodes[p].data = Apply(fb, f, m.nodes[p].data)
		}
	})
}

func GraphUpdateEdge(fb *Fiber, i, f, v Value) Value {
	return edit(v, func(m *Graph) {
		if p := m.edgeAt(index(i)); p >= 0 {
			m.edges[p].data = Apply(fb, f, m.edges[p].data)
		}
	})
}

func GraphMapNodes(fb *Fiber, f, v Value) Value {
	return edit(v, func(m *Graph) {
		for p := range m.nodes {
			m.nodes[p].data = Apply(fb, f, m.nodes[p].data)
		}
	})
}

func GraphMapEdges(fb *Fiber, f, v Value) Value {
	return edit(v, func(m *Graph) {
		for p := range m.edges {
			m.edges[p].data = Apply(fb, f, m.edges[p].data)
		}
	})
}

// filterNodes keeps the nodes `keep` gives new data, then removes the
// others, as Effect's `filterMapNodes`.
func filterNodes(v Value, keep func(Value) (Value, bool)) Value {
	return edit(v, func(m *Graph) {
		remove := []int{}
		for p, n := range m.nodes {
			if data, ok := keep(n.data); ok {
				m.nodes[p].data = data
			} else {
				remove = append(remove, n.index)
			}
		}
		for _, i := range remove {
			m.removeNode(i)
		}
	})
}

func filterEdges(v Value, keep func(Value) (Value, bool)) Value {
	return edit(v, func(m *Graph) {
		remove := []int{}
		for p, e := range m.edges {
			if data, ok := keep(e.data); ok {
				m.edges[p].data = data
			} else {
				remove = append(remove, e.index)
			}
		}
		for _, i := range remove {
			m.removeEdge(i)
		}
	})
}

func optionKeep(fb *Fiber, f Value) func(Value) (Value, bool) {
	return func(x Value) (Value, bool) {
		if o := Apply(fb, f, x); isSome(o) {
			return value(o), true
		}
		return nil, false
	}
}

func predicateKeep(fb *Fiber, f Value) func(Value) (Value, bool) {
	return func(x Value) (Value, bool) { return x, Bool(Apply(fb, f, x)) }
}

func GraphFilterMapNodes(fb *Fiber, f, v Value) Value { return filterNodes(v, optionKeep(fb, f)) }
func GraphFilterMapEdges(fb *Fiber, f, v Value) Value { return filterEdges(v, optionKeep(fb, f)) }
func GraphFilterNodes(fb *Fiber, f, v Value) Value    { return filterNodes(v, predicateKeep(fb, f)) }
func GraphFilterEdges(fb *Fiber, f, v Value) Value    { return filterEdges(v, predicateKeep(fb, f)) }

func GraphReverse(v Value) Value {
	return edit(v, func(m *Graph) {
		if m.kind == "undirected" {
			return
		}
		for p, e := range m.edges {
			m.edges[p].source, m.edges[p].target = e.target, e.source
		}
		m.adj, m.radj = m.radj, m.adj
	})
}

// GraphSum is `sum that self`: the nodes and edges of `self`, then of
// `that`, under new indexes.
func GraphSum(that, self Value) Value {
	a, b := graphOf(self), graphOf(that)
	assertSameKind(a, b)
	out := newGraph(a.kind)
	for _, g := range []*Graph{a, b} {
		moved := map[int]int{}
		for _, n := range g.nodes {
			moved[n.index] = out.addNode(n.data)
		}
		for _, e := range g.edges {
			out.addEdge(moved[e.source], moved[e.target], e.data)
		}
	}
	return out
}

func assertSameKind(a, b *Graph) {
	if a.kind != b.kind {
		graphFail("Cannot combine " + a.kind + " and " + b.kind + " graphs")
	}
}

// GraphComplement is `complement f self`: an edge, made by `f`, between
// each two nodes `self` does not join.
func GraphComplement(fb *Fiber, f, self Value) Value {
	g := graphOf(self)
	out := newGraph(g.kind)
	out.nodes = make([]graphNode, 0, len(g.nodes))
	for _, n := range g.nodes {
		out.addNode(n.data)
	}
	rows, _ := g.rows(false)
	marks := make([]int, len(g.nodes))
	for i := range g.nodes {
		for _, j := range rows[i] {
			marks[j] = i + 1
		}
		start := 0
		if g.kind == "undirected" {
			start = i + 1
		}
		for j := start; j < len(g.nodes); j++ {
			if i == j || marks[j] == i+1 {
				continue
			}
			out.addEdge(i, j, apply2(fb, f, g.nodes[i].data, g.nodes[j].data))
		}
	}
	return out
}

// GraphNeighborhood is the subgraph of the nodes a bounded breadth-first
// walk from `i` reaches, in the order it reaches them.
func GraphNeighborhood(i, options, v Value) Value {
	return graphAttempt(func() Value {
		g := graphOf(v)
		o := record(options)
		radius := traversalRadius(o.Get("radius"), 1)
		direction := "outgoing"
		if d, ok := some(o.Get("direction")); ok {
			direction = d.(string)
		}
		reached := g.bfs([]int{index(i)}, direction, radius)
		out := newGraph(g.kind)
		moved := map[int]int{}
		for _, p := range reached {
			moved[g.nodes[p].index] = out.addNode(g.nodes[p].data)
		}
		for _, e := range g.edges {
			s, okS := moved[e.source]
			t, okT := moved[e.target]
			if okS && okT {
				out.addEdge(s, t, e.data)
			}
		}
		return out
	})
}

func GraphInducedSubgraph(is, v Value) Value {
	return graphAttempt(func() Value {
		g := graphOf(v)
		selected := map[int]bool{}
		for _, i := range List(is) {
			if !g.hasNode(index(i)) {
				missingNode(index(i))
			}
			selected[index(i)] = true
		}
		out := newGraph(g.kind)
		for _, n := range g.nodes {
			if selected[n.index] {
				out.nodes = append(out.nodes, n)
			}
		}
		for _, e := range g.edges {
			if selected[e.source] && selected[e.target] {
				out.edges = append(out.edges, e)
			}
		}
		return out.hydrate()
	})
}
