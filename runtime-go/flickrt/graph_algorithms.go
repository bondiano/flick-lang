package flickrt

import "sort"

// Graph algorithms on cycles, bipartiteness, reachability and
// connectivity, over the compact view.

func GraphFindCycle(v Value) Value {
	g := graphOf(v)
	colors := map[int]int{}
	parentNode, parentEdge := map[int]int{}, map[int]int{}
	type frame struct{ node, position int }
	for _, start := range g.nodes {
		if colors[start.index] != 0 {
			continue
		}
		colors[start.index] = 1
		stack := []frame{{start.index, 0}}
		for len(stack) > 0 {
			top := &stack[len(stack)-1]
			adjacency := g.adj[top.node]
			if top.position >= len(adjacency) {
				colors[top.node] = 2
				stack = stack[:len(stack)-1]
				continue
			}
			ei := adjacency[top.position]
			top.position++
			if pe, ok := parentEdge[top.node]; g.kind == "undirected" && ok && pe == ei {
				continue
			}
			node := top.node
			next := g.neighbor(node, g.edge(ei))
			switch colors[next] {
			case 1:
				path, edges := []int{node}, []int{}
				for cursor := node; cursor != next; {
					edges = append(edges, parentEdge[cursor])
					cursor = parentNode[cursor]
					path = append(path, cursor)
				}
				reverseInts(path)
				reverseInts(edges)
				return Some(Rec("path", indexList(append(path, next)), "edges", indexList(append(edges, ei))))
			case 0:
				colors[next] = 1
				parentNode[next], parentEdge[next] = node, ei
				stack = append(stack, frame{next, 0})
			}
		}
	}
	return None
}

func reverseInts(xs []int) {
	for i, j := 0, len(xs)-1; i < j; i, j = i+1, j-1 {
		xs[i], xs[j] = xs[j], xs[i]
	}
}

func GraphIsAcyclic(v Value) Value { return graphOf(v).isAcyclic() }

func (g *Graph) isAcyclic() bool {
	outgoing, edges := g.rows(false)
	if g.kind == "undirected" {
		visited := make([]bool, len(g.nodes))
		for start := range g.nodes {
			if visited[start] {
				continue
			}
			visited[start] = true
			stack, parents := []int{start}, []int{-1}
			for len(stack) > 0 {
				node, parent := stack[len(stack)-1], parents[len(parents)-1]
				stack, parents = stack[:len(stack)-1], parents[:len(parents)-1]
				for k, next := range outgoing[node] {
					if edges[node][k] == parent {
						continue
					}
					if visited[next] {
						return false
					}
					visited[next] = true
					stack, parents = append(stack, next), append(parents, edges[node][k])
				}
			}
		}
		return true
	}
	colors := make([]int, len(g.nodes))
	for start := range g.nodes {
		if colors[start] != 0 {
			continue
		}
		colors[start] = 1
		stack, positions := []int{start}, []int{0}
		for len(stack) > 0 {
			top := len(stack) - 1
			node := stack[top]
			if positions[top] < len(outgoing[node]) {
				next := outgoing[node][positions[top]]
				positions[top]++
				if colors[next] == 1 {
					return false
				}
				if colors[next] == 0 {
					colors[next] = 1
					stack, positions = append(stack, next), append(positions, 0)
				}
			} else {
				colors[node] = 2
				stack, positions = stack[:top], positions[:top]
			}
		}
	}
	return true
}

// twoColors is a breadth-first two-coloring, nil when an edge joins two
// nodes of a color.
func (g *Graph) twoColors() []int {
	outgoing := g.outgoing()
	colors := make([]int, len(g.nodes))
	for p := range colors {
		colors[p] = -1
	}
	for start := range g.nodes {
		if colors[start] != -1 {
			continue
		}
		colors[start] = 0
		for queue := []int{start}; len(queue) > 0; queue = queue[1:] {
			current := queue[0]
			for _, next := range outgoing[current] {
				if colors[next] == -1 {
					colors[next] = 1 - colors[current]
					queue = append(queue, next)
				} else if colors[next] == colors[current] {
					return nil
				}
			}
		}
	}
	return colors
}

func GraphIsBipartite(v Value) Value {
	g := graphOf(v)
	if g.kind == "directed" {
		panic(Defect{"GraphError: Cannot determine bipartite status of directed graph"})
	}
	return g.twoColors() != nil
}

