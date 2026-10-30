package flickrt

import "math"

// Effect's `Array` functions the standard library binds, in the order Flick
// passes the arguments: data-last, the list at the end. Each copies what it
// returns; edge cases (counts, bounds, empty lists) are Effect's.

func pred(fb *Fiber, f, a Value) bool { return Bool(Apply(fb, f, a)) }

// countOf is Effect's `Count.normalize`: a positive count, else 0.
func countOf(n Value) int { return max(0, int(n.(int32))) }

func ArrayFindLast(fb *Fiber, f, xs Value) Value {
	in := List(xs)
	for i := len(in) - 1; i >= 0; i-- {
		if pred(fb, f, in[i]) {
			return Some(in[i])
		}
	}
	return None
}

func ArrayFindLastIndex(fb *Fiber, f, xs Value) Value {
	in := List(xs)
	for i := len(in) - 1; i >= 0; i-- {
		if pred(fb, f, in[i]) {
			return Some(int32(i))
		}
	}
	return None
}

func ArrayFindFirstWithIndex(fb *Fiber, f, xs Value) Value {
	for i, x := range List(xs) {
		if pred(fb, f, x) {
			return Some([]Value{x, int32(i)})
		}
	}
	return None
}

func ArrayCountBy(fb *Fiber, f, xs Value) Value {
	n := int32(0)
	for _, x := range List(xs) {
		if pred(fb, f, x) {
			n++
		}
	}
	return n
}

// ArrayContainsWith is whether `eq a x` holds for an element `x`.
func ArrayContainsWith(fb *Fiber, eq, a, xs Value) Value {
	for _, x := range List(xs) {
		if Bool(apply2(fb, eq, a, x)) {
			return true
		}
	}
	return false
}

// ArrayDedupeWith keeps each element no earlier one is equivalent to.
func ArrayDedupeWith(fb *Fiber, eq, xs Value) Value {
	out := []Value{}
next:
	for _, x := range List(xs) {
		for _, kept := range out {
			if Bool(apply2(fb, eq, x, kept)) {
				continue next
			}
		}
		out = append(out, x)
	}
	return out
}

func ArrayDedupeAdjacentWith(fb *Fiber, eq, xs Value) Value {
	out := []Value{}
	for _, x := range List(xs) {
		if len(out) == 0 || !Bool(apply2(fb, eq, x, out[len(out)-1])) {
			out = append(out, x)
		}
	}
	return out
}

// ArrayDifferenceWith is the elements of `self` not in `that`, as Effect's
// data-last curried form reads `difference_with eq that self`.
func ArrayDifferenceWith(fb *Fiber, eq, that, self Value) Value {
	return ArrayFilter(fb, Fn(func(fb *Fiber, a Value) Value { return !ArrayContainsWith(fb, eq, a, that).(bool) }), self)
}

func ArrayIntersectionWith(fb *Fiber, eq, that, self Value) Value {
	return ArrayFilter(fb, Fn(func(fb *Fiber, a Value) Value { return ArrayContainsWith(fb, eq, a, that) }), self)
}

func ArrayFilter(fb *Fiber, f, xs Value) Value { return ListFilter(fb, f, xs) }

// ArrayUnionWith dedupes both, unless one is empty: then the other as it is.
func ArrayUnionWith(fb *Fiber, that, eq, self Value) Value {
	a, b := List(self), List(that)
	switch {
	case len(a) == 0:
		return b
	case len(b) == 0:
		return a
	}
	return ArrayDedupeWith(fb, eq, ListConcat(a, b))
}

// ArrayGroupBy is a record of the elements by key, keys in JS key order.
func ArrayGroupBy(fb *Fiber, f, xs Value) Value {
	r := &Record{}
	for _, x := range List(xs) {
		k := Apply(fb, f, x).(string)
		if i := r.index(k); i >= 0 {
			r = r.set(k, append(append([]Value{}, List(r.vals[i])...), x))
		} else {
			r = r.set(k, []Value{x})
		}
	}
	return r
}

func ArrayJoin(sep, xs Value) Value { return StringJoin(sep, xs) }

func ArrayTakeRight(n, xs Value) Value {
	in := List(xs)
	return in[len(in)-min(countOf(n), len(in)):]
}

func ArrayDropRight(n, xs Value) Value {
	in := List(xs)
	return in[:len(in)-min(countOf(n), len(in))]
}

func spanIndex(fb *Fiber, f, xs Value) int {
	in := List(xs)
	i := 0
	for i < len(in) && pred(fb, f, in[i]) {
		i++
	}
	return i
}

