package flickrt

import (
	"math"
	"strconv"
)

// Graph walkers, walked at once as the JS adapter's are, so a `GraphError`
// of the walk is the `Err`: a walker is its visits in order.
type graphWalker struct{ indices, values []Value }

func (g *Graph) walker(positions []int) *graphWalker {
	w := &graphWalker{make([]Value, len(positions)), make([]Value, len(positions))}
	for i, p := range positions {
		w.indices[i], w.values[i] = int32(g.nodes[p].index), g.nodes[p].data
	}
	return w
}

func GraphIndices(w Value) Value { return append([]Value{}, w.(*graphWalker).indices...) }
func GraphValues(w Value) Value  { return append([]Value{}, w.(*graphWalker).values...) }

func GraphEntries(w Value) Value {
	x := w.(*graphWalker)
	out := make([]Value, len(x.indices))
	for i := range out {
		out[i] = []Value{x.indices[i], x.values[i]}
	}
	return out
}

// traversalRadius is the radius of a walk: none is `fallback` (Infinity
// for a walk).
func traversalRadius(o Value, fallback float64) float64 {
	if r, ok := some(o); ok {
		if r.(int32) < 0 {
			graphFail("Traversal radius must be a non-negative integer or Infinity")
		}
		return float64(r.(int32))
	}
	return fallback
}

// starts is the positions of the start nodes, none when not given.
func (g *Graph) starts(o Value) []int {
	out := []int{}
	if s, ok := some(o); ok {
		for _, i := range List(s) {
			p := g.nodeAt(index(i))
			if p < 0 {
				missingNode(index(i))
			}
			out = append(out, p)
		}
	}
	return out
}

func directionOf(o Value) string {
	if d, ok := some(o); ok {
		return d.(string)
	}
	return "outgoing"
}

// within is a bounded breadth-first pass: the nodes no further than
// `radius` from the starts, marked.
func reachedWithin(n int, starts []int, primary, secondary [][]int, radius float64) []bool {
	reached := make([]bool, n)
	queue, depths := []int{}, []float64{}
	for _, p := range starts {
		if !reached[p] {
			reached[p] = true
			queue, depths = append(queue, p), append(depths, 0)
		}
	}
	for head := 0; head < len(queue); head++ {
		current, depth := queue[head], depths[head]
		if depth >= radius {
			continue
		}
		for _, rows := range [][][]int{primary, secondary} {
			if rows == nil {
				continue
			}
			for _, next := range rows[current] {
				if !reached[next] {
					reached[next] = true
					queue, depths = append(queue, next), append(depths, depth+1)
				}
			}
		}
	}
	return reached
}

// bfs is a breadth-first walk from the starts: positions in visiting order.
func (g *Graph) bfs(starts []int, direction string, radius float64) []int {
	for _, i := range starts {
		if !g.hasNode(i) {
			missingNode(i)
		}
	}
	positions := make([]int, len(starts))
	for k, i := range starts {
		positions[k] = g.nodeAt(i)
	}
	return g.bfsFrom(positions, direction, radius)
}

func (g *Graph) bfsFrom(starts []int, direction string, radius float64) []int {
	primary, secondary := g.adjacencies(direction)
	discovered := make([]bool, len(g.nodes))
	queue, depths := []int{}, []float64{}
	for _, p := range starts {
		if !discovered[p] {
			discovered[p] = true
			queue, depths = append(queue, p), append(depths, 0)
		}
	}
	for head := 0; head < len(queue); head++ {
		current, depth := queue[head], depths[head]
		if depth >= radius {
			continue
		}
		for _, rows := range [][][]int{primary, secondary} {
			if rows == nil {
				continue
			}
			for _, next := range rows[current] {
				if !discovered[next] {
					discovered[next] = true
					queue, depths = append(queue, next), append(depths, depth+1)
				}
			}
		}
	}
	return queue
}

// dfs is a depth-first walk: preorder, neighbors pushed in reverse so the
// first is walked first; a bounded walk keeps to the nodes within reach.
func (g *Graph) dfs(starts []int, direction string, radius float64) []int {
	primary, secondary := g.adjacencies(direction)
	var reached []bool
	if !math.IsInf(radius, 1) {
		reached = reachedWithin(len(g.nodes), starts, primary, secondary, radius)
	}
	yielded := make([]bool, len(g.nodes))
	stack := []int{}
	for i := len(starts) - 1; i >= 0; i-- {
		stack = append(stack, starts[i])
	}
	push := func(rows [][]int, current int) {
		for i := len(rows[current]) - 1; i >= 0; i-- {
			next := rows[current][i]
			if (reached == nil || reached[next]) && !yielded[next] {
				stack = append(stack, next)
			}
		}
	}
	out := []int{}
	for len(stack) > 0 {
		current := stack[len(stack)-1]
		stack = stack[:len(stack)-1]
		if yielded[current] {
			continue
		}
		if secondary != nil {
			push(secondary, current)
		}
		push(primary, current)
		yielded[current] = true
		out = append(out, current)
	}
	return out
}

