package flickrt

import (
	"math"
	"strconv"
)

// Effect's Graph shortest paths: Dijkstra, A*, Bellman-Ford.

// minHeap is Effect's dense min-heap: ties pop in push order, and an
// indexed heap moves a node already in it instead of adding it twice.
type minHeap struct {
	nodes      []int
	priorities []float64
	sequences  []int
	positions  []int
}

func newMinHeap(n int, indexed bool) *minHeap {
	h := &minHeap{}
	if indexed {
		h.positions = make([]int, n)
		for i := range h.positions {
			h.positions[i] = -1
		}
	}
	return h
}

func (h *minHeap) before(p float64, s int, q float64, t int) bool { return p < q || p == q && s < t }

func (h *minHeap) set(i, node int, priority float64, sequence int) {
	h.nodes[i], h.priorities[i], h.sequences[i] = node, priority, sequence
	if h.positions != nil {
		h.positions[node] = i
	}
}

func (h *minHeap) push(node int, priority float64, sequence int) {
	i := -1
	if h.positions != nil {
		i = h.positions[node]
	}
	if i == -1 {
		i = len(h.nodes)
		h.nodes, h.priorities, h.sequences = append(h.nodes, 0), append(h.priorities, 0), append(h.sequences, 0)
	}
	for i > 0 {
		parent := (i - 1) / 2
		if !h.before(priority, sequence, h.priorities[parent], h.sequences[parent]) {
			break
		}
		h.set(i, h.nodes[parent], h.priorities[parent], h.sequences[parent])
		i = parent
	}
	h.set(i, node, priority, sequence)
}

func (h *minHeap) pop() (int, float64) {
	node, priority := h.nodes[0], h.priorities[0]
	if h.positions != nil {
		h.positions[node] = -1
	}
	last := len(h.nodes) - 1
	n, p, s := h.nodes[last], h.priorities[last], h.sequences[last]
	h.nodes, h.priorities, h.sequences = h.nodes[:last], h.priorities[:last], h.sequences[:last]
	if last == 0 {
		return node, priority
	}
	i := 0
	for {
		child := i*2 + 1
		if child >= last {
			break
		}
		if right := child + 1; right < last && h.before(h.priorities[right], h.sequences[right], h.priorities[child], h.sequences[child]) {
			child = right
		}
		if !h.before(h.priorities[child], h.sequences[child], p, s) {
			break
		}
		h.set(i, h.nodes[child], h.priorities[child], h.sequences[child])
		i = child
	}
	h.set(i, n, p, s)
	return node, priority
}

// weights is the cost of each edge, checked by `bad`, which names it.
func (g *Graph) weights(fb *Fiber, cost Value, bad func(w float64) bool, message string) []float64 {
	out := make([]float64, len(g.edges))
	for q, e := range g.edges {
		w := Apply(fb, cost, e.data).(float64)
		if bad(w) {
			graphFail(message)
		}
		out[q] = w
	}
	return out
}

func negativeOrNaN(w float64) bool  { return math.IsNaN(w) || w < 0 }
func nanOrMinusInf(w float64) bool  { return math.IsNaN(w) || math.IsInf(w, -1) }
func finite(x float64) bool         { return !math.IsNaN(x) && !math.IsInf(x, 0) }
func overflows(w, sum float64) bool { return !math.IsInf(w, 1) && !finite(sum) }

func pathResult(path, edges []int, distance float64, costs []Value) Value {
	return Rec("path", indexList(path), "edges", indexList(edges), "distance", distance, "costs", costs)
}

// tracePath is the path to `target` by the previous node and edge of each
// position.
func (g *Graph) tracePath(target int, previousNode, previousEdge []int, distance float64) Value {
	path, edges, costs := []int{}, []int{}, []Value{}
	for current := target; current != -1; current = previousNode[current] {
		path = append(path, g.nodes[current].index)
		if e := previousEdge[current]; e != -1 {
			edges, costs = append(edges, g.edges[e].index), append(costs, g.edges[e].data)
		}
	}
	reverseInts(path)
	reverseInts(edges)
	for i, j := 0, len(costs)-1; i < j; i, j = i+1, j-1 {
		costs[i], costs[j] = costs[j], costs[i]
	}
	return Some(pathResult(path, edges, distance, costs))
}

// endpointsOf is the positions of a config's source and target, which must
// exist.
func (g *Graph) endpointsOf(c *Record) (int, int) {
	s, t := g.existing(c.Get("source")), g.existing(c.Get("target"))
	return g.nodeAt(s), g.nodeAt(t)
}

func filled(n int, x int) []int {
	out := make([]int, n)
	for i := range out {
		out[i] = x
	}
	return out
}

func infinities(n int) []float64 {
	out := make([]float64, n)
	for i := range out {
		out[i] = math.Inf(1)
	}
	return out
}

