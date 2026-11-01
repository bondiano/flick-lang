package flickrt

// Sink's folds, takes and loops, as Effect's (Sink.ts): each reads chunks
// until it is done or its upstream is, the rest of the chunk it stopped
// in left over.

// untilDone runs `each` until it or the upstream it pulls halts: the done
// value.
func untilDone(fb *Fiber, each func(fb *Fiber)) Value {
	return catchDone(fb, func(fb *Fiber) Value {
		for {
			each(fb)
		}
	}, func(_ *Fiber, d Value) Value { return d })
}

// rest is the elements after `i`, nil when there are none.
func rest(arr []Value, i int) []Value {
	if i+1 < len(arr) {
		return arr[i+1:]
	}
	return nil
}

// sinkFold folds elements while `cont` holds of the state.
func sinkFold(initial func(fb *Fiber) Value, cont func(fb *Fiber, s Value) bool, f func(fb *Fiber, s, x Value) Value) *Sink {
	return sinkFromTransform(func(fb *Fiber, upstream Pull, _ *Owner) (Value, []Value) {
		state := initial(fb)
		var leftover []Value
		untilDone(fb, func(fb *Fiber) {
			arr := upstream(fb).([]Value)
			for i, x := range arr {
				state = f(fb, state, x)
				if !cont(fb, state) {
					leftover = rest(arr, i)
					halt(Unit)
				}
			}
		})
		return state, leftover
	})
}

// sinkFoldArray folds chunks while `cont` holds of the state.
func sinkFoldArray(initial func(fb *Fiber) Value, cont func(fb *Fiber, s Value) bool, f func(fb *Fiber, s, chunk Value) Value) *Sink {
	return sinkFromTransform(func(fb *Fiber, upstream Pull, _ *Owner) (Value, []Value) {
		state := initial(fb)
		untilDone(fb, func(fb *Fiber) {
			state = f(fb, state, upstream(fb))
			if !cont(fb, state) {
				halt(Unit)
			}
		})
		return state, nil
	})
}

// sinkReduceWhile is Effect's `reduceWhile` and `reduceWhileEffect`: it
// checks the initial state too.
func sinkReduceWhile(initial func(fb *Fiber) Value, pred func(fb *Fiber, s Value) bool, f func(fb *Fiber, s, x Value) Value) *Sink {
	return sinkFromTransform(func(fb *Fiber, upstream Pull, _ *Owner) (Value, []Value) {
		state := initial(fb)
		if !pred(fb, state) {
			return state, nil
		}
		var leftover []Value
		untilDone(fb, func(fb *Fiber) {
			arr := upstream(fb).([]Value)
			for i, x := range arr {
				state = f(fb, state, x)
				if !pred(fb, state) {
					leftover = rest(arr, i)
					halt(Unit)
				}
			}
		})
		return state, leftover
	})
}

func sinkReduceWhileArray(initial func(fb *Fiber) Value, pred func(fb *Fiber, s Value) bool, f func(fb *Fiber, s, chunk Value) Value) *Sink {
	return sinkFromTransform(func(fb *Fiber, upstream Pull, _ *Owner) (Value, []Value) {
		state := initial(fb)
		if !pred(fb, state) {
			return state, nil
		}
		untilDone(fb, func(fb *Fiber) {
			state = f(fb, state, upstream(fb))
			if !pred(fb, state) {
				halt(Unit)
			}
		})
		return state, nil
	})
}

func sinkReduceArray(initial func(fb *Fiber) Value, f func(fb *Fiber, s, chunk Value) Value) *Sink {
	return sinkFromTransform(func(fb *Fiber, upstream Pull, _ *Owner) (Value, []Value) {
		state := initial(fb)
		untilDone(fb, func(fb *Fiber) { state = f(fb, state, upstream(fb)) })
		return state, nil
	})
}

// sinkTakeWhile takes the elements `f` passes, up to the first it does
// not; with `inclusive`, that one too.
func sinkTakeWhile(f func(fb *Fiber, x Value) (Value, bool), inclusive bool) *Sink {
	return sinkFromTransform(func(fb *Fiber, upstream Pull, _ *Owner) (Value, []Value) {
		out := []Value{}
		var leftover []Value
		untilDone(fb, func(fb *Fiber) {
			arr := upstream(fb).([]Value)
			for i, x := range arr {
				b, ok := f(fb, x)
				if ok || inclusive {
					out = append(out, b)
				}
				if ok == inclusive {
					leftover = rest(arr, i)
					halt(Unit)
				}
			}
		})
		return out, leftover
	})
}

