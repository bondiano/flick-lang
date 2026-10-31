package flickrt

import (
	"math"
	"sort"
	"strconv"
)

// Effect's Graph flows, cuts, spanning forests and transitive reduction.

type flowSolution struct {
	value      float64
	flows, cut Value
	sourceSide []bool
}

// solveFlow is Effect's `solveMaximumFlow`: Edmonds-Karp over residual
// arcs, a self-loop carrying none.
func (g *Graph) solveFlow(fb *Fiber, config Value) flowSolution {
	if g.kind == "undirected" {
		graphFail("Cannot compute flow of undirected graph")
	}
	c := record(config)
	source, target := g.nodeAt(index(c.Get("source"))), g.nodeAt(index(c.Get("target")))
	if source < 0 {
		missingNode(index(c.Get("source")))
	}
	if target < 0 {
		missingNode(index(c.Get("target")))
	}
	if source == target {
		graphFail("Flow source and target must be different nodes")
	}
	type arc struct {
		from, to int
		capacity float64
		flow     float64
	}
	sources, targets := g.endpoints()
	arcs := []arc{}
	adjacency := make([][]int, len(g.nodes))
	forward := make([]int, len(g.edges))
	for q, e := range g.edges {
		capacity := Apply(fb, c.Get("capacity"), e.data).(float64)
		if math.IsNaN(capacity) || math.IsInf(capacity, 0) || capacity < 0 {
			graphFail("Edge " + strconv.Itoa(e.index) + " capacity must be a finite non-negative number")
		}
		forward[q] = -1
		from, to := sources[q], targets[q]
		if from == to {
			continue
		}
		forward[q] = len(arcs)
		adjacency[from] = append(adjacency[from], len(arcs))
		arcs = append(arcs, arc{from, to, capacity, 0})
		adjacency[to] = append(adjacency[to], len(arcs))
		arcs = append(arcs, arc{to, from, 0, 0})
	}
	residual := func(stopAt int) ([]bool, []int) {
		visited, parent := make([]bool, len(g.nodes)), make([]int, len(g.nodes))
		visited[source] = true
		for queue := []int{source}; len(queue) > 0 && (stopAt < 0 || !visited[stopAt]); queue = queue[1:] {
			for _, a := range adjacency[queue[0]] {
				if arcs[a].capacity-arcs[a].flow > 0 && !visited[arcs[a].to] {
					visited[arcs[a].to], parent[arcs[a].to] = true, a
					queue = append(queue, arcs[a].to)
					if arcs[a].to == stopAt {
						break
					}
				}
			}
		}
		return visited, parent
	}
	value := 0.0
	for {
		visited, parent := residual(target)
		if !visited[target] {
			break
		}
		amount := math.Inf(1)
		for node := target; node != source; node = arcs[parent[node]].from {
			amount = math.Min(amount, arcs[parent[node]].capacity-arcs[parent[node]].flow)
		}
		if math.IsInf(value+amount, 0) || math.IsNaN(value+amount) {
			graphFail("Maximum flow exceeds the finite number range")
		}
		for node := target; node != source; node = arcs[parent[node]].from {
			arcs[parent[node]].flow += amount
			arcs[parent[node]^1].flow -= amount
		}
		value += amount
	}
	flows := make([]Value, len(g.edges))
	for q, e := range g.edges {
		flow := 0.0
		if forward[q] != -1 {
			flow = arcs[forward[q]].flow
		}
		flows[q] = []Value{int32(e.index), flow}
	}
	sourceSide, _ := residual(-1)
	cut := []int{}
	for q, e := range g.edges {
		if sources[q] != targets[q] && sourceSide[sources[q]] && !sourceSide[targets[q]] {
			cut = append(cut, e.index)
		}
	}
	return flowSolution{value, flows, indexList(cut), sourceSide}
}

func GraphMaximumFlow(fb *Fiber, config, v Value) Value {
	return graphAttempt(func() Value {
		s := graphOf(v).solveFlow(fb, config)
		return Rec("value", s.value, "flows", s.flows, "cut", s.cut)
	})
}

