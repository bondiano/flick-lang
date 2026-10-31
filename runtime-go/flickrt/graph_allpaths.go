package flickrt

import (
	"math"
	"strconv"
)

// Effect's Graph all-pairs shortest paths and path enumerations.

// witness is how Floyd-Warshall reached a target: by one edge, or by two
// halves through another node.
type witness struct {
	target, edge int
	left, right  *witness
}

func GraphFloydWarshall(fb *Fiber, cost, v Value) Value {
	return graphAttempt(func() Value {
		g := graphOf(v)
		n := len(g.nodes)
		sources, targets := g.endpoints()
		witnesses := make([]*witness, n*n)
		distances := make([]float64, n*n)
		for i := range distances {
			distances[i] = math.Inf(1)
		}
		for i := 0; i < n; i++ {
			distances[i*n+i] = 0
		}
		weights := g.weights(fb, cost, nanOrMinusInf, "Floyd-Warshall algorithm does not support NaN or -Infinity edge weights")
		for q, w := range weights {
			s, t := sources[q], targets[q]
			if w < distances[s*n+t] {
				distances[s*n+t], witnesses[s*n+t] = w, &witness{target: t, edge: q}
			}
			if g.kind == "undirected" && w < distances[t*n+s] {
				distances[t*n+s], witnesses[t*n+s] = w, &witness{target: s, edge: q}
			}
		}
		for k := 0; k < n; k++ {
			for i := 0; i < n; i++ {
				ik := distances[i*n+k]
				if math.IsInf(ik, 1) {
					continue
				}
				left := witnesses[i*n+k]
				for j := 0; j < n; j++ {
					kj := distances[k*n+j]
					if math.IsInf(kj, 1) {
						continue
					}
					candidate := ik + kj
					if !finite(candidate) {
						graphFail("Floyd-Warshall distance calculation exceeded the finite number range")
					}
					if candidate < distances[i*n+j] {
						distances[i*n+j] = candidate
						right := witnesses[k*n+j]
						switch {
						case left == nil:
							witnesses[i*n+j] = right
						case right == nil:
							witnesses[i*n+j] = left
						default:
							witnesses[i*n+j] = &witness{target: j, edge: -1, left: left, right: right}
						}
					}
				}
			}
		}
		for i := 0; i < n; i++ {
			if distances[i*n+i] < 0 {
				graphFail("Negative cycle detected involving node " + strconv.Itoa(g.nodes[i].index))
			}
		}
		distanceRows, pathRows, edgeRows, costRows := make([]Value, n), make([]Value, n), make([]Value, n), make([]Value, n)
		positions := filled(n, -1)
		for i := 0; i < n; i++ {
			source := int32(g.nodes[i].index)
			dRow, pRow, eRow, cRow := make([]Value, n), make([]Value, n), make([]Value, n), make([]Value, n)
			for j := 0; j < n; j++ {
				target := int32(g.nodes[j].index)
				d := distances[i*n+j]
				dRow[j] = []Value{target, d}
				switch {
				case i == j:
					pRow[j], eRow[j], cRow[j] = []Value{target, Some([]Value{source})}, []Value{target, []Value{}}, []Value{target, []Value{}}
				case math.IsInf(d, 1):
					pRow[j], eRow[j], cRow[j] = []Value{target, None}, []Value{target, []Value{}}, []Value{target, []Value{}}
				default:
					path, pathEdges, pathCosts := []int{int(source)}, []int{}, []Value{}
					positions[i] = 0
					nodes := []int{i}
					for stack := []*witness{witnesses[i*n+j]}; len(stack) > 0; {
						w := stack[len(stack)-1]
						stack = stack[:len(stack)-1]
						switch at := positions[w.target]; {
						case at != -1:
							for p := at + 1; p < len(nodes); p++ {
								positions[nodes[p]] = -1
							}
							nodes, path = nodes[:at+1], path[:at+1]
							pathEdges, pathCosts = pathEdges[:at], pathCosts[:at]
						case w.edge != -1:
							positions[w.target] = len(nodes)
							nodes = append(nodes, w.target)
							path = append(path, g.nodes[w.target].index)
							pathEdges = append(pathEdges, g.edges[w.edge].index)
							pathCosts = append(pathCosts, g.edges[w.edge].data)
						default:
							stack = append(stack, w.right, w.left)
						}
					}
					for _, p := range nodes {
						positions[p] = -1
					}
					pRow[j] = []Value{target, Some(indexList(path))}
					eRow[j], cRow[j] = []Value{target, indexList(pathEdges)}, []Value{target, pathCosts}
				}
			}
			distanceRows[i], pathRows[i] = []Value{source, dRow}, []Value{source, pRow}
			edgeRows[i], costRows[i] = []Value{source, eRow}, []Value{source, cRow}
		}
		return Rec("distances", distanceRows, "paths", pathRows, "edges", edgeRows, "costs", costRows)
	})
}

// enumerationLimit is Effect's `pathEnumerationLimit`: none is no limit.
func enumerationLimit(o Value) int {
	if l, ok := some(o); ok {
		if l.(int32) < 0 {
			graphFail("Path enumeration limit must be a non-negative integer or Infinity")
		}
		return int(l.(int32))
	}
	return math.MaxInt
}

