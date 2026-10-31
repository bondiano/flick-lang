package flickrt

import (
	"reflect"
	"testing"
)

// graphOfEdges is a graph of `n` nodes and the weighted edges, in order.
func graphOfEdges(kind string, n int, edges [][3]float64) *Graph {
	g := newGraph(kind)
	for i := 0; i < n; i++ {
		g.addNode(int32(i))
	}
	for _, e := range edges {
		g.addEdge(int(e[0]), int(e[1]), e[2])
	}
	return g
}

var edgeWeight = Fn(func(_ *Fiber, w Value) Value { return w })

func okOf(t *testing.T, r Value) Value {
	t.Helper()
	if !isOk(r) {
		t.Fatalf("not Ok: %v", Get(value(r), 0))
	}
	return value(r)
}

// The classic flow network: its maximum flow is 23, cut at the edges into
// the last layer.
func TestGraphMaximumFlow(t *testing.T) {
	g := graphOfEdges("directed", 6, [][3]float64{
		{0, 1, 16}, {0, 2, 13}, {1, 2, 10}, {2, 1, 4}, {1, 3, 12},
		{3, 2, 9}, {2, 4, 14}, {4, 3, 7}, {3, 5, 20}, {4, 5, 4},
	})
	config := Rec("source", int32(0), "target", int32(5), "capacity", edgeWeight)
	r := record(okOf(t, GraphMaximumFlow(nil, config, g)))
	if r.Get("value") != 23.0 {
		t.Errorf("value %v", r.Get("value"))
	}
	cut := record(okOf(t, GraphMinimumCut(nil, config, g)))
	if !reflect.DeepEqual(cut.Get("source"), []Value{int32(0), int32(1), int32(2), int32(4)}) {
		t.Errorf("source side %v", cut.Get("source"))
	}
}

func TestGraphStronglyConnectedComponents(t *testing.T) {
	g := graphOfEdges("directed", 5, [][3]float64{{0, 1, 1}, {1, 2, 1}, {2, 0, 1}, {2, 3, 1}, {3, 4, 1}, {4, 3, 1}})
	got := GraphStronglyConnectedComponents(g)
	want := []Value{[]Value{int32(0), int32(2), int32(1)}, []Value{int32(3), int32(4)}}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("components %v", got)
	}
}

// Dijkstra and Bellman-Ford agree on non-negative weights; Bellman-Ford
// finds a cheaper path through a negative edge.
func TestGraphShortestPaths(t *testing.T) {
	g := graphOfEdges("directed", 4, [][3]float64{{0, 1, 4}, {0, 2, 1}, {2, 1, 2}, {1, 3, 1}, {2, 3, 5}})
	config := Rec("source", int32(0), "target", int32(3), "cost", edgeWeight)
	d := record(value(okOf(t, GraphDijkstra(nil, config, g))))
	b := record(value(okOf(t, GraphBellmanFord(nil, config, g))))
	if d.Get("distance") != 4.0 || !reflect.DeepEqual(d.Get("path"), b.Get("path")) {
		t.Errorf("dijkstra %v, bellman-ford %v", d.Get("path"), b.Get("path"))
	}
	negative := graphOfEdges("directed", 3, [][3]float64{{0, 2, 1}, {0, 1, 3}, {1, 2, -5}})
	config = Rec("source", int32(0), "target", int32(2), "cost", edgeWeight)
	if r := record(value(okOf(t, GraphBellmanFord(nil, config, negative)))); r.Get("distance") != -2.0 {
		t.Errorf("negative edge: %v", r.Get("distance"))
	}
	if r := GraphDijkstra(nil, config, negative); isOk(r) {
		t.Errorf("dijkstra took a negative weight")
	}
}

// A path a-b-c with a triangle c-d-e: two bridges and two cut nodes.
func TestGraphLowLinks(t *testing.T) {
	g := graphOfEdges("undirected", 5, [][3]float64{{0, 1, 1}, {1, 2, 1}, {2, 3, 1}, {3, 4, 1}, {4, 2, 1}})
	bridges, points, components := g.lowLinks()
	if !reflect.DeepEqual(bridges, []int{0, 1}) || !reflect.DeepEqual(points, []int{1, 2}) {
		t.Errorf("bridges %v, points %v", bridges, points)
	}
	if !reflect.DeepEqual(components, [][]int{{0, 1}, {1, 2}, {2, 3, 4}}) {
		t.Errorf("components %v", components)
	}
}