func ArrayTakeWhile(fb *Fiber, f, xs Value) Value { return List(xs)[:spanIndex(fb, f, xs)] }
func ArrayDropWhile(fb *Fiber, f, xs Value) Value { return List(xs)[spanIndex(fb, f, xs):] }

func ArrayTakeWhileFilter(fb *Fiber, f, xs Value) Value {
	out := []Value{}
	for _, x := range List(xs) {
		r := Apply(fb, f, x)
		if !isOk(r) {
			break
		}
		out = append(out, value(r))
	}
	return out
}

func ArrayDropWhileFilter(fb *Fiber, f, xs Value) Value {
	in := List(xs)
	i := 0
	for i < len(in) && isOk(Apply(fb, f, in[i])) {
		i++
	}
	return in[i:]
}

func ArraySpan(fb *Fiber, f, xs Value) Value {
	i := spanIndex(fb, f, xs)
	return []Value{List(xs)[:i], List(xs)[i:]}
}

func ArraySplitAt(n, xs Value) Value {
	in := List(xs)
	k := min(countOf(n), len(in))
	return []Value{in[:k], in[k:]}
}

// ArraySplit is `n` groups as even as can be, the last the shortest.
func ArraySplit(n, xs Value) Value {
	in := List(xs)
	size := int(math.Ceil(float64(len(in)) / float64(max(1, countOf(n)))))
	return ArrayChunksOf(int32(size), in)
}

func ArraySplitWhere(fb *Fiber, f, xs Value) Value {
	return ArraySpan(fb, Fn(func(fb *Fiber, a Value) Value { return !pred(fb, f, a) }), xs)
}

func ArrayChunksOf(n, xs Value) Value {
	in := List(xs)
	size := max(1, countOf(n))
	out := []Value{}
	for i := 0; i < len(in); i += size {
		out = append(out, in[i:min(i+size, len(in))])
	}
	return out
}

func ArrayWindow(n, xs Value) Value {
	in := List(xs)
	size := countOf(n)
	out := []Value{}
	if size == 0 || size > len(in) {
		return out
	}
	for i := 0; i+size <= len(in); i++ {
		out = append(out, in[i:i+size])
	}
	return out
}

// ArrayChop applies `f` to what is left until nothing is.
func ArrayChop(fb *Fiber, f, xs Value) Value {
	out := []Value{}
	for rest := List(xs); len(rest) > 0; {
		step := Apply(fb, f, rest).([]Value)
		out = append(out, step[0])
		rest = List(step[1])
	}
	return out
}

func ArrayTail(xs Value) Value {
	if in := List(xs); len(in) > 0 {
		return Some(in[1:])
	}
	return None
}

func ArrayInit(xs Value) Value {
	if in := List(xs); len(in) > 0 {
		return Some(in[:len(in)-1])
	}
	return None
}

// ArrayPad is `n` elements: cut, or filled with `fill`.
func ArrayPad(n, fill, xs Value) Value {
	in, size := List(xs), countOf(n)
	if len(in) >= size {
		return in[:size]
	}
	return ListConcat(in, ListRepeat(int32(size-len(in)), fill))
}

func ArrayRotate(n, xs Value) Value {
	in := List(xs)
	if len(in) == 0 {
		return []Value{}
	}
	m := int(n.(int32)) % len(in)
	if m < 0 {
		m += len(in)
	}
	// a positive count moves the elements right
	return ListConcat(in[len(in)-m:], in[:len(in)-m])
}

func ArrayInsertAt(i, b, xs Value) Value {
	in, k := List(xs), int(i.(int32))
	if k < 0 || k > len(in) {
		return None
	}
	out := make([]Value, 0, len(in)+1)
	return Some(append(append(append(out, in[:k]...), b), in[k:]...))
}

func ArrayRemove(i, xs Value) Value {
	in, k := List(xs), int(i.(int32))
	if k < 0 || k >= len(in) {
		return append([]Value{}, in...)
	}
	return ListConcat(in[:k], in[k+1:])
}

func ArrayModify(fb *Fiber, i, f, xs Value) Value {
	in, k := List(xs), int(i.(int32))
	if k < 0 || k >= len(in) {
		return None
	}
	out := append([]Value{}, in...)
	out[k] = Apply(fb, f, in[k])
	return Some(out)
}

func ArrayReplace(fb *Fiber, i, b, xs Value) Value {
	return ArrayModify(fb, i, Fn(func(fb *Fiber, _ Value) Value { return b }), xs)
}

