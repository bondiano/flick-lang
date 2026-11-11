package flickrt

import (
	"math"
	"sort"
)

// `HashRing`, persistent as the JS adapter's: each change works on a copy,
// and a node is known by the string the ring's key function gives.

type ringNode struct {
	value  Value
	weight float64
}

type ringPoint struct {
	hash int32
	key  string
}

type HashRing struct {
	key         Value
	baseWeight  float64
	totalWeight float64
	nodes       map[string]ringNode
	ring        []ringPoint
}

func hashRing(v Value) *HashRing { return v.(*HashRing) }

func (r *HashRing) copy() *HashRing {
	next := *r
	next.nodes = make(map[string]ringNode, len(r.nodes))
	for k, n := range r.nodes {
		next.nodes[k] = n
	}
	next.ring = append([]ringPoint{}, r.ring...)
	return &next
}

func (r *HashRing) keyOf(fb *Fiber, a Value) string { return Apply(fb, r.key, a).(string) }

func HashRingMake(options, key Value) Value {
	base := 128.0
	if w := record(options).Get("baseWeight"); isSome(w) {
		base = float64(value(w).(int32))
	}
	return &HashRing{key: key, baseWeight: math.Max(base, 1), nodes: map[string]ringNode{}}
}

func HashRingAddMany(fb *Fiber, options, nodes, ring Value) Value {
	self := hashRing(ring).copy()
	weight := 1.0
	if w := record(options).Get("weight"); isSome(w) {
		weight = value(w).(float64)
	}
	weight = math.Max(weight, 0.1)
	keys := []string{}
	var toRemove map[string]bool
	for _, node := range List(nodes) {
		key := self.keyOf(fb, node)
		if entry, ok := self.nodes[key]; ok {
			self.nodes[key] = ringNode{node, entry.weight}
			if entry.weight == weight {
				continue
			}
			if toRemove == nil {
				toRemove = map[string]bool{}
			}
			toRemove[key] = true
		}
		self.nodes[key] = ringNode{node, weight}
		keys = append(keys, key)
	}
	if toRemove != nil {
		self.ring = ringWithout(self.ring, func(k string) bool { return toRemove[k] })
	}
	self.addToRing(keys, FloatRound(weight*self.baseWeight).(float64))
	self.updateTotalWeight()
	return self
}

func HashRingAdd(fb *Fiber, options, node, ring Value) Value {
	return HashRingAddMany(fb, options, []Value{node}, ring)
}

func ringWithout(ring []ringPoint, drop func(string) bool) []ringPoint {
	out := make([]ringPoint, 0, len(ring))
	for _, p := range ring {
		if !drop(p.key) {
			out = append(out, p)
		}
	}
	return out
}

func (r *HashRing) addToRing(keys []string, weight float64) {
	for i := weight; i > 0; i-- {
		for _, key := range keys {
			r.ring = append(r.ring, ringPoint{hashString(key + ":" + formatNumber(i)), key})
		}
	}
	// ties break by node key, so the order is independent of history
	sort.SliceStable(r.ring, func(i, j int) bool {
		a, b := r.ring[i], r.ring[j]
		if a.hash != b.hash {
			return a.hash < b.hash
		}
		return CompareStrings(a.key, b.key) < 0
	})
}

// updateTotalWeight sums in key order, for the same rounding on every target.
func (r *HashRing) updateTotalWeight() {
	keys := make([]string, 0, len(r.nodes))
	for k := range r.nodes {
		keys = append(keys, k)
	}
	sort.Slice(keys, func(i, j int) bool { return CompareStrings(keys[i], keys[j]) < 0 })
	total := 0.0
	for _, k := range keys {
		total += r.nodes[k].weight
	}
	r.totalWeight = total
}