// GraphMaximumBipartiteMatching is a Hopcroft-Karp matching, the left side
// the nodes of the first color.
func GraphMaximumBipartiteMatching(v Value) Value {
	return graphAttempt(func() Value {
		g := graphOf(v)
		if g.kind == "directed" {
			graphFail("Cannot find bipartite matching of directed graph")
		}
		colors := g.twoColors()
		if colors == nil {
			graphFail("Cannot find bipartite matching of non-bipartite graph")
		}
		n := len(g.nodes)
		sources, targets := g.endpoints()
		type arc struct{ right, edge int }
		adjacency := make([][]arc, n)
		seen := make([]map[int]bool, n)
		for q := range g.edges {
			left, right := sources[q], targets[q]
			if colors[left] != 0 {
				left, right = right, left
			}
			if seen[left] == nil {
				seen[left] = map[int]bool{}
			}
			if !seen[left][right] {
				seen[left][right] = true
				adjacency[left] = append(adjacency[left], arc{right, q})
			}
		}
		const unmatched, infinity = -1, 0x7fffffff
		matchLeft, matchRight, matchEdge, distance := make([]int, n), make([]int, n), make([]int, n), make([]int, n)
		for p := 0; p < n; p++ {
			matchLeft[p], matchRight[p], matchEdge[p] = unmatched, unmatched, unmatched
		}
		shortest := infinity
		hasLayer := func() bool {
			queue := []int{}
			shortest = infinity
			for left := 0; left < n; left++ {
				if colors[left] != 0 {
					continue
				}
				if matchLeft[left] == unmatched {
					distance[left] = 0
					queue = append(queue, left)
				} else {
					distance[left] = infinity
				}
			}
			for head := 0; head < len(queue); head++ {
				left := queue[head]
				if distance[left] >= shortest {
					continue
				}
				for _, a := range adjacency[left] {
					next := matchRight[a.right]
					if next == unmatched {
						shortest = distance[left] + 1
					} else if distance[next] == infinity {
						distance[next] = distance[left] + 1
						queue = append(queue, next)
					}
				}
			}
			return shortest != infinity
		}
		type frame struct{ left, position, viaRight, viaEdge int }
		augment := func(start int) {
			stack := []frame{{start, 0, unmatched, unmatched}}
			for len(stack) > 0 {
				top := &stack[len(stack)-1]
				arcs := adjacency[top.left]
				if top.position >= len(arcs) {
					distance[top.left] = infinity
					stack = stack[:len(stack)-1]
					continue
				}
				a := arcs[top.position]
				top.position++
				next := matchRight[a.right]
				if next == unmatched && distance[top.left]+1 == shortest {
					matchLeft[top.left], matchRight[a.right], matchEdge[top.left] = a.right, top.left, a.edge
					for i := len(stack) - 1; i > 0; i-- {
						child, parent := stack[i], stack[i-1]
						matchLeft[parent.left], matchRight[child.viaRight], matchEdge[parent.left] =
							child.viaRight, parent.left, child.viaEdge
					}
					return
				}
				if next != unmatched && distance[next] == distance[top.left]+1 {
					stack = append(stack, frame{next, 0, a.right, a.edge})
				}
			}
		}
		for hasLayer() {
			for left := 0; left < n; left++ {
				if colors[left] == 0 && matchLeft[left] == unmatched {
					augment(left)
				}
			}
		}
		out := []Value{}
		for left := 0; left < n; left++ {
			if matchLeft[left] != unmatched {
				out = append(out, Rec("left", int32(g.nodes[left].index), "right", int32(g.nodes[matchLeft[left]].index),
					"edge", int32(g.edges[matchEdge[left]].index)))
			}
		}
		return out
	})
}

// reach is a breadth-first walk from position `source` over the rows of
// a direction (an undirected graph ignores it): the distance of each
// position, -1 when unreached.
func (g *Graph) reach(source int, direction string) []int {
	if g.kind == "undirected" {
		direction = "outgoing"
	}
	primary, secondary := g.adjacencies(direction)
	distances := make([]int, len(g.nodes))
	for p := range distances {
		distances[p] = -1
	}
	distances[source] = 0
	for queue := []int{source}; len(queue) > 0; queue = queue[1:] {
		current := queue[0]
		for _, rows := range [][][]int{primary, secondary} {
			if rows == nil {
				continue
			}
			for _, next := range rows[current] {
				if distances[next] == -1 {
					distances[next] = distances[current] + 1
					queue = append(queue, next)
				}
			}
		}
	}
	return distances
}

func GraphUnweightedDistances(source, options, v Value) Value {
	return graphAttempt(func() Value {
		g := graphOf(v)
		s := g.nodeAt(g.existing(source))
		out := []Value{}
		for p, d := range g.reach(s, directionOf(record(options).Get("direction"))) {
			if d != -1 {
				out = append(out, []Value{int32(g.nodes[p].index), int32(d)})
			}
		}
		return out
	})
}