func ArrayIntersperse(middle, xs Value) Value {
	out := []Value{}
	for i, x := range List(xs) {
		if i > 0 {
			out = append(out, middle)
		}
		out = append(out, x)
	}
	return out
}

func ArrayAppendAll(that, self Value) Value { return ListConcat(self, that) }

func ArrayCartesianWith(fb *Fiber, that, f, self Value) Value {
	out := []Value{}
	for _, a := range List(self) {
		for _, b := range List(that) {
			out = append(out, apply2(fb, f, a, b))
		}
	}
	return out
}

func ArrayCartesian(fb *Fiber, that, self Value) Value {
	return ArrayCartesianWith(fb, that, Curry2(func(fb *Fiber, a, b Value) Value { return []Value{a, b} }), self)
}

func ArrayZipWith(fb *Fiber, that, f, self Value) Value {
	a, b := List(self), List(that)
	out := make([]Value, min(len(a), len(b)))
	for i := range out {
		out[i] = apply2(fb, f, a[i], b[i])
	}
	return out
}

func ArrayMapAccum(fb *Fiber, s, f, xs Value) Value {
	acc, out := s, []Value{}
	for _, x := range List(xs) {
		r := apply2(fb, f, acc, x).([]Value)
		acc = r[0]
		out = append(out, r[1])
	}
	return []Value{acc, out}
}

func ArrayScan(fb *Fiber, b, f, xs Value) Value {
	out := []Value{b}
	for _, x := range List(xs) {
		out = append(out, apply2(fb, f, out[len(out)-1], x))
	}
	return out
}

func ArrayScanRight(fb *Fiber, b, f, xs Value) Value {
	in := List(xs)
	out := make([]Value, len(in)+1)
	out[len(in)] = b
	for i := len(in) - 1; i >= 0; i-- {
		out[i] = apply2(fb, f, out[i+1], in[i])
	}
	return out
}

// ArrayExtend is `f` of each suffix.
func ArrayExtend(fb *Fiber, f, xs Value) Value {
	in := List(xs)
	out := make([]Value, len(in))
	for i := range in {
		out[i] = Apply(fb, f, in[i:])
	}
	return out
}

func ArrayFilterMapWhile(fb *Fiber, f, xs Value) Value { return ArrayTakeWhileFilter(fb, f, xs) }

func ArrayGetSomes(xs Value) Value {
	out := []Value{}
	for _, o := range List(xs) {
		if isSome(o) {
			out = append(out, value(o))
		}
	}
	return out
}

func results(xs Value, ok bool) Value {
	out := []Value{}
	for _, r := range List(xs) {
		if isOk(r) == ok {
			out = append(out, value(r))
		}
	}
	return out
}

func ArrayGetFailures(xs Value) Value  { return results(xs, false) }
func ArrayGetSuccesses(xs Value) Value { return results(xs, true) }

func ArrayPartition(fb *Fiber, f, xs Value) Value {
	passes, fails := []Value{}, []Value{}
	for _, x := range List(xs) {
		if r := Apply(fb, f, x); isOk(r) {
			passes = append(passes, value(r))
		} else {
			fails = append(fails, value(r))
		}
	}
	return []Value{passes, fails}
}

func ArraySeparate(fb *Fiber, xs Value) Value {
	return ArrayPartition(fb, Fn(func(_ *Fiber, v Value) Value { return Identity(v) }), xs)
}

func ArrayUnfold(fb *Fiber, b, f Value) Value {
	out := []Value{}
	for o := Apply(fb, f, b); isSome(o); {
		pair := value(o).([]Value)
		out = append(out, pair[0])
		o = Apply(fb, f, pair[1])
	}
	return out
}

func ArrayLiftOption(fb *Fiber, f, a Value) Value {
	if o := Apply(fb, f, a); isSome(o) {
		return []Value{value(o)}
	}
	return []Value{}
}

func ArrayLiftPredicate(fb *Fiber, p, a Value) Value {
	if pred(fb, p, a) {
		return []Value{a}
	}
	return []Value{}
}

func ArrayLiftResult(fb *Fiber, f, a Value) Value {
	if r := Apply(fb, f, a); isOk(r) {
		return []Value{value(r)}
	}
	return []Value{}
}

// ArrayRepeat is the list `n` times, at least once.
func ArrayRepeat(n, xs Value) Value {
	out := []Value{}
	for i := 0; i < max(1, countOf(n)); i++ {
		out = append(out, List(xs)...)
	}
	return out
}

