package flickrt

import (
	"iter"
	"math"
)

// Seq is a lazy sequence that starts over on every iteration, so it stays
// a value: the state of an iteration lives inside the function. A list is
// a sequence too (see Iter).
type Seq func(yield func(Value) bool)

// Iter is the elements of a List or a Seq, for a `for` to range over.
func Iter(v Value) iter.Seq[Value] {
	switch s := v.(type) {
	case Seq:
		return iter.Seq[Value](s)
	case []Value:
		return func(yield func(Value) bool) {
			for _, x := range s {
				if !yield(x) {
					return
				}
			}
		}
	}
	panic(Defect{"not a sequence"})
}

func seq(f func(yield func(Value) bool)) Value { return Seq(f) }

func SeqOfList(xs Value) Value { return xs }

func SeqFrom(n Value) Value {
	return seq(func(yield func(Value) bool) {
		for i := n.(int32); yield(i); i++ {
		}
	})
}

// SeqRangeBy runs from `start` by `step` before `end`, either way; a zero
// step is empty.
func SeqRangeBy(start, end, step Value) Value {
	a, b, d := start.(int32), end.(int32), step.(int32)
	return seq(func(yield func(Value) bool) {
		if d == 0 {
			return
		}
		for i := int64(a); d > 0 && i < int64(b) || d < 0 && i > int64(b); i += int64(d) {
			if !yield(int32(i)) {
				return
			}
		}
	})
}

func SeqRange(start, end Value) Value { return SeqRangeBy(start, end, int32(1)) }

func SeqRangeTo(start, end Value) Value {
	a, b := start.(int32), end.(int32)
	return seq(func(yield func(Value) bool) {
		for i := int64(a); i <= int64(b); i++ {
			if !yield(int32(i)) {
				return
			}
		}
	})
}

func SeqIterate(f, x Value) Value {
	return seq(func(yield func(Value) bool) {
		for v := x; yield(v); v = Apply(f, v) {
		}
	})
}

func SeqRepeat(x Value) Value {
	return seq(func(yield func(Value) bool) {
		for yield(x) {
		}
	})
}

func SeqCycle(xs Value) Value {
	list := List(xs)
	return seq(func(yield func(Value) bool) {
		for len(list) > 0 {
			for _, x := range list {
				if !yield(x) {
					return
				}
			}
		}
	})
}

func SeqUnfold(f, s Value) Value {
	return seq(func(yield func(Value) bool) {
		for o := Apply(f, s).(*Data); o.Tag == "Some"; {
			pair := o.Fields[0].([]Value)
			if !yield(pair[0]) {
				return
			}
			o = Apply(f, pair[1]).(*Data)
		}
	})
}

func SeqAppend(xs, ys Value) Value {
	return seq(func(yield func(Value) bool) {
		for x := range Iter(xs) {
			if !yield(x) {
				return
			}
		}
		for y := range Iter(ys) {
			if !yield(y) {
				return
			}
		}
	})
}

// SeqMap2 is `Applicative.map2` of sequences: every pair, the first
// sequence outermost.
func SeqMap2(f, xs, ys Value) Value {
	return seq(func(yield func(Value) bool) {
		for x := range Iter(xs) {
			for y := range Iter(ys) {
				if !yield(apply2(f, x, y)) {
					return
				}
			}
		}
	})
}

func SeqMap(f, xs Value) Value {
	return seq(func(yield func(Value) bool) {
		for x := range Iter(xs) {
			if !yield(Apply(f, x)) {
				return
			}
		}
	})
}

func SeqFilter(p, xs Value) Value {
	return seq(func(yield func(Value) bool) {
		for x := range Iter(xs) {
			if Bool(Apply(p, x)) && !yield(x) {
				return
			}
		}
	})
}

func SeqFilterMap(f, xs Value) Value {
	return seq(func(yield func(Value) bool) {
		for x := range Iter(xs) {
			if o := Apply(f, x).(*Data); o.Tag == "Some" && !yield(o.Fields[0]) {
				return
			}
		}
	})
}

func SeqFlatMap(f, xs Value) Value {
	return seq(func(yield func(Value) bool) {
		for x := range Iter(xs) {
			for y := range Iter(Apply(f, x)) {
				if !yield(y) {
					return
				}
			}
		}
	})
}

// SeqTake is at most `n` elements, pulling none after the last.
func SeqTake(n, xs Value) Value {
	return seq(func(yield func(Value) bool) {
		left := n.(int32)
		if left <= 0 {
			return
		}
		for x := range Iter(xs) {
			if !yield(x) {
				return
			}
			if left--; left == 0 {
				return
			}
		}
	})
}