// GraphSimplePaths is Effect's depth-first enumeration of the paths that
// repeat no node, each edge counting 1 toward the distance.
func GraphSimplePaths(options, v Value) Value {
	return graphAttempt(func() Value {
		g, o := graphOf(v), record(options)
		source, target := g.endpointsOf(o)
		limit := enumerationLimit(o.Get("limit"))
		out := []Value{}
		if limit == 0 {
			return out
		}
		outgoing, edgeRows := g.rows(false)
		path, pathEdges, costs := []int{g.nodes[source].index}, []int{}, []Value{}
		visited := make([]bool, len(g.nodes))
		visited[source] = true
		type frame struct{ node, position int }
		stack := []frame{{source, 0}}
		backtrack := func() {
			top := stack[len(stack)-1]
			stack = stack[:len(stack)-1]
			if len(stack) > 0 {
				visited[top.node] = false
				path, pathEdges, costs = path[:len(path)-1], pathEdges[:len(pathEdges)-1], costs[:len(costs)-1]
			}
		}
		for len(stack) > 0 && len(out) < limit {
			top := &stack[len(stack)-1]
			if top.node == target {
				out = append(out, pathResult(append([]int{}, path...), append([]int{}, pathEdges...),
					float64(len(pathEdges)), append([]Value{}, costs...)))
				backtrack()
				continue
			}
			if top.position >= len(outgoing[top.node]) {
				backtrack()
				continue
			}
			k := top.position
			top.position++
			next := outgoing[top.node][k]
			if visited[next] {
				continue
			}
			e := edgeRows[top.node][k]
			visited[next] = true
			path, pathEdges = append(path, g.nodes[next].index), append(pathEdges, g.edges[e].index)
			costs = append(costs, g.edges[e].data)
			stack = append(stack, frame{next, 0})
		}
		return out
	})
}

// GraphAllShortestPaths is Effect's `allShortestPaths`: Dijkstra keeping
// every predecessor at the shortest distance, then a walk back from the
// target.
func GraphAllShortestPaths(fb *Fiber, options, v Value) Value {
	return graphAttempt(func() Value {
		g, o := graphOf(v), record(options)
		source, target := g.endpointsOf(o)
		limit := enumerationLimit(o.Get("limit"))
		outgoing, edgeRows := g.rows(false)
		weights := g.weights(fb, o.Get("cost"), negativeOrNaN, "All shortest paths requires non-negative edge weights")
		out := []Value{}
		if limit == 0 {
			return out
		}
		n := len(g.nodes)
		distances := infinities(n)
		distances[source] = 0
		type step struct{ node, edge int }
		previous := make([][]step, n)
		queue, sequence := newMinHeap(n, false), 0
		queue.push(source, 0, sequence)
		sequence++
		for len(queue.nodes) > 0 {
			current, distance := queue.pop()
			if distance != distances[current] {
				continue
			}
			for k, next := range outgoing[current] {
				e := edgeRows[current][k]
				d := distance + weights[e]
				if overflows(weights[e], d) {
					graphFail("All shortest paths distance calculation exceeded the finite number range")
				}
				if d < distances[next] {
					distances[next], previous[next] = d, []step{{current, e}}
					queue.push(next, d, sequence)
					sequence++
				} else if d == distances[next] && !math.IsInf(d, 1) {
					previous[next] = append(previous[next], step{current, e})
				}
			}
		}
		distance := distances[target]
		if math.IsInf(distance, 1) {
			return out
		}
		if source == target {
			return append(out, pathResult([]int{g.nodes[source].index}, []int{}, 0, []Value{}))
		}
		reversePath, reverseEdges := []int{target}, []int{}
		visited := make([]bool, n)
		visited[target] = true
		type frame struct{ node, position int }
		stack := []frame{{target, 0}}
		backtrack := func() {
			top := stack[len(stack)-1]
			stack = stack[:len(stack)-1]
			if len(stack) > 0 {
				visited[top.node] = false
				reversePath, reverseEdges = reversePath[:len(reversePath)-1], reverseEdges[:len(reverseEdges)-1]
			}
		}
		for len(stack) > 0 && len(out) < limit {
			top := &stack[len(stack)-1]
			if top.node == source {
				path, edges, costs := make([]int, len(reversePath)), make([]int, len(reverseEdges)), make([]Value, len(reverseEdges))
				for i, p := range reversePath {
					path[len(path)-1-i] = g.nodes[p].index
				}
				for i, e := range reverseEdges {
					edges[len(edges)-1-i], costs[len(costs)-1-i] = g.edges[e].index, g.edges[e].data
				}
				out = append(out, pathResult(path, edges, distance, costs))
				backtrack()
				continue
			}
			if top.position >= len(previous[top.node]) {
				backtrack()
				continue
			}
			p := previous[top.node][top.position]
			top.position++
			if visited[p.node] {
				continue
			}
			visited[p.node] = true
			reversePath, reverseEdges = append(reversePath, p.node), append(reverseEdges, p.edge)
			stack = append(stack, frame{p.node, 0})
		}
		return out
	})
}