func GraphDijkstra(fb *Fiber, config, v Value) Value {
	return graphAttempt(func() Value {
		g, c := graphOf(v), record(config)
		source, target := g.endpointsOf(c)
		outgoing, edgeRows := g.rows(false)
		weights := g.weights(fb, c.Get("cost"), negativeOrNaN, "Dijkstra's algorithm requires non-negative edge weights")
		if source == target {
			return Some(pathResult([]int{g.nodes[source].index}, []int{}, 0, []Value{}))
		}
		n := len(g.nodes)
		distances, previousNode, previousEdge, visited := infinities(n), filled(n, -1), filled(n, -1), make([]bool, n)
		distances[source] = 0
		heap, sequence := newMinHeap(n, true), 0
		heap.push(source, 0, sequence)
		sequence++
		for len(heap.nodes) > 0 {
			current, distance := heap.pop()
			if visited[current] {
				continue
			}
			visited[current] = true
			if current == target {
				break
			}
			for k, next := range outgoing[current] {
				e := edgeRows[current][k]
				d := distance + weights[e]
				if overflows(weights[e], d) {
					graphFail("Dijkstra distance calculation exceeded the finite number range")
				}
				if d < distances[next] {
					distances[next], previousNode[next], previousEdge[next] = d, current, e
					if !visited[next] {
						heap.push(next, d, sequence)
						sequence++
					}
				}
			}
		}
		if math.IsInf(distances[target], 1) {
			return None
		}
		return g.tracePath(target, previousNode, previousEdge, distances[target])
	})
}

// GraphAstar is Effect's A*; the heuristic is curried, as Flick passes it.
func GraphAstar(fb *Fiber, config, v Value) Value {
	return graphAttempt(func() Value {
		g, c := graphOf(v), record(config)
		source, target := g.endpointsOf(c)
		outgoing, edgeRows := g.rows(false)
		weights := g.weights(fb, c.Get("cost"), negativeOrNaN, "A* algorithm requires non-negative edge weights")
		heuristic := func(data Value) float64 {
			h := apply2(fb, c.Get("heuristic"), data, g.nodes[target].data).(float64)
			if !finite(h) {
				graphFail("A* algorithm requires finite heuristic values")
			}
			return h
		}
		if source == target {
			heuristic(g.nodes[source].data)
			return Some(pathResult([]int{g.nodes[source].index}, []int{}, 0, []Value{}))
		}
		n := len(g.nodes)
		scores, previousNode, previousEdge, visited := infinities(n), filled(n, -1), filled(n, -1), make([]bool, n)
		scores[source] = 0
		open, sequence := newMinHeap(n, false), 0
		open.push(source, heuristic(g.nodes[source].data), sequence)
		sequence++
		for len(open.nodes) > 0 {
			current, _ := open.pop()
			if visited[current] {
				continue
			}
			visited[current] = true
			if current == target {
				break
			}
			for k, next := range outgoing[current] {
				if visited[next] {
					continue
				}
				e := edgeRows[current][k]
				score := scores[current] + weights[e]
				if overflows(weights[e], score) {
					graphFail("A* distance calculation exceeded the finite number range")
				}
				if score < scores[next] {
					scores[next], previousNode[next], previousEdge[next] = score, current, e
					priority := score + heuristic(g.nodes[next].data)
					if !finite(priority) {
						graphFail("A* priority calculation exceeded the finite number range")
					}
					open.push(next, priority, sequence)
					sequence++
				}
			}
		}
		if math.IsInf(scores[target], 1) {
			return None
		}
		return g.tracePath(target, previousNode, previousEdge, scores[target])
	})
}

func GraphBellmanFord(fb *Fiber, config, v Value) Value {
	return graphAttempt(func() Value {
		g, c := graphOf(v), record(config)
		source, target := g.endpointsOf(c)
		sources, targets := g.endpoints()
		outgoing, edgeRows := g.rows(false)
		weights := g.weights(fb, c.Get("cost"), nanOrMinusInf, "Bellman-Ford algorithm does not support NaN or -Infinity edge weights")
		add := func(d, w float64) float64 {
			if math.IsInf(d, 1) || math.IsInf(w, 1) {
				return math.Inf(1)
			}
			if !finite(d + w) {
				graphFail("Bellman-Ford distance calculation exceeded the finite number range")
			}
			return d + w
		}
		n := len(g.nodes)
		distances, previousNode, previousEdge := infinities(n), filled(n, -1), filled(n, -1)
		distances[source] = 0
		undirected := g.kind == "undirected"
		for iteration := 0; iteration < n-1; iteration++ {
			updated := false
			for q := range g.edges {
				s, t := sources[q], targets[q]
				if d := add(distances[s], weights[q]); d < distances[t] {
					distances[t], previousNode[t], previousEdge[t], updated = d, s, q, true
				}
				if undirected && s != t {
					if d := add(distances[t], weights[q]); d < distances[s] {
						distances[s], previousNode[s], previousEdge[s], updated = d, t, q, true
					}
				}
			}
			if !updated {
				break
			}
		}
		affected := make([]bool, n)
		queue := []int{}
		mark := func(p int) {
			if !affected[p] {
				affected[p] = true
				queue = append(queue, p)
			}
		}
		for q := range g.edges {
			s, t := sources[q], targets[q]
			if add(distances[s], weights[q]) < distances[t] {
				mark(t)
			}
			if undirected && s != t && add(distances[t], weights[q]) < distances[s] {
				mark(s)
			}
		}
		for head := 0; head < len(queue); head++ {
			for k, next := range outgoing[queue[head]] {
				if !math.IsInf(weights[edgeRows[queue[head]][k]], 1) {
					mark(next)
				}
			}
		}
		cycle := "Negative cycle affects path to node " + strconv.Itoa(g.nodes[target].index)
		if affected[target] {
			graphFail(cycle)
		}
		if math.IsInf(distances[target], 1) {
			return None
		}
		steps := 0
		for current := target; current != -1; current = previousNode[current] {
			if steps++; steps > n {
				graphFail(cycle)
			}
		}
		return g.tracePath(target, previousNode, previousEdge, distances[target])
	})
}