// sinkForEachWhileArray runs `f` of each chunk while it gives true.
func sinkForEachWhileArray(f func(fb *Fiber, chunk []Value) bool) *Sink {
	return sinkFromTransform(func(fb *Fiber, upstream Pull, _ *Owner) (Value, []Value) {
		untilDone(fb, func(fb *Fiber) {
			if !f(fb, upstream(fb).([]Value)) {
				halt(Unit)
			}
		})
		return Unit, nil
	})
}

func sinkForEachArray(f func(fb *Fiber, chunk []Value)) *Sink {
	return sinkForEachWhileArray(func(fb *Fiber, chunk []Value) bool { f(fb, chunk); return true })
}

// The bindings.

func lazy(f Value) func(fb *Fiber) Value { return func(fb *Fiber) Value { return Apply(fb, f, Unit) } }

func holds(f Value) func(fb *Fiber, s Value) bool {
	return func(fb *Fiber, s Value) bool { return Apply(fb, f, s).(bool) }
}

func step2(f Value) func(fb *Fiber, s, x Value) Value {
	return func(fb *Fiber, s, x Value) Value { return apply2(fb, f, s, x) }
}

func step2Effect(f Value) func(fb *Fiber, s, x Value) Value {
	return func(fb *Fiber, s, x Value) Value { return Run(fb, apply2(fb, f, s, x)) }
}

func always(*Fiber, Value) bool { return true }

func isNone(_ *Fiber, s Value) bool { return s.(*Data).Tag == "None" }

func SinkFold(initial, cont, f Value) Value {
	return sinkFold(lazy(initial), holds(cont), step2Effect(f))
}

func SinkFoldArray(initial, cont, f Value) Value {
	return sinkFoldArray(lazy(initial), holds(cont), step2Effect(f))
}

// SinkFoldUntil folds at most `max` elements.
func SinkFoldUntil(initial, max, f Value) Value {
	count := max.(int32)
	if count <= 0 {
		return SinkSync(initial)
	}
	folded := sinkFold(func(fb *Fiber) Value { return []Value{Apply(fb, initial, Unit), int32(0)} },
		func(_ *Fiber, s Value) bool { return s.([]Value)[1].(int32) < count },
		func(fb *Fiber, s, x Value) Value {
			pair := s.([]Value)
			return []Value{Run(fb, apply2(fb, f, pair[0], x)), pair[1].(int32) + 1}
		})
	return sinkMapEnd(folded, func(_ *Fiber, v Value, l []Value) (Value, []Value) { return v.([]Value)[0], l })
}

func SinkEvery(f Value) Value {
	return sinkFold(func(*Fiber) Value { return true }, func(_ *Fiber, s Value) bool { return s.(bool) },
		func(fb *Fiber, _, x Value) Value { return Apply(fb, f, x) })
}

func SinkSome(f Value) Value {
	return sinkFold(func(*Fiber) Value { return false }, func(_ *Fiber, s Value) bool { return !s.(bool) },
		func(fb *Fiber, _, x Value) Value { return Apply(fb, f, x) })
}

// SinkTake takes `n` elements.
func SinkTake(n Value) Value {
	count := max(int(n.(int32)), 0)
	return sinkFromTransform(func(fb *Fiber, upstream Pull, _ *Owner) (Value, []Value) {
		taken := []Value{}
		if count == 0 {
			return taken, nil
		}
		var leftover []Value
		untilDone(fb, func(fb *Fiber) {
			arr := upstream(fb).([]Value)
			if len(taken)+len(arr) <= count {
				if taken = append(taken, arr...); len(taken) == count {
					halt(Unit)
				}
				return
			}
			for i, x := range arr {
				if taken = append(taken, x); len(taken) == count {
					leftover = rest(arr, i)
					halt(Unit)
				}
			}
		})
		return taken, leftover
	})
}

func SinkReduceWhile(initial, pred, f Value) Value {
	return sinkReduceWhile(lazy(initial), holds(pred), step2(f))
}

