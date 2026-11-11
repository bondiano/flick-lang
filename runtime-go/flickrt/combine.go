package flickrt

import (
	"math"
	"math/big"
)

// Combiner is a way to combine two values, and with an initial value a
// reducer, which is a Combiner too.
type Combiner struct {
	combine func(fb *Fiber, a, b Value) Value
	initial Value
	all     func(fb *Fiber, xs []Value) Value
}

func combiner(f func(*Fiber, Value, Value) Value) Value { return &Combiner{combine: f} }

func reducer(f func(*Fiber, Value, Value) Value, initial Value) *Combiner {
	r := &Combiner{combine: f, initial: initial}
	r.all = func(fb *Fiber, xs []Value) Value {
		out := initial
		for _, x := range xs {
			out = f(fb, out, x)
		}
		return out
	}
	return r
}

func comb(c Value) *Combiner { return c.(*Combiner) }

func CombinerMake(f Value) Value {
	return combiner(func(fb *Fiber, a, b Value) Value { return Apply2(fb, f, a, b) })
}
func CombinerCombine(fb *Fiber, c, a, b Value) Value { return comb(c).combine(fb, a, b) }

func CombinerFlip(c Value) Value {
	f := comb(c).combine
	return combiner(func(fb *Fiber, a, b Value) Value { return f(fb, b, a) })
}

func orderOf(cmp Value) func(*Fiber, Value, Value) int {
	return func(fb *Fiber, a, b Value) int {
		switch Apply2(fb, cmp, a, b).(*Data).Tag {
		case LT.(*Data).Tag:
			return -1
		case GT.(*Data).Tag:
			return 1
		}
		return 0
	}
}

func CombinerMin(cmp Value) Value {
	o := orderOf(cmp)
	return combiner(func(fb *Fiber, a, b Value) Value {
		if o(fb, a, b) == -1 {
			return a
		}
		return b
	})
}

func CombinerMax(cmp Value) Value {
	o := orderOf(cmp)
	return combiner(func(fb *Fiber, a, b Value) Value {
		if o(fb, a, b) == 1 {
			return a
		}
		return b
	})
}

func CombinerFirst() Value           { return combiner(func(_ *Fiber, a, _ Value) Value { return a }) }
func CombinerLast() Value            { return combiner(func(_ *Fiber, _, b Value) Value { return b }) }
func CombinerConstant(x Value) Value { return combiner(func(_ *Fiber, _, _ Value) Value { return x }) }

func CombinerIntercalate(middle, c Value) Value {
	f := comb(c).combine
	return combiner(func(fb *Fiber, a, b Value) Value { return f(fb, a, f(fb, middle, b)) })
}

func bigCombiner(pick int) Value {
	return combiner(func(_ *Fiber, a, b Value) Value {
		if bigOrder(a, b) == pick {
			return a
		}
		return b
	})
}

var (
	BigIntCombinerMax Value = bigCombiner(1)
	BigIntCombinerMin Value = bigCombiner(-1)
)

func OptionCombinerFailFast(c Value) Value {
	f := comb(c).combine
	return combiner(func(fb *Fiber, a, b Value) Value { return failFast(fb, f, a, b) })
}

func failFast(fb *Fiber, f func(*Fiber, Value, Value) Value, a, b Value) Value {
	x, y := a.(*Data), b.(*Data)
	if x.Tag == "None" || y.Tag == "None" {
		return None
	}
	return Some(f(fb, x.Fields[0], y.Fields[0]))
}

// StructCombiner combines records field by field, as `combiners` lists
// the fields.
func StructCombiner(_, combiners Value) Value {
	cs := record(combiners)
	return combiner(func(fb *Fiber, a, b Value) Value {
		x, y := record(a), record(b)
		out := &Record{}
		for i, k := range cs.keys {
			out = out.set(k, comb(cs.vals[i]).combine(fb, x.Get(k), y.Get(k)))
		}
		return out
	})
}

func StructReducer(d, reducers Value) Value {
	rs := record(reducers)
	initial := &Record{}
	for i, k := range rs.keys {
		initial = initial.set(k, comb(rs.vals[i]).initial)
	}
	return reducer(comb(StructCombiner(d, reducers)).combine, initial)
}

func TupleCombiner(combiners Value) Value {
	cs := List(combiners)
	return combiner(func(fb *Fiber, a, b Value) Value {
		x, y := List(a), List(b)
		out := make([]Value, len(x))
		for i := range x {
			out[i] = comb(cs[i]).combine(fb, x[i], y[i])
		}
		return out
	})
}

func TupleReducer(reducers Value) Value {
	rs := List(reducers)
	initial := make([]Value, len(rs))
	for i, r := range rs {
		initial[i] = comb(r).initial
	}
	return reducer(comb(TupleCombiner(reducers)).combine, initial)
}

func ReducerMake(f, initial Value) Value {
	return reducer(func(fb *Fiber, a, b Value) Value { return Apply2(fb, f, a, b) }, initial)
}