func GraphHasPath(source, target, options, v Value) Value {
	return graphAttempt(func() Value {
		g := graphOf(v)
		s, t := g.existing(source), g.existing(target)
		if s == t {
			return true
		}
		return g.reach(g.nodeAt(s), directionOf(record(options).Get("direction")))[g.nodeAt(t)] != -1
	})
}

func (g *Graph) undirectedOrDefect(message string) {
	if g.kind == "directed" {
		panic(Defect{"GraphError: " + message})
	}
}

// components is the depth-first components over the rows.
func (g *Graph) components(primary, secondary [][]int) Value {
	visited := make([]bool, len(g.nodes))
	out := []Value{}
	for start := range g.nodes {
		if visited[start] {
			continue
		}
		component := []int{}
		for stack := []int{start}; len(stack) > 0; {
			current := stack[len(stack)-1]
			stack = stack[:len(stack)-1]
			if visited[current] {
				continue
			}
			visited[current] = true
			component = append(component, current)
			for _, rows := range [][][]int{primary, secondary} {
				if rows == nil {
					continue
				}
				for _, next := range rows[current] {
					if !visited[next] {
						stack = append(stack, next)
					}
				}
			}
		}
		out = append(out, g.nodeIds(component))
	}
	return out
}

// GraphConnectedComponents pushes each neighbor of a visit once.
func GraphConnectedComponents(v Value) Value {
	g := graphOf(v)
	g.undirectedOrDefect("Cannot find connected components of directed graph")
	outgoing := g.outgoing()
	unique := make([][]int, len(outgoing))
	for p, row := range outgoing {
		seen := map[int]bool{}
		for _, next := range row {
			if !seen[next] {
				seen[next] = true
				unique[p] = append(unique[p], next)
			}
		}
	}
	return g.components(unique, nil)
}

func GraphWeaklyConnectedComponents(v Value) Value {
	g := graphOf(v)
	g.directedOrDefect("Cannot find weakly connected components of undirected graph")
	return g.components(g.adjacencies("undirected"))
}

func GraphStronglyConnectedComponents(v Value) Value {
	g := graphOf(v)
	g.directedOrDefect("Cannot find strongly connected components of undirected graph")
	outgoing, incoming := g.outgoing(), g.incoming()
	visited := make([]bool, len(g.nodes))
	finish := []int{}
	for start := range g.nodes {
		if visited[start] {
			continue
		}
		visited[start] = true
		stack, positions := []int{start}, []int{0}
		for len(stack) > 0 {
			top := len(stack) - 1
			node := stack[top]
			if positions[top] < len(outgoing[node]) {
				next := outgoing[node][positions[top]]
				positions[top]++
				if !visited[next] {
					visited[next] = true
					stack, positions = append(stack, next), append(positions, 0)
				}
			} else {
				finish = append(finish, node)
				stack, positions = stack[:top], positions[:top]
			}
		}
	}
	visited = make([]bool, len(g.nodes))
	out := []Value{}
	for i := len(finish) - 1; i >= 0; i-- {
		if visited[finish[i]] {
			continue
		}
		component := []int{}
		for stack := []int{finish[i]}; len(stack) > 0; {
			node := stack[len(stack)-1]
			stack = stack[:len(stack)-1]
			if visited[node] {
				continue
			}
			visited[node] = true
			component = append(component, node)
			for _, previous := range incoming[node] {
				if !visited[previous] {
					stack = append(stack, previous)
				}
			}
		}
		out = append(out, g.nodeIds(component))
	}
	return out
}

// reachesAll is whether a walk from the first node reaches every node.
func reachesAll(n int, primary, secondary [][]int) bool {
	if n == 0 {
		return true
	}
	visited := make([]bool, n)
	visited[0] = true
	count := 1
	for queue := []int{0}; len(queue) > 0; queue = queue[1:] {
		for _, rows := range [][][]int{primary, secondary} {
			if rows == nil {
				continue
			}
			for _, next := range rows[queue[0]] {
				if !visited[next] {
					visited[next] = true
					count++
					queue = append(queue, next)
				}
			}
		}
	}
	return count == n
}

func GraphIsConnected(v Value) Value {
	g := graphOf(v)
	g.undirectedOrDefect("Cannot find connected components of directed graph")
	return reachesAll(len(g.nodes), g.outgoing(), nil)
}

func GraphIsWeaklyConnected(v Value) Value {
	g := graphOf(v)
	g.directedOrDefect("Cannot find weakly connected components of undirected graph")
	primary, secondary := g.adjacencies("undirected")
	return reachesAll(len(g.nodes), primary, secondary)
}

