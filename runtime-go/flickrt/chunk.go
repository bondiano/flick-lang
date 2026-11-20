package flickrt

// Chunk bindings use the order Flick passes their arguments: data-last.

func chunkPair(p Value) Value { return []Value{chunkOf(At(p, 0)), chunkOf(At(p, 1))} }

// chunks is a chunk of the chunks of a list of lists.
func chunks(groups Value) Value {
	in := List(groups)
	out := make([]Value, len(in))
	for i, g := range in {
		out[i] = chunkOf(g)
	}
	return chunkOf(out)
}

func ChunkAppend(x, c Value) Value { return joinChunks(c.(*Chunk), chunkOf([]Value{x})) }
func ChunkConcat(a, b Value) Value { return joinChunks(a.(*Chunk), b.(*Chunk)) }

func ChunkGet(i, c Value) Value {
	xs, index := c.(*Chunk), int(i.(int32))
	if index < 0 || index >= xs.length() {
		return None
	}
	return Some(xs.at(index))
}

func ChunkMap(fb *Fiber, f, c Value) Value { return chunkOf(ListMap(fb, f, items(c))) }

func ChunkNonEmpty(c Value) Value {
	if c.(*Chunk).length() > 0 {
		return Some(c)
	}
	return None
}

func ChunkIsEmpty(c Value) Value { return c.(*Chunk).length() == 0 }
func ChunkOf(a Value) Value      { return chunkOf([]Value{a}) }

// ChunkMake is `make x rest`: the variadic arguments come as a list.
func ChunkMake(x, rest Value) Value { return chunkOf(ListConcat([]Value{x}, rest)) }

func ChunkMakeBy(fb *Fiber, n, f Value) Value { return chunkOf(ArrayMakeBy(fb, n, f)) }
func ChunkRange(start, end Value) Value       { return chunkOf(ArrayRange(start, end)) }

func ChunkHead(c Value) Value { return ChunkGet(int32(0), c) }
func ChunkLast(c Value) Value {
	xs := c.(*Chunk)
	if xs.length() == 0 {
		return None
	}
	return Some(xs.at(xs.length() - 1))
}
func ChunkHeadNonEmpty(c Value) Value { return c.(*Chunk).at(0) }
func ChunkLastNonEmpty(c Value) Value {
	xs := c.(*Chunk)
	return xs.at(xs.length() - 1)
}

func ChunkTail(c Value) Value {
	if c.(*Chunk).length() > 0 {
		return Some(ChunkTailNonEmpty(c))
	}
	return None
}

func ChunkTailNonEmpty(c Value) Value {
	xs := c.(*Chunk)
	return xs.slice(1, xs.length())
}

func ChunkPrepend(x, c Value) Value         { return joinChunks(chunkOf([]Value{x}), c.(*Chunk)) }
func ChunkAppendAll(that, self Value) Value { return ChunkConcat(self, that) }

func ChunkTake(n, c Value) Value {
	xs := c.(*Chunk)
	return xs.slice(0, min(countOf(n), xs.length()))
}
func ChunkDrop(n, c Value) Value {
	xs := c.(*Chunk)
	return xs.slice(min(countOf(n), xs.length()), xs.length())
}
func ChunkTakeRight(n, c Value) Value {
	xs := c.(*Chunk)
	return xs.slice(max(0, xs.length()-countOf(n)), xs.length())
}
func ChunkDropRight(n, c Value) Value {
	xs := c.(*Chunk)
	return xs.slice(0, max(0, xs.length()-countOf(n)))
}

func chunkSpanIndex(fb *Fiber, f Value, xs *Chunk) int {
	i := 0
	xs.walk(false, func(x Value) bool {
		if !pred(fb, f, x) {
			return false
		}
		i++
		return true
	})
	return i
}

func ChunkTakeWhile(fb *Fiber, f, c Value) Value {
	xs := c.(*Chunk)
	return xs.slice(0, chunkSpanIndex(fb, f, xs))
}
func ChunkDropWhile(fb *Fiber, f, c Value) Value {
	xs := c.(*Chunk)
	return xs.slice(chunkSpanIndex(fb, f, xs), xs.length())
}

func chunkSplitAt(i int, c *Chunk) Value {
	return []Value{c.slice(0, i), c.slice(i, c.length())}
}

func ChunkSplitAt(n, c Value) Value {
	xs := c.(*Chunk)
	return chunkSplitAt(min(countOf(n), xs.length()), xs)
}

// ChunkSplitNonEmptyAt keeps at least one element in the first part.
func ChunkSplitNonEmptyAt(n, c Value) Value {
	xs := c.(*Chunk)
	return chunkSplitAt(min(max(1, countOf(n)), xs.length()), xs)
}

func ChunkSplit(n, c Value) Value { return chunks(ArraySplit(n, items(c))) }
func ChunkSplitWhere(fb *Fiber, f, c Value) Value {
	xs := c.(*Chunk)
	i := 0
	xs.walk(false, func(x Value) bool {
		if pred(fb, f, x) {
			return false
		}
		i++
		return true
	})
	return chunkSplitAt(i, xs)
}
func ChunkChunksOf(n, c Value) Value { return chunks(ArrayChunksOf(n, items(c))) }
func ChunkRemove(i, c Value) Value   { return chunkOf(ArrayRemove(i, items(c))) }

func ChunkModify(fb *Fiber, i, f, c Value) Value {
	if o := ArrayModify(fb, i, f, items(c)); isSome(o) {
		return Some(chunkOf(value(o)))
	}
	return None
}