func SeqTakeWhile(p, xs Value) Value {
	return seq(func(yield func(Value) bool) {
		for x := range Iter(xs) {
			if !Bool(Apply(p, x)) || !yield(x) {
				return
			}
		}
	})
}

func SeqDrop(n, xs Value) Value {
	return seq(func(yield func(Value) bool) {
		skip := n.(int32)
		for x := range Iter(xs) {
			if skip > 0 {
				skip--
			} else if !yield(x) {
				return
			}
		}
	})
}

func SeqDropWhile(p, xs Value) Value {
	return seq(func(yield func(Value) bool) {
		dropping := true
		for x := range Iter(xs) {
			if dropping && Bool(Apply(p, x)) {
				continue
			}
			dropping = false
			if !yield(x) {
				return
			}
		}
	})
}

func SeqZipWith(f, xs, ys Value) Value {
	return seq(func(yield func(Value) bool) {
		next, stop := iter.Pull(Iter(ys))
		defer stop()
		for x := range Iter(xs) {
			y, ok := next()
			if !ok || !yield(apply2(f, x, y)) {
				return
			}
		}
	})
}

func SeqZip(xs, ys Value) Value {
	return SeqZipWith(Curry2(func(a, b Value) Value { return []Value{a, b} }), xs, ys)
}

func SeqEnumerate(xs Value) Value { return SeqZip(SeqFrom(int32(0)), xs) }

// SeqScan is `init`, then each running fold of the elements.
func SeqScan(f, init, xs Value) Value {
	return seq(func(yield func(Value) bool) {
		acc := init
		if !yield(acc) {
			return
		}
		for x := range Iter(xs) {
			if acc = apply2(f, acc, x); !yield(acc) {
				return
			}
		}
	})
}

// SeqChunks is lists of `n` elements, the last one shorter when the
// elements run out.
func SeqChunks(n, xs Value) Value {
	size := int(n.(int32))
	return seq(func(yield func(Value) bool) {
		if size <= 0 {
			return
		}
		chunk := []Value{}
		for x := range Iter(xs) {
			if chunk = append(chunk, x); len(chunk) == size {
				if !yield(chunk) {
					return
				}
				chunk = []Value{}
			}
		}
		if len(chunk) > 0 {
			yield(chunk)
		}
	})
}

// SeqWindows is every run of `n` neighbouring elements, each a new list.
func SeqWindows(n, xs Value) Value {
	size := int(n.(int32))
	return seq(func(yield func(Value) bool) {
		if size <= 0 {
			return
		}
		var window []Value
		for x := range Iter(xs) {
			if window = append(window, x); len(window) > size {
				window = window[1:]
			}
			if len(window) == size && !yield(append([]Value{}, window...)) {
				return
			}
		}
	})
}

func SeqToList(xs Value) Value {
	out := []Value{}
	for x := range Iter(xs) {
		out = append(out, x)
	}
	return out
}

func SeqFold(f, init, xs Value) Value {
	acc := init
	for x := range Iter(xs) {
		acc = apply2(f, acc, x)
	}
	return acc
}

func SeqAny(p, xs Value) Value {
	for x := range Iter(xs) {
		if Bool(Apply(p, x)) {
			return true
		}
	}
	return false
}

func SeqAll(p, xs Value) Value {
	for x := range Iter(xs) {
		if !Bool(Apply(p, x)) {
			return false
		}
	}
	return true
}

func SeqFind(p, xs Value) Value {
	for x := range Iter(xs) {
		if Bool(Apply(p, x)) {
			return Some(x)
		}
	}
	return None
}

func SeqFirst(xs Value) Value {
	for x := range Iter(xs) {
		return Some(x)
	}
	return None
}

// SeqEachEffect runs the task of `f` of each element.
func SeqEachEffect(fb *Fiber, f, xs Value) Value {
	for x := range Iter(xs) {
		Run(fb, Apply(f, x))
	}
	return Unit
}

// SeqFoldEffect folds the elements with the tasks of `f`.
func SeqFoldEffect(fb *Fiber, f, init, xs Value) Value {
	acc := init
	for x := range Iter(xs) {
		acc = Run(fb, apply2(f, acc, x))
	}
	return acc
}

// Count is a count of elements, as `Seq.take` truncates it.
func Count(n Value) Value {
	if f, ok := n.(float64); ok {
		return int32(math.Trunc(f))
	}
	return n
}

// CountOf is a count as a Go int.
func CountOf(n Value) int {
	if f, ok := n.(float64); ok {
		if f != f {
			return 0
		}
		return int(f)
	}
	return int(n.(int32))
}

// StreamFromSeq is the Stream of a Seq's elements.
//
// shortcut: a Stream is the Seq it was made from, the only Stream the go target builds; a real one comes with Stream
func StreamFromSeq(s Value) Value { return s }