func GraphMinimumCut(fb *Fiber, config, v Value) Value {
	return graphAttempt(func() Value {
		g := graphOf(v)
		s := g.solveFlow(fb, config)
		source, target := []Value{}, []Value{}
		for p, n := range g.nodes {
			if s.sourceSide[p] {
				source = append(source, int32(n.index))
			} else {
				target = append(target, int32(n.index))
			}
		}
		return Rec("value", s.value, "edges", s.cut, "source", source, "target", target)
	})
}

// GraphMinimumSpanningForest is Effect's Kruskal: edges by weight, ties in
// edge order, an infinite weight never taken.
func GraphMinimumSpanningForest(fb *Fiber, cost, v Value) Value {
	return graphAttempt(func() Value {
		g := graphOf(v)
		if g.kind == "directed" {
			graphFail("Cannot find minimum spanning forest of directed graph")
		}
		type weighted struct {
			edge   int
			weight float64
		}
		edges := []weighted{}
		for q, e := range g.edges {
			w := Apply(fb, cost, e.data).(float64)
			if math.IsNaN(w) || math.IsInf(w, -1) {
				graphFail("Minimum spanning forest does not support NaN or -Infinity edge weights")
			}
			if !math.IsInf(w, 1) {
				edges = append(edges, weighted{q, w})
			}
		}
		sort.SliceStable(edges, func(i, j int) bool { return edges[i].weight < edges[j].weight })
		parents, ranks := make([]int, len(g.nodes)), make([]int, len(g.nodes))
		for p := range parents {
			parents[p] = p
		}
		find := func(node int) int {
			root := node
			for parents[root] != root {
				root = parents[root]
			}
			for parents[node] != node {
				parents[node], node = root, parents[node]
			}
			return root
		}
		selected := make([]bool, len(g.edges))
		for _, w := range edges {
			e := g.edges[w.edge]
			a, b := find(g.nodeAt(e.source)), find(g.nodeAt(e.target))
			if a == b {
				continue
			}
			selected[w.edge] = true
			if ranks[a] < ranks[b] {
				a, b = b, a
			}
			parents[b] = a
			if ranks[a] == ranks[b] {
				ranks[a]++
			}
		}
		return g.keepEdges(selected)
	})
}

// keepEdges is the graph of all the nodes and the selected edges, as
// Effect's `fromSnapshot` of them.
func (g *Graph) keepEdges(selected []bool) *Graph {
	out := newGraph(g.kind)
	out.nodes = append([]graphNode{}, g.nodes...)
	for q, e := range g.edges {
		if selected[q] {
			out.edges = append(out.edges, e)
		}
	}
	return out.hydrate()
}

// GraphTransitiveReduction keeps the first edge from each node to each
// other that no longer path joins, as Effect's `transitiveReduction`.
func GraphTransitiveReduction(v Value) Value {
	return graphAttempt(func() Value {
		g := graphOf(v)
		if g.kind == "undirected" {
			graphFail("Cannot transitively reduce undirected graph")
		}
		if !g.isAcyclic() {
			graphFail("Cannot transitively reduce cyclic graph")
		}
		type pair struct{ source, target int }
		first := map[pair]bool{}
		retained := make([]bool, len(g.edges))
		for q, e := range g.edges {
			key := pair{e.source, e.target}
			if first[key] {
				continue
			}
			first[key] = true
			visited := map[int]bool{e.source: true}
			reachable := false
			for queue := []int{e.source}; len(queue) > 0 && !reachable; queue = queue[1:] {
				current := queue[0]
				for _, ci := range g.adj[current] {
					candidate := g.edge(ci)
					if current == e.source && candidate.target == e.target {
						continue
					}
					if candidate.target == e.target {
						reachable = true
						break
					}
					if !visited[candidate.target] {
						visited[candidate.target] = true
						queue = append(queue, candidate.target)
					}
				}
			}
			retained[q] = !reachable
		}
		return g.keepEdges(retained)
	})
}