func GraphIsStronglyConnected(v Value) Value {
	g := graphOf(v)
	g.directedOrDefect("Cannot find strongly connected components of undirected graph")
	return reachesAll(len(g.nodes), g.outgoing(), nil) && reachesAll(len(g.nodes), g.incoming(), nil)
}

func GraphIsTree(v Value) Value {
	g := graphOf(v)
	g.undirectedOrDefect("Cannot determine tree status of directed graph")
	return len(g.nodes) > 0 && len(g.edges) == len(g.nodes)-1 && reachesAll(len(g.nodes), g.outgoing(), nil)
}

// lowLinks is the bridges, articulation points and biconnected components
// of an undirected graph, by low links.
func (g *Graph) lowLinks() (bridges, points []int, components [][]int) {
	g.undirectedOrDefect("Cannot analyze undirected connectivity of directed graph")
	n := len(g.nodes)
	outgoing, edgeRows := g.rows(false)
	sources, targets := g.endpoints()
	discovered, low, parentNode, parentEdge, children := make([]int, n), make([]int, n), make([]int, n), make([]int, n), make([]int, n)
	bridgeMarks, pointMarks := make([]bool, len(g.edges)), make([]bool, n)
	for p := 0; p < n; p++ {
		discovered[p], parentNode[p], parentEdge[p] = -1, -1, -1
	}
	edgeStack := []int{}
	loops, loopSeen := []int{}, map[int]bool{}
	pop := func(stop int) {
		seen, nodes := map[int]bool{}, []int{}
		for len(edgeStack) > 0 {
			e := edgeStack[len(edgeStack)-1]
			edgeStack = edgeStack[:len(edgeStack)-1]
			for _, p := range []int{sources[e], targets[e]} {
				if !seen[p] {
					seen[p] = true
					nodes = append(nodes, p)
				}
			}
			if e == stop {
				break
			}
		}
		if len(nodes) > 0 {
			sort.Ints(nodes)
			components = append(components, nodes)
		}
	}
	time := 0
	type frame struct{ node, position int }
	for start := 0; start < n; start++ {
		if discovered[start] != -1 {
			continue
		}
		discovered[start], low[start] = time, time
		time++
		stack := []frame{{start, 0}}
		for len(stack) > 0 {
			top := &stack[len(stack)-1]
			node := top.node
			if top.position < len(outgoing[node]) {
				k := top.position
				top.position++
				e, next := edgeRows[node][k], outgoing[node][k]
				switch {
				case next == node:
					if !loopSeen[node] {
						loopSeen[node] = true
						loops = append(loops, node)
					}
				case e == parentEdge[node]:
				case discovered[next] == -1:
					children[node]++
					parentNode[next], parentEdge[next] = node, e
					discovered[next], low[next] = time, time
					time++
					edgeStack = append(edgeStack, e)
					stack = append(stack, frame{next, 0})
				case discovered[next] < discovered[node]:
					low[node] = min(low[node], discovered[next])
					edgeStack = append(edgeStack, e)
				}
				continue
			}
			stack = stack[:len(stack)-1]
			parent := parentNode[node]
			if parent == -1 {
				if children[node] > 1 {
					pointMarks[node] = true
				}
				continue
			}
			low[parent] = min(low[parent], low[node])
			if low[node] > discovered[parent] {
				bridgeMarks[parentEdge[node]] = true
			}
			if low[node] >= discovered[parent] {
				if parentNode[parent] != -1 {
					pointMarks[parent] = true
				}
				pop(parentEdge[node])
			}
		}
	}
	for _, p := range loops {
		components = append(components, []int{p})
	}
	sort.SliceStable(components, func(i, j int) bool {
		a, b := components[i], components[j]
		for k := 0; k < len(a) && k < len(b); k++ {
			if a[k] != b[k] {
				return a[k] < b[k]
			}
		}
		return len(a) < len(b)
	})
	for q, marked := range bridgeMarks {
		if marked {
			bridges = append(bridges, g.edges[q].index)
		}
	}
	for p, marked := range pointMarks {
		if marked {
			points = append(points, g.nodes[p].index)
		}
	}
	return bridges, points, components
}

func GraphBridges(v Value) Value {
	bridges, _, _ := graphOf(v).lowLinks()
	return indexList(bridges)
}

func GraphArticulationPoints(v Value) Value {
	_, points, _ := graphOf(v).lowLinks()
	return indexList(points)
}

func GraphBiconnectedComponents(v Value) Value {
	g := graphOf(v)
	_, _, components := g.lowLinks()
	out := make([]Value, len(components))
	for i, c := range components {
		out[i] = g.nodeIds(c)
	}
	return out
}