// dfsPostOrder is a depth-first walk in postorder: each node after the
// nodes it reaches, starts taken in order.
func (g *Graph) dfsPostOrder(starts []int, direction string, radius float64) []int {
	primary, secondary := g.adjacencies(direction)
	var reached []bool
	if !math.IsInf(radius, 1) {
		reached = reachedWithin(len(g.nodes), starts, primary, secondary, radius)
	}
	type frame struct{ node, primary, secondary int }
	stack := []frame{}
	discovered := make([]bool, len(g.nodes))
	push := func(node int) {
		if (reached == nil || reached[node]) && !discovered[node] {
			discovered[node] = true
			stack = append(stack, frame{node, 0, 0})
		}
	}
	out := []int{}
	for next := 0; ; {
		for len(stack) == 0 && next < len(starts) {
			push(starts[next])
			next++
		}
		if len(stack) == 0 {
			return out
		}
		top := &stack[len(stack)-1]
		if top.primary < len(primary[top.node]) {
			top.primary++
			push(primary[top.node][top.primary-1])
			continue
		}
		if secondary != nil && top.secondary < len(secondary[top.node]) {
			top.secondary++
			push(secondary[top.node][top.secondary-1])
			continue
		}
		out = append(out, top.node)
		stack = stack[:len(stack)-1]
	}
}

type graphSearch func(g *Graph, starts []int, direction string, radius float64) []int

// search runs a walk of a `SearchConfig`, its options checked first.
func search(walk graphSearch, options, v Value) Value {
	return graphAttempt(func() Value {
		g, o := graphOf(v), record(options)
		radius := traversalRadius(o.Get("radius"), math.Inf(1))
		starts := g.starts(o.Get("start"))
		return g.walker(walk(g, starts, directionOf(o.Get("direction")), radius))
	})
}

func GraphDfs(options, v Value) Value          { return search((*Graph).dfs, options, v) }
func GraphBfs(options, v Value) Value          { return search((*Graph).bfsFrom, options, v) }
func GraphDfsPostOrder(options, v Value) Value { return search((*Graph).dfsPostOrder, options, v) }

// GraphTopo is a topological order: Kahn's, the initial nodes first.
func GraphTopo(options, v Value) Value {
	return graphAttempt(func() Value {
		g := graphOf(v)
		if g.kind == "undirected" {
			graphFail("Cannot perform topological sort on undirected graph")
		}
		if !g.isAcyclic() {
			graphFail("Cannot perform topological sort on cyclic graph")
		}
		initials := g.starts(record(options).Get("initials"))
		outgoing, incoming := g.outgoing(), g.incoming()
		inDegree := make([]int, len(g.nodes))
		for p := range g.nodes {
			inDegree[p] = len(incoming[p])
		}
		initial := make([]bool, len(g.nodes))
		queue := []int{}
		for _, p := range initials {
			if inDegree[p] != 0 {
				graphFail("Initial node " + strconv.Itoa(g.nodes[p].index) + " has incoming edges")
			}
			initial[p] = true
			queue = append(queue, p)
		}
		for p := range g.nodes {
			if inDegree[p] == 0 && !initial[p] {
				queue = append(queue, p)
			}
		}
		done := make([]bool, len(g.nodes))
		out := []int{}
		for head := 0; head < len(queue); head++ {
			current := queue[head]
			if done[current] {
				continue
			}
			done[current] = true
			for _, next := range outgoing[current] {
				if !done[next] {
					if inDegree[next]--; inDegree[next] == 0 {
						queue = append(queue, next)
					}
				}
			}
			out = append(out, current)
		}
		if len(out) < len(g.nodes) {
			graphFail("Cannot perform topological sort on cyclic graph")
		}
		return g.walker(out)
	})
}

func GraphNodes(v Value) Value {
	g := graphOf(v)
	positions := make([]int, len(g.nodes))
	for p := range positions {
		positions[p] = p
	}
	return g.walker(positions)
}

func GraphEdges(v Value) Value {
	g := graphOf(v)
	w := &graphWalker{make([]Value, len(g.edges)), make([]Value, len(g.edges))}
	for i, e := range g.edges {
		w.indices[i], w.values[i] = int32(e.index), edgeRecord(e)
	}
	return w
}

func edgeRecord(e graphEdge) Value {
	return Rec("source", int32(e.source), "target", int32(e.target), "data", e.data)
}

// GraphExternals is the nodes without an edge in the direction.
func GraphExternals(options, v Value) Value {
	g := graphOf(v)
	adjacency := g.adj
	if directionOf(record(options).Get("direction")) == "incoming" {
		adjacency = g.radj
	}
	out := []int{}
	for p, n := range g.nodes {
		if len(adjacency[n.index]) == 0 {
			out = append(out, p)
		}
	}
	return g.walker(out)
}