func ArrayMatch(fb *Fiber, options, xs Value) Value {
	r := record(options)
	if in := List(xs); len(in) > 0 {
		return Apply(fb, r.Get("onNonEmpty"), in)
	}
	return Apply(fb, r.Get("onEmpty"), Unit)
}

func ArrayMatchLeft(fb *Fiber, options, xs Value) Value {
	r := record(options)
	if in := List(xs); len(in) > 0 {
		return apply2(fb, r.Get("onNonEmpty"), in[0], in[1:])
	}
	return Apply(fb, r.Get("onEmpty"), Unit)
}

func ArrayMatchRight(fb *Fiber, options, xs Value) Value {
	r := record(options)
	if in := List(xs); len(in) > 0 {
		return apply2(fb, r.Get("onNonEmpty"), in[:len(in)-1], in[len(in)-1])
	}
	return Apply(fb, r.Get("onEmpty"), Unit)
}

func NonEmptyFromList(xs Value) Value {
	if len(List(xs)) > 0 {
		return Some(xs)
	}
	return None
}

func NonEmptyCons(x, xs Value) Value { return ListConcat([]Value{x}, xs) }
func NonEmptyHead(xs Value) Value    { return List(xs)[0] }
func ArrayAppend(x, xs Value) Value  { return ListConcat(xs, []Value{x}) }

// ArrayMakeBy is `f` of each index below `n`, at least one.
func ArrayMakeBy(fb *Fiber, n, f Value) Value {
	out := make([]Value, max(1, countOf(n)))
	for i := range out {
		out[i] = Apply(fb, f, int32(i))
	}
	return out
}

func ArrayRange(start, end Value) Value {
	a, b := start.(int32), end.(int32)
	if a > b {
		return []Value{a}
	}
	return ListRange(a, b+1)
}

func ArrayReplicate(fb *Fiber, n, a Value) Value {
	return ArrayMakeBy(fb, n, Fn(func(fb *Fiber, _ Value) Value { return a }))
}

func NonEmptyLast(xs Value) Value { in := List(xs); return in[len(in)-1] }
func NonEmptyTail(xs Value) Value { return List(xs)[1:] }
func NonEmptyInit(xs Value) Value { in := List(xs); return in[:len(in)-1] }

func NonEmptyModifyHead(fb *Fiber, f, xs Value) Value {
	out := append([]Value{}, List(xs)...)
	out[0] = Apply(fb, f, out[0])
	return out
}

func NonEmptySetHead(fb *Fiber, b, xs Value) Value {
	return NonEmptyModifyHead(fb, Fn(func(fb *Fiber, _ Value) Value { return b }), xs)
}

func NonEmptyModifyLast(fb *Fiber, f, xs Value) Value {
	out := append([]Value{}, List(xs)...)
	out[len(out)-1] = Apply(fb, f, out[len(out)-1])
	return out
}

func NonEmptySetLast(fb *Fiber, b, xs Value) Value {
	return NonEmptyModifyLast(fb, Fn(func(fb *Fiber, _ Value) Value { return b }), xs)
}

// NonEmptySplitAt splits after at least the first element.
func NonEmptySplitAt(n, xs Value) Value {
	in := List(xs)
	k := min(max(1, countOf(n)), len(in))
	return []Value{in[:k], in[k:]}
}

func NonEmptyUnappend(xs Value) Value  { in := List(xs); return []Value{in[:len(in)-1], in[len(in)-1]} }
func NonEmptyUnprepend(xs Value) Value { in := List(xs); return []Value{in[0], in[1:]} }

// NonEmptyGroupWith groups each run of elements equivalent to its first.
func NonEmptyGroupWith(fb *Fiber, eq, xs Value) Value {
	in := List(xs)
	out := []Value{}
	for i := 0; i < len(in); {
		j := i + 1
		for j < len(in) && Bool(apply2(fb, eq, in[j], in[i])) {
			j++
		}
		out = append(out, in[i:j])
		i = j
	}
	return out
}

// NonEmptyMax and NonEmptyMin keep the first of equal elements.
func NonEmptyMax(fb *Fiber, cmp, xs Value) Value {
	o, in := orderOf(cmp), List(xs)
	best := in[0]
	for _, x := range in[1:] {
		if o(fb, best, x) < 0 {
			best = x
		}
	}
	return best
}

func NonEmptyMin(fb *Fiber, cmp, xs Value) Value {
	o, in := orderOf(cmp), List(xs)
	best := in[0]
	for _, x := range in[1:] {
		if o(fb, best, x) > 0 {
			best = x
		}
	}
	return best
}