func ChunkReplace(fb *Fiber, i, b, c Value) Value {
	return ChunkModify(fb, i, Fn(func(*Fiber, Value) Value { return b }), c)
}

func ChunkReverse(c Value) Value { return chunkOf(ListReverse(items(c))) }

func ChunkFilter(fb *Fiber, f, c Value) Value { return chunkOf(ListFilter(fb, f, items(c))) }

func ChunkFilterMap(fb *Fiber, f, c Value) Value {
	out := []Value{}
	for _, x := range items(c) {
		if r := Apply(fb, f, x); isOk(r) {
			out = append(out, value(r))
		}
	}
	return chunkOf(out)
}

// ChunkFilterMapWhile stops at the first failure.
func ChunkFilterMapWhile(fb *Fiber, f, c Value) Value {
	out := []Value{}
	c.(*Chunk).walk(false, func(x Value) bool {
		r := Apply(fb, f, x)
		if !isOk(r) {
			return false
		}
		out = append(out, value(r))
		return true
	})
	return chunkOf(out)
}

func ChunkCompact(c Value) Value {
	out := []Value{}
	for _, o := range items(c) {
		if isSome(o) {
			out = append(out, value(o))
		}
	}
	return chunkOf(out)
}

func ChunkFlatMap(fb *Fiber, f, c Value) Value {
	out := []Value{}
	for _, x := range items(c) {
		out = append(out, items(Apply(fb, f, x))...)
	}
	return chunkOf(out)
}

func ChunkFlatten(c Value) Value {
	out := []Value{}
	for _, inner := range items(c) {
		out = append(out, items(inner)...)
	}
	return chunkOf(out)
}

func ChunkContainsWith(fb *Fiber, eq, a, c Value) Value {
	return !c.(*Chunk).walk(false, func(x Value) bool { return !Bool(Apply2(fb, eq, a, x)) })
}

// equalsFn is the structural `==` as an equivalence.
var equalsFn = Curry2(func(_ *Fiber, a, b Value) Value { return Equals(a, b) })

func ChunkDedupe(c Value) Value { return chunkOf(ArrayDedupeWith(nil, equalsFn, items(c))) }
func ChunkDedupeAdjacent(c Value) Value {
	return chunkOf(ArrayDedupeAdjacentWith(nil, equalsFn, items(c)))
}

func ChunkDifferenceWith(fb *Fiber, eq, that, self Value) Value {
	return chunkOf(ArrayDifferenceWith(fb, eq, items(that), items(self)))
}

func ChunkIntersection(that, self Value) Value {
	return chunkOf(ArrayIntersectionWith(nil, equalsFn, items(that), items(self)))
}

func ChunkUnion(that, self Value) Value {
	return chunkOf(ArrayUnionWith(nil, items(that), equalsFn, items(self)))
}

func ChunkEvery(fb *Fiber, f, c Value) Value {
	return c.(*Chunk).walk(false, func(x Value) bool { return pred(fb, f, x) })
}
func ChunkSome(fb *Fiber, f, c Value) Value {
	return !c.(*Chunk).walk(false, func(x Value) bool { return !pred(fb, f, x) })
}

func chunkFind(fb *Fiber, f Value, c *Chunk, reverse bool) (Value, int) {
	i, step := 0, 1
	if reverse {
		i, step = c.length()-1, -1
	}
	var found Value
	if c.walk(reverse, func(x Value) bool {
		if pred(fb, f, x) {
			found = x
			return false
		}
		i += step
		return true
	}) {
		return nil, -1
	}
	return found, i
}

func ChunkFindFirst(fb *Fiber, f, c Value) Value {
	if x, i := chunkFind(fb, f, c.(*Chunk), false); i >= 0 {
		return Some(x)
	}
	return None
}
func ChunkFindFirstIndex(fb *Fiber, f, c Value) Value {
	if _, i := chunkFind(fb, f, c.(*Chunk), false); i >= 0 {
		return Some(int32(i))
	}
	return None
}
func ChunkFindLast(fb *Fiber, f, c Value) Value {
	if x, i := chunkFind(fb, f, c.(*Chunk), true); i >= 0 {
		return Some(x)
	}
	return None
}
func ChunkFindLastIndex(fb *Fiber, f, c Value) Value {
	if _, i := chunkFind(fb, f, c.(*Chunk), true); i >= 0 {
		return Some(int32(i))
	}
	return None
}

func ChunkJoin(sep, c Value) Value { return StringJoin(sep, items(c)) }

func ChunkMapAccum(fb *Fiber, s, f, c Value) Value {
	r := List(ArrayMapAccum(fb, s, f, items(c)))
	return []Value{r[0], chunkOf(r[1])}
}

func ChunkPartition(fb *Fiber, f, c Value) Value { return chunkPair(ArrayPartition(fb, f, items(c))) }
func ChunkSeparate(c Value) Value                { return chunkPair(ArraySeparate(nil, items(c))) }

func ChunkReduce(fb *Fiber, b, f, c Value) Value {
	return c.(*Chunk).fold(fb, b, f, false)
}

func ChunkReduceRight(fb *Fiber, b, f, c Value) Value {
	return c.(*Chunk).fold(fb, b, f, true)
}

func ChunkSortBy(fb *Fiber, cmp, c Value) Value { return chunkOf(ListSortBy(fb, cmp, items(c))) }

func ChunkUnzip(c Value) Value        { return chunkPair(ListUnzip(items(c))) }
func ChunkZip(that, self Value) Value { return chunkOf(ListZip(items(self), items(that))) }

func ChunkZipWith(fb *Fiber, that, f, self Value) Value {
	return chunkOf(ArrayZipWith(fb, items(that), f, items(self)))
}
