package flickrt

// Graph queries: nodes, edges, degrees and neighbors.

func GraphGetNode(i, v Value) Value {
	g := graphOf(v)
	if p := g.nodeAt(index(i)); p >= 0 {
		return Some(g.nodes[p].data)
	}
	return None
}

func GraphHasNode(i, v Value) Value { return graphOf(v).hasNode(index(i)) }

func GraphFindNode(fb *Fiber, f, v Value) Value {
	for _, n := range graphOf(v).nodes {
		if Bool(Apply(fb, f, n.data)) {
			return Some(int32(n.index))
		}
	}
	return None
}

func GraphFindNodes(fb *Fiber, f, v Value) Value {
	out := []Value{}
	for _, n := range graphOf(v).nodes {
		if Bool(Apply(fb, f, n.data)) {
			out = append(out, int32(n.index))
		}
	}
	return out
}

func edgeTest(fb *Fiber, f Value, e graphEdge) bool {
	return Bool(Apply(fb, apply2(fb, f, e.data, int32(e.source)), int32(e.target)))
}

func GraphFindEdge(fb *Fiber, f, v Value) Value {
	for _, e := range graphOf(v).edges {
		if edgeTest(fb, f, e) {
			return Some(int32(e.index))
		}
	}
	return None
}

func GraphFindEdges(fb *Fiber, f, v Value) Value {
	out := []Value{}
	for _, e := range graphOf(v).edges {
		if edgeTest(fb, f, e) {
			out = append(out, int32(e.index))
		}
	}
	return out
}

func GraphGetEdge(i, v Value) Value {
	g := graphOf(v)
	if p := g.edgeAt(index(i)); p >= 0 {
		return Some(edgeRecord(g.edges[p]))
	}
	return None
}

// neighbor is the node across `e` from `node`.
func (g *Graph) neighbor(node int, e graphEdge) int {
	if g.kind == "undirected" && e.target == node {
		return e.source
	}
	return e.target
}

func GraphHasEdge(source, target, v Value) Value {
	g, s, t := graphOf(v), index(source), index(target)
	for _, ei := range g.adj[s] {
		if g.neighbor(s, g.edge(ei)) == t {
			return true
		}
	}
	return false
}

// existing is the node `i`, which must exist.
func (g *Graph) existing(i Value) int {
	if !g.hasNode(index(i)) {
		missingNode(index(i))
	}
	return index(i)
}

// GraphIncidentEdges merges the outgoing and incoming edges by index.
func GraphIncidentEdges(i, v Value) Value {
	return graphAttempt(func() Value {
		g := graphOf(v)
		n := g.existing(i)
		out, in := g.adj[n], g.radj[n]
		result := []int{}
		if g.kind == "undirected" {
			previous := -1
			for _, e := range out {
				if e != previous {
					result = append(result, e)
					previous = e
				}
			}
			return indexList(result)
		}
		a, b := 0, 0
		for a < len(out) && b < len(in) {
			switch {
			case out[a] < in[b]:
				result = append(result, out[a])
				a++
			case in[b] < out[a]:
				result = append(result, in[b])
				b++
			default:
				result = append(result, out[a])
				a, b = a+1, b+1
			}
		}
		result = append(append(result, out[a:]...), in[b:]...)
		return indexList(result)
	})
}

func (g *Graph) directedOnly(message string) {
	if g.kind == "undirected" {
		graphFail(message)
	}
}

func GraphOutgoingEdges(i, v Value) Value {
	return graphAttempt(func() Value {
		g := graphOf(v)
		g.directedOnly("Cannot get outgoing edges of undirected graph")
		return indexList(g.adj[g.existing(i)])
	})
}

func GraphIncomingEdges(i, v Value) Value {
	return graphAttempt(func() Value {
		g := graphOf(v)
		g.directedOnly("Cannot get incoming edges of undirected graph")
		return indexList(g.radj[g.existing(i)])
	})
}

func GraphEdgesBetween(source, target, v Value) Value {
	return graphAttempt(func() Value {
		g := graphOf(v)
		s, t := g.existing(source), g.existing(target)
		result := []int{}
		previous := -1
		for _, ei := range g.adj[s] {
			if ei == previous {
				continue
			}
			previous = ei
			if g.neighbor(s, g.edge(ei)) == t {
				result = append(result, ei)
			}
		}
		return indexList(result)
	})
}

func GraphDegree(i, v Value) Value {
	return graphAttempt(func() Value {
		g := graphOf(v)
		if g.kind == "directed" {
			graphFail("Cannot get degree of directed graph")
		}
		return int32(len(g.adj[g.existing(i)]))
	})
}

func GraphOutDegree(i, v Value) Value {
	return graphAttempt(func() Value {
		g := graphOf(v)
		g.directedOnly("Cannot get outgoing edges of undirected graph")
		return int32(len(g.adj[g.existing(i)]))
	})
}

func GraphInDegree(i, v Value) Value {
	return graphAttempt(func() Value {
		g := graphOf(v)
		g.directedOnly("Cannot get incoming edges of undirected graph")
		return int32(len(g.radj[g.existing(i)]))
	})
}

// uniqueNeighbors is each node across the edges of `node` in the
// direction once, in adjacency order.
func (g *Graph) uniqueNeighbors(node int, incoming bool) Value {
	adjacency := g.adj
	if incoming {
		adjacency = g.radj
	}
	seen := map[int]bool{}
	out := []Value{}
	for _, ei := range adjacency[node] {
		e := g.edge(ei)
		next := e.target
		if g.kind == "undirected" && e.source != node || g.kind != "undirected" && incoming {
			next = e.source
		}
		if !seen[next] {
			seen[next] = true
			out = append(out, int32(next))
		}
	}
	return out
}

// directedOrDefect is a query typed for directed graphs only, whose
// `GraphError` is a defect as in the JS adapter.
func (g *Graph) directedOrDefect(message string) {
	if g.kind == "undirected" {
		panic(Defect{"GraphError: " + message})
	}
}

func GraphNeighbors(i, v Value) Value { return graphOf(v).uniqueNeighbors(index(i), false) }

func GraphSuccessors(i, v Value) Value {
	g := graphOf(v)
	g.directedOrDefect("Cannot get successors of undirected graph")
	return g.uniqueNeighbors(index(i), false)
}

func GraphPredecessors(i, v Value) Value {
	g := graphOf(v)
	g.directedOrDefect("Cannot get predecessors of undirected graph")
	return g.uniqueNeighbors(index(i), true)
}

func GraphNeighborsDirected(i, direction, v Value) Value {
	g := graphOf(v)
	g.directedOrDefect("Cannot get directed neighbors of undirected graph")
	return g.uniqueNeighbors(index(i), direction.(string) == "incoming")
}