func SinkReduceWhileEffect(initial, pred, f Value) Value {
	return sinkReduceWhile(lazy(initial), holds(pred), step2Effect(f))
}

func SinkReduceWhileArray(initial, pred, f Value) Value {
	return sinkReduceWhileArray(lazy(initial), holds(pred), step2(f))
}

func SinkReduceWhileArrayEffect(initial, pred, f Value) Value {
	return sinkReduceWhileArray(lazy(initial), holds(pred), step2Effect(f))
}

func SinkReduce(initial, f Value) Value {
	return sinkReduceArray(lazy(initial), func(fb *Fiber, s, chunk Value) Value {
		for _, x := range chunk.([]Value) {
			s = apply2(fb, f, s, x)
		}
		return s
	})
}

func SinkReduceArray(initial, f Value) Value { return sinkReduceArray(lazy(initial), step2(f)) }

func SinkReduceEffect(initial, f Value) Value {
	return sinkReduceWhile(lazy(initial), always, step2Effect(f))
}

func noneYet(*Fiber) Value { return None }

func SinkHead() Value {
	return sinkReduceWhile(noneYet, isNone, func(_ *Fiber, _, x Value) Value { return Some(x) })
}

func SinkLast() Value {
	return sinkReduceArray(noneYet, func(_ *Fiber, _, chunk Value) Value {
		arr := chunk.([]Value)
		return Some(arr[len(arr)-1])
	})
}

func SinkFind(f Value) Value {
	return sinkReduceWhile(noneYet, isNone, func(fb *Fiber, acc, x Value) Value {
		if Apply(fb, f, x).(bool) {
			return Some(x)
		}
		return acc
	})
}

func SinkFindEffect(f Value) Value {
	return sinkReduceWhile(noneYet, isNone, func(fb *Fiber, acc, x Value) Value {
		if Run(fb, Apply(fb, f, x)).(bool) {
			return Some(x)
		}
		return acc
	})
}

var SinkSum Value = sinkReduceArray(func(*Fiber) Value { return 0.0 }, func(_ *Fiber, s, chunk Value) Value {
	total := s.(float64)
	for _, x := range chunk.([]Value) {
		total += x.(float64)
	}
	return total
})

var SinkCount Value = sinkReduceArray(func(*Fiber) Value { return int32(0) }, func(_ *Fiber, s, chunk Value) Value {
	return s.(int32) + int32(len(chunk.([]Value)))
})

func SinkCollect() Value {
	return sinkReduceArray(func(*Fiber) Value { return []Value{} }, func(_ *Fiber, s, chunk Value) Value {
		return append(s.([]Value), chunk.([]Value)...)
	})
}

func SinkTakeWhile(f Value) Value { return sinkTakeWhile(predicate(f), false) }

func SinkTakeWhileFilter(f Value) Value { return sinkTakeWhile(filterOf(f), false) }

func SinkTakeWhileEffect(f Value) Value { return sinkTakeWhile(predicateEffect(f), false) }

func SinkTakeWhileFilterEffect(f Value) Value { return sinkTakeWhile(filterEffectOf(f), false) }

func SinkTakeUntil(f Value) Value { return sinkTakeWhile(predicate(f), true) }

func SinkTakeUntilEffect(f Value) Value { return sinkTakeWhile(predicateEffect(f), true) }

func SinkForEach(f Value) Value {
	return sinkForEachArray(func(fb *Fiber, chunk []Value) {
		for _, x := range chunk {
			Run(fb, Apply(fb, f, x))
		}
	})
}

func SinkForEachArray(f Value) Value {
	return sinkForEachArray(func(fb *Fiber, chunk []Value) { Run(fb, Apply(fb, f, chunk)) })
}

func SinkForEachWhile(f Value) Value {
	return sinkForEachWhileArray(func(fb *Fiber, chunk []Value) bool {
		for _, x := range chunk {
			if !Run(fb, Apply(fb, f, x)).(bool) {
				return false
			}
		}
		return true
	})
}

func SinkForEachWhileArray(f Value) Value {
	return sinkForEachWhileArray(func(fb *Fiber, chunk []Value) bool { return Run(fb, Apply(fb, f, chunk)).(bool) })
}
