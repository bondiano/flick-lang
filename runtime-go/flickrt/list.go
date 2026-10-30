package flickrt

import "sort"

// Lists are immutable []Value: an operation returns a new slice, or the
// same one when nothing changes. Callbacks are curried Flick functions.

func apply2(fb *Fiber, f, a, b Value) Value { return Apply(fb, Apply(fb, f, a), b) }

func ListMap(fb *Fiber, f, xs Value) Value {
	in := List(xs)
	out := make([]Value, len(in))
	for i, x := range in {
		out[i] = Apply(fb, f, x)
	}
	return out
}

func ListMapIndexed(fb *Fiber, f, xs Value) Value {
	in := List(xs)
	out := make([]Value, len(in))
	for i, x := range in {
		out[i] = apply2(fb, f, int32(i), x)
	}
	return out
}

// ListEachEffect runs the task of `f` of each element.
func ListEachEffect(fb *Fiber, f, xs Value) Value {
	for _, x := range List(xs) {
		Run(fb, Apply(fb, f, x))
	}
	return Unit
}

func ListFold(fb *Fiber, f, init, xs Value) Value {
	acc := init
	for _, x := range List(xs) {
		acc = apply2(fb, f, acc, x)
	}
	return acc
}

func ListFoldRight(fb *Fiber, f, init, xs Value) Value {
	in := List(xs)
	acc := init
	for i := len(in) - 1; i >= 0; i-- {
		acc = apply2(fb, f, in[i], acc)
	}
	return acc
}

func ListFilter(fb *Fiber, p, xs Value) Value {
	out := []Value{}
	for _, x := range List(xs) {
		if Bool(Apply(fb, p, x)) {
			out = append(out, x)
		}
	}
	return out
}

func ListFilterMap(fb *Fiber, f, xs Value) Value {
	out := []Value{}
	for _, x := range List(xs) {
		if o := Apply(fb, f, x).(*Data); o.Tag == "Some" {
			out = append(out, o.Fields[0])
		}
	}
	return out
}

func ListFlatMap(fb *Fiber, f, xs Value) Value {
	out := []Value{}
	for _, x := range List(xs) {
		out = append(out, List(Apply(fb, f, x))...)
	}
	return out
}

// ListMap2 is `Applicative.map2` of lists: every pair, the first list
// outermost.
func ListMap2(fb *Fiber, f, xs, ys Value) Value {
	out := []Value{}
	for _, x := range List(xs) {
		for _, y := range List(ys) {
			out = append(out, apply2(fb, f, x, y))
		}
	}
	return out
}

func OptionMap2(fb *Fiber, f, x, y Value) Value {
	a, b := x.(*Data), y.(*Data)
	if a.Tag == "Some" && b.Tag == "Some" {
		return Some(apply2(fb, f, a.Fields[0], b.Fields[0]))
	}
	return None
}

func ResultMap2(fb *Fiber, f, x, y Value) Value {
	a, b := x.(*Data), y.(*Data)
	switch {
	case a.Tag == "Failure":
		return a
	case b.Tag == "Failure":
		return b
	}
	return Ok(apply2(fb, f, a.Fields[0], b.Fields[0]))
}

func ListFind(fb *Fiber, p, xs Value) Value {
	for _, x := range List(xs) {
		if Bool(Apply(fb, p, x)) {
			return Some(x)
		}
	}
	return None
}

func ListFindIndex(fb *Fiber, p, xs Value) Value {
	for i, x := range List(xs) {
		if Bool(Apply(fb, p, x)) {
			return Some(int32(i))
		}
	}
	return None
}

func ListAny(fb *Fiber, p, xs Value) Value {
	for _, x := range List(xs) {
		if Bool(Apply(fb, p, x)) {
			return true
		}
	}
	return false
}

func ListAll(fb *Fiber, p, xs Value) Value {
	for _, x := range List(xs) {
		if !Bool(Apply(fb, p, x)) {
			return false
		}
	}
	return true
}

func ListPartition(fb *Fiber, p, xs Value) Value {
	yes, no := []Value{}, []Value{}
	for _, x := range List(xs) {
		if Bool(Apply(fb, p, x)) {
			yes = append(yes, x)
		} else {
			no = append(no, x)
		}
	}
	return []Value{yes, no}
}

// ListSortBy is a stable sort on a copy.
func ListSortBy(fb *Fiber, cmp, xs Value) Value {
	out := append([]Value{}, List(xs)...)
	sort.SliceStable(out, func(i, j int) bool {
		return apply2(fb, cmp, out[i], out[j]).(*Data).Tag == LT.(*Data).Tag
	})
	return out
}

func ListLength(xs Value) Value { return int32(len(List(xs))) }

func ListGet(i, xs Value) Value {
	in, k := List(xs), int(i.(int32))
	if k < 0 || k >= len(in) {
		return None
	}
	return Some(in[k])
}

func ListSet(i, x, xs Value) Value {
	in, k := List(xs), int(i.(int32))
	if k < 0 || k >= len(in) {
		return xs
	}
	out := append([]Value{}, in...)
	out[k] = x
	return out
}

func ListSingleton(x Value) Value { return []Value{x} }

func ListRepeat(n, x Value) Value {
	out := make([]Value, max(0, int(n.(int32))))
	for i := range out {
		out[i] = x
	}
	return out
}

func ListRange(from, until Value) Value {
	a, b := from.(int32), until.(int32)
	out := make([]Value, 0, max(0, int(b)-int(a)))
	for x := a; x < b; x++ {
		out = append(out, x)
	}
	return out
}

func ListInitialize(fb *Fiber, n, f Value) Value {
	out := make([]Value, max(0, int(n.(int32))))
	for i := range out {
		out[i] = Apply(fb, f, int32(i))
	}
	return out
}

func ListReverse(xs Value) Value {
	in := List(xs)
	out := make([]Value, len(in))
	for i, x := range in {
		out[len(in)-1-i] = x
	}
	return out
}

func ListConcatAll(xss Value) Value {
	out := []Value{}
	for _, xs := range List(xss) {
		out = append(out, List(xs)...)
	}
	return out
}

// clip is JS `slice`'s reading of a position.
func clip(i int32, n int) int { return position(i, n, true) }

func ListTake(n, xs Value) Value {
	in := List(xs)
	return in[:max(0, min(int(n.(int32)), len(in)))]
}

func ListDrop(n, xs Value) Value {
	in := List(xs)
	return in[max(0, min(int(n.(int32)), len(in))):]
}

func ListSlice(start, end, xs Value) Value {
	in := List(xs)
	from, to := clip(start.(int32), len(in)), clip(end.(int32), len(in))
	if from >= to {
		return []Value{}
	}
	return in[from:to]
}

func ListZip(xs, ys Value) Value {
	a, b := List(xs), List(ys)
	n := min(len(a), len(b))
	out := make([]Value, n)
	for i := 0; i < n; i++ {
		out[i] = []Value{a[i], b[i]}
	}
	return out
}

func ListUnzip(pairs Value) Value {
	in := List(pairs)
	as, bs := make([]Value, len(in)), make([]Value, len(in))
	for i, p := range in {
		as[i], bs[i] = At(p, 0), At(p, 1)
	}
	return []Value{as, bs}
}