func ReducerInitialValue(r Value) Value              { return comb(r).initial }
func ReducerCombineAll(fb *Fiber, r, xs Value) Value { return comb(r).all(fb, List(xs)) }
func ReducerToCombiner(r Value) Value                { return r }

func ReducerFlip(r Value) Value {
	f := comb(r).combine
	return reducer(func(fb *Fiber, a, b Value) Value { return f(fb, b, a) }, comb(r).initial)
}

var (
	NumberReducerSum Value = reducer(func(_ *Fiber, a, b Value) Value { return a.(float64) + b.(float64) }, 0.0)
	NumberReducerMax Value = reducer(func(_ *Fiber, a, b Value) Value { return jsMax(a.(float64), b.(float64)) }, math.Inf(-1))
	NumberReducerMin Value = reducer(func(_ *Fiber, a, b Value) Value { return jsMin(a.(float64), b.(float64)) }, math.Inf(1))
	BigIntReducerSum Value = reducer(func(_ *Fiber, a, b Value) Value { return BigIntSum(a, b) }, new(big.Int))
	StringConcat     Value = reducer(func(_ *Fiber, a, b Value) Value { return Concat(a, b) }, "")
	BooleanAnd       Value = reducer(func(_ *Fiber, a, b Value) Value { return a.(bool) && b.(bool) }, true)
	BooleanOr        Value = reducer(func(_ *Fiber, a, b Value) Value { return a.(bool) || b.(bool) }, false)
)

// The multiplying reducers stop at a zero.
var (
	NumberReducerMultiply Value = func() Value {
		r := reducer(func(_ *Fiber, a, b Value) Value { return a.(float64) * b.(float64) }, 1.0)
		r.all = func(_ *Fiber, xs []Value) Value { return NumberMultiplyAll(xs) }
		return r
	}()
	BigIntReducerMultiply Value = func() Value {
		r := reducer(func(_ *Fiber, a, b Value) Value { return BigIntMultiply(a, b) }, big.NewInt(1))
		r.all = func(_ *Fiber, xs []Value) Value { return BigIntMultiplyAll(xs) }
		return r
	}()
)

// jsMax and jsMin are JS `Math.max` and `Math.min`: NaN wins, and +0 is
// above -0.
func jsMax(a, b float64) float64 {
	if a != a || b != b {
		return math.NaN()
	}
	return math.Max(a, b)
}

func jsMin(a, b float64) float64 {
	if a != a || b != b {
		return math.NaN()
	}
	return math.Min(a, b)
}

func ListReducerConcat() Value {
	return reducer(func(_ *Fiber, a, b Value) Value { return ListConcat(a, b) }, []Value{})
}

// OptionReducer combines the values of the `Some`s, a `None` changing
// nothing.
func OptionReducer(c Value) Value {
	f := comb(c).combine
	return reducer(func(fb *Fiber, a, b Value) Value {
		x, y := a.(*Data), b.(*Data)
		switch {
		case x.Tag == "None":
			return b
		case y.Tag == "None":
			return a
		}
		return Some(f(fb, x.Fields[0], y.Fields[0]))
	}, None)
}

// OptionReducerFailFast is `None` as soon as one is.
func OptionReducerFailFast(r Value) Value {
	f := comb(r).combine
	out := reducer(func(fb *Fiber, a, b Value) Value { return failFast(fb, f, a, b) }, Some(comb(r).initial))
	out.all = func(fb *Fiber, xs []Value) Value {
		acc := out.initial
		for _, x := range xs {
			if acc = failFast(fb, f, acc, x); acc == None {
				return acc
			}
		}
		return acc
	}
	return out
}

// RecordReducerUnion merges records, combining the values of a label both
// have; RecordReducerIntersection keeps only those.
func RecordReducerUnion(c Value) Value {
	f := comb(c).combine
	return reducer(func(fb *Fiber, a, b Value) Value {
		x, y := record(a), record(b)
		if len(x.keys) == 0 {
			return y
		}
		out := &Record{}
		for i, k := range x.keys {
			v := x.vals[i]
			if j := y.index(k); j >= 0 {
				v = f(fb, v, y.vals[j])
			}
			out = out.set(k, v)
		}
		for j, k := range y.keys {
			if out.index(k) < 0 {
				out = out.set(k, y.vals[j])
			}
		}
		return out
	}, &Record{})
}

func RecordReducerIntersection(c Value) Value {
	f := comb(c).combine
	return reducer(func(fb *Fiber, a, b Value) Value {
		x, y := record(a), record(b)
		out := &Record{}
		for i, k := range x.keys {
			if j := y.index(k); j >= 0 {
				out = out.set(k, f(fb, x.vals[i], y.vals[j]))
			}
		}
		return out
	}, &Record{})
}

// HashCombine combines two hashes, data-last.
func HashCombine(b, self Value) Value { return effectCombine(self.(int32), b.(int32)) }
func HashOptimize(n Value) Value      { return optimize(n.(int32)) }
