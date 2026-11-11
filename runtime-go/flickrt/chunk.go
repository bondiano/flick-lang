package flickrt

// The `Chunk` functions on `Chunk{items}`, in the order Flick passes the
// arguments: data-last. A chunk holds its elements in a slice, so most are
// the `Array` functions over the items; none ever changes items it shares.

func items(c Value) []Value   { return c.(*Chunk).items }
func chunkOf(xs Value) Value  { return &Chunk{List(xs)} }
func chunkPair(p Value) Value { return []Value{chunkOf(At(p, 0)), chunkOf(At(p, 1))} }

// chunks is a chunk of the chunks of a list of lists.
func chunks(groups Value) Value {
	in := List(groups)
	out := make([]Value, len(in))
	for i, g := range in {
		out[i] = chunkOf(g)
	}
	return &Chunk{out}
}

func ChunkAppend(x, c Value) Value { return chunkOf(ListConcat(items(c), []Value{x})) }
func ChunkConcat(a, b Value) Value { return chunkOf(ListConcat(items(a), items(b))) }
func ChunkGet(i, c Value) Value    { return ListGet(i, items(c)) }

func ChunkMap(fb *Fiber, f, c Value) Value { return chunkOf(ListMap(fb, f, items(c))) }

func ChunkNonEmpty(c Value) Value {
	if len(items(c)) > 0 {
		return Some(c)
	}
	return None
}

func ChunkIsEmpty(c Value) Value { return len(items(c)) == 0 }
func ChunkOf(a Value) Value      { return &Chunk{[]Value{a}} }

// ChunkMake is `make x rest`: the variadic arguments come as a list.
func ChunkMake(x, rest Value) Value { return chunkOf(ListConcat([]Value{x}, rest)) }

func ChunkMakeBy(fb *Fiber, n, f Value) Value { return chunkOf(ArrayMakeBy(fb, n, f)) }
func ChunkRange(start, end Value) Value       { return chunkOf(ArrayRange(start, end)) }

func ChunkHead(c Value) Value         { return ListGet(int32(0), items(c)) }
func ChunkLast(c Value) Value         { return ListGet(int32(len(items(c))-1), items(c)) }
func ChunkHeadNonEmpty(c Value) Value { return items(c)[0] }
func ChunkLastNonEmpty(c Value) Value { return items(c)[len(items(c))-1] }

func ChunkTail(c Value) Value {
	if len(items(c)) > 0 {
		return Some(ChunkTailNonEmpty(c))
	}
	return None
}

func ChunkTailNonEmpty(c Value) Value { return &Chunk{items(c)[1:]} }

func ChunkPrepend(x, c Value) Value         { return chunkOf(ListConcat([]Value{x}, items(c))) }
func ChunkAppendAll(that, self Value) Value { return ChunkConcat(self, that) }

func ChunkTake(n, c Value) Value      { return &Chunk{items(c)[:min(countOf(n), len(items(c)))]} }
func ChunkDrop(n, c Value) Value      { return &Chunk{items(c)[min(countOf(n), len(items(c))):]} }
func ChunkTakeRight(n, c Value) Value { return chunkOf(ArrayTakeRight(n, items(c))) }
func ChunkDropRight(n, c Value) Value { return chunkOf(ArrayDropRight(n, items(c))) }

func ChunkTakeWhile(fb *Fiber, f, c Value) Value { return chunkOf(ArrayTakeWhile(fb, f, items(c))) }
func ChunkDropWhile(fb *Fiber, f, c Value) Value { return chunkOf(ArrayDropWhile(fb, f, items(c))) }

func ChunkSplitAt(n, c Value) Value { return chunkPair(ArraySplitAt(n, items(c))) }

// ChunkSplitNonEmptyAt keeps at least one element in the first part.
func ChunkSplitNonEmptyAt(n, c Value) Value {
	k := max(1, countOf(n))
	if k >= len(items(c)) {
		return []Value{c, ChunkEmpty}
	}
	return chunkPair(ArraySplitAt(int32(k), items(c)))
}

func ChunkSplit(n, c Value) Value                 { return chunks(ArraySplit(n, items(c))) }
func ChunkSplitWhere(fb *Fiber, f, c Value) Value { return chunkPair(ArraySplitWhere(fb, f, items(c))) }
func ChunkChunksOf(n, c Value) Value              { return chunks(ArrayChunksOf(n, items(c))) }
func ChunkRemove(i, c Value) Value                { return chunkOf(ArrayRemove(i, items(c))) }

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
	return chunkOf(ArrayTakeWhileFilter(fb, f, items(c)))
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
	return ArrayContainsWith(fb, eq, a, items(c))
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

func ChunkEvery(fb *Fiber, f, c Value) Value          { return ListAll(fb, f, items(c)) }
func ChunkSome(fb *Fiber, f, c Value) Value           { return ListAny(fb, f, items(c)) }
func ChunkFindFirst(fb *Fiber, f, c Value) Value      { return ListFind(fb, f, items(c)) }
func ChunkFindFirstIndex(fb *Fiber, f, c Value) Value { return ListFindIndex(fb, f, items(c)) }
func ChunkFindLast(fb *Fiber, f, c Value) Value       { return ArrayFindLast(fb, f, items(c)) }
func ChunkFindLastIndex(fb *Fiber, f, c Value) Value  { return ArrayFindLastIndex(fb, f, items(c)) }

func ChunkJoin(sep, c Value) Value { return StringJoin(sep, items(c)) }

func ChunkMapAccum(fb *Fiber, s, f, c Value) Value {
	r := List(ArrayMapAccum(fb, s, f, items(c)))
	return []Value{r[0], chunkOf(r[1])}
}

func ChunkPartition(fb *Fiber, f, c Value) Value { return chunkPair(ArrayPartition(fb, f, items(c))) }
func ChunkSeparate(c Value) Value                { return chunkPair(ArraySeparate(nil, items(c))) }

func ChunkReduce(fb *Fiber, b, f, c Value) Value {
	acc := b
	for _, x := range items(c) {
		acc = Apply2(fb, f, acc, x)
	}
	return acc
}

func ChunkReduceRight(fb *Fiber, b, f, c Value) Value {
	acc, in := b, items(c)
	for i := len(in) - 1; i >= 0; i-- {
		acc = Apply2(fb, f, acc, in[i])
	}
	return acc
}

func ChunkSortBy(fb *Fiber, cmp, c Value) Value { return chunkOf(ListSortBy(fb, cmp, items(c))) }

func ChunkUnzip(c Value) Value        { return chunkPair(ListUnzip(items(c))) }
func ChunkZip(that, self Value) Value { return chunkOf(ListZip(items(self), items(that))) }

func ChunkZipWith(fb *Fiber, that, f, self Value) Value {
	return chunkOf(ArrayZipWith(fb, items(that), f, items(self)))
}