func HashRingRemove(fb *Fiber, node, ring Value) Value {
	r := hashRing(ring)
	key := r.keyOf(fb, node)
	if _, ok := r.nodes[key]; !ok {
		return r
	}
	self := r.copy()
	delete(self.nodes, key)
	self.ring = ringWithout(self.ring, func(k string) bool { return k == key })
	self.updateTotalWeight()
	return self
}

func HashRingHas(fb *Fiber, node, ring Value) Value {
	r := hashRing(ring)
	_, ok := r.nodes[r.keyOf(fb, node)]
	return ok
}

func HashRingGet(input, ring Value) Value {
	r := hashRing(ring)
	if len(r.ring) == 0 {
		return None
	}
	index, _ := r.indexFor(hashString(input.(string)), nil)
	return Some(r.nodes[r.ring[index].key].value)
}

func HashRingGetShards(count, ring Value) Value {
	r := hashRing(ring)
	if len(r.ring) == 0 {
		return None
	}
	n := 0
	if c := count.(int32); c > 0 {
		n = int(c)
	}
	shards := make([]Value, n)
	allocations := map[string]int{}
	remaining := make([]bool, n)
	exclude := map[string]bool{}
	type distance struct {
		shard int
		node  string
		dist  float64
	}
	hashes := make([]int32, n)
	distances := make([]distance, n)
	for shard := 0; shard < n; shard++ {
		hashes[shard] = hashString("shard-" + formatNumber(float64(shard)))
		index, dist := r.indexFor(hashes[shard], nil)
		distances[shard] = distance{shard, r.ring[index].key, dist}
		remaining[shard] = true
	}
	sort.SliceStable(distances, func(i, j int) bool { return distances[i].dist < distances[j].dist })
	maxPerNode := func(weight float64) int {
		return int(math.Max(1, math.Floor(float64(n)*(weight/r.totalWeight))))
	}
	for _, d := range distances {
		if exclude[d.node] {
			continue
		}
		node := r.nodes[d.node]
		shards[d.shard] = node.value
		remaining[d.shard] = false
		allocations[d.node]++
		if allocations[d.node] >= maxPerNode(node.weight) {
			exclude[d.node] = true
		}
	}
	allAtMax := len(exclude) == len(r.nodes)
	for shard := 0; shard < n; shard++ {
		if !remaining[shard] {
			continue
		}
		var skip map[string]bool
		if !allAtMax {
			skip = exclude
		}
		index, _ := r.indexFor(hashes[shard], skip)
		key := r.ring[index].key
		node := r.nodes[key]
		shards[shard] = node.value
		if allAtMax {
			continue
		}
		allocations[key]++
		if allocations[key] >= maxPerNode(node.weight) {
			exclude[key] = true
			if len(exclude) == len(r.nodes) {
				allAtMax = true
			}
		}
	}
	return Some(shards)
}

// indexFor is the ring point closest to `hash`, skipping the nodes of
// `exclude` when it is not nil.
func (r *HashRing) indexFor(hash int32, exclude map[string]bool) (int, float64) {
	ring := r.ring
	n := len(ring)
	lo, hi := 0, n-1
	for lo <= hi {
		mid := (lo + hi) / 2
		if ring[mid].hash >= hash {
			hi = mid - 1
		} else {
			lo = mid + 1
		}
	}
	dist := func(i int) float64 { return math.Abs(float64(ring[i].hash) - float64(hash)) }
	a := lo
	if lo == n {
		a = lo - 1
	}
	if exclude == nil {
		b := lo - 1
		if b < 0 || dist(a) <= dist(b) {
			return a, dist(a)
		}
		return b, dist(b)
	} else if !exclude[ring[a].key] {
		return a, dist(a)
	}
	for i := 1; i <= max(lo, n-lo); i++ {
		if index := lo - i; index >= 0 && index < n && !exclude[ring[index].key] {
			return index, dist(index)
		}
		if index := lo + i; index >= 0 && index < n && !exclude[ring[index].key] {
			return index, dist(index)
		}
	}
	return a, dist(a)
}
