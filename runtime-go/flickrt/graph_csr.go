package flickrt

// The compact (CSR) view of a graph, which the traversals and algorithms
// walk: a node is its position in insertion order, its neighbors are in
// adjacency order, and an edge is its position too.

// rows is, for each node position, the positions of its neighbors and of
// the edges to them; `incoming` follows directed edges backwards.
func (g *Graph) rows(incoming bool) (neighbors, edges [][]int) {
	adjacency := g.adj
	if incoming {
		adjacency = g.radj
	}
	neighbors, edges = make([][]int, len(g.nodes)), make([][]int, len(g.nodes))
	for p, n := range g.nodes {
		for _, ei := range adjacency[n.index] {
			q := g.edgeAt(ei)
			e := g.edges[q]
			next := e.target
			if g.kind == "undirected" && e.source != n.index || g.kind != "undirected" && incoming {
				next = e.source
			}
			neighbors[p] = append(neighbors[p], g.nodeAt(next))
			edges[p] = append(edges[p], q)
		}
	}
	return neighbors, edges
}

func (g *Graph) outgoing() [][]int { out, _ := g.rows(false); return out }
func (g *Graph) incoming() [][]int { in, _ := g.rows(true); return in }

// adjacencies is the rows a walk in `direction` follows: a directed graph
// walked both ways needs the incoming rows too.
func (g *Graph) adjacencies(direction string) (primary, secondary [][]int) {
	switch {
	case g.kind == "directed" && direction == "incoming":
		return g.incoming(), nil
	case g.kind == "directed" && direction == "undirected":
		return g.outgoing(), g.incoming()
	}
	return g.outgoing(), nil
}

// endpoints are the positions of the ends of each edge.
func (g *Graph) endpoints() (sources, targets []int) {
	sources, targets = make([]int, len(g.edges)), make([]int, len(g.edges))
	for q, e := range g.edges {
		sources[q], targets[q] = g.nodeAt(e.source), g.nodeAt(e.target)
	}
	return sources, targets
}

func (g *Graph) nodeIds(positions []int) Value {
	out := make([]Value, len(positions))
	for i, p := range positions {
		out[i] = int32(g.nodes[p].index)
	}
	return out
}
