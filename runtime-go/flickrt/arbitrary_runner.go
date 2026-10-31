package flickrt

import (
	"math"
	"strconv"
	"strings"
)

// The combinators and the runner of Effect's Arbitrary
// (internal/arbitrary/runner.ts): attempts at progressive sizes, discards,
// shrinking of the first failure and the replay of it.

func constantGen(v Value) *arbGen {
	return makeGen(0, func(*genState) *sample { return &sample{v, nil} })
}

func mapGen(self *arbGen, f func(*genState, Value) Value) *arbGen {
	return makeGen(self.minCost, func(st *genState) *sample {
		s := self.generate(st)
		if s == nil {
			return nil
		}
		return mapSample(s, func(v Value) Value { return f(st, v) })
	})
}

func filterGen(self *arbGen, keep func(*genState, Value) bool) *arbGen {
	return makeGen(self.minCost, func(st *genState) *sample {
		s := self.generate(st)
		if s == nil {
			return nil
		}
		test := func(v Value) bool { return keep(st, v) }
		if !st.shrinks {
			if test(s.value) {
				return s
			}
			return nil
		}
		return filterSample(s, test)
	})
}

func filterMapGen(self *arbGen, f func(*genState, Value) (Value, bool)) *arbGen {
	return makeGen(self.minCost, func(st *genState) *sample {
		s := self.generate(st)
		if s == nil {
			return nil
		}
		apply := func(v Value) (Value, bool) { return f(st, v) }
		if st.shrinks {
			return filterMapSample(s, apply)
		}
		if v, ok := apply(s.value); ok {
			return &sample{v, nil}
		}
		return nil
	})
}

// arrayGen is `Arbitrary.array`.
func arrayGen(item *arbGen, minimum, maximum float64) *arbGen {
	if minimum != math.Trunc(minimum) || minimum < 0 || minimum > 0xffffffff ||
		maximum != math.Trunc(maximum) || maximum < minimum || maximum > 0xffffffff {
		panic(Defect{"RangeError: Arbitrary.array: expected 0 <= minLength <= maxLength <= 4294967295 with integer lengths"})
	}
	minCost := 0.0
	if minimum != 0 {
		minCost = minimum * item.minCost
	}
	return makeGen(minCost, func(st *genState) *sample {
		affordable := maximum
		if item.minCost > 0 {
			affordable = math.Floor(*st.budget / item.minCost)
		}
		upper := math.Min(maximum, math.Min(math.Max(minimum, st.size), affordable))
		if upper < minimum {
			return nil
		}
		length := int(st.randomLength(minimum, upper))
		children := make([]*arbGen, length)
		indexes := make([]int, length)
		for i := range children {
			children[i], indexes[i] = item, i
		}
		var order []int
		if length >= 2 {
			order = shuffle(st, indexes)
		}
		itemState := st
		if st.size < float64(length) {
			itemState = st.withSize(float64(length))
		}
		reserved := 0.0
		if length > 0 {
			reserved = float64(length) * item.minCost
		}
		samples := generateProduct(children, itemState, order, reserved)
		if samples == nil {
			return nil
		}
		return arraySample(samples, arrayShape{0, 0, length, 0, int(minimum)}, st.shrinks)
	})
}

// allGen is `Arbitrary.all` over a list: one value of each, as a list.
func allGen(members []*arbGen) *arbGen {
	minCost := 0.0
	indexes := make([]int, len(members))
	for i, m := range members {
		minCost += m.minCost
		indexes[i] = i
	}
	return makeGen(minCost, func(st *genState) *sample {
		var order []int
		if len(indexes) >= 2 {
			order = shuffle(st, indexes)
		}
		samples := generateProduct(members, st, order, minCost)
		if samples == nil {
			return nil
		}
		return productSample(samples, func(vs []Value) Value { return vs })
	})
}

func (st *genState) with(random *xoshiro, remaining float64) *genState {
	c := *st
	c.random, c.budget = random, &remaining
	return &c
}

// flatMapGen is `Arbitrary.flatMap`: the source shrinks first, and the
// dependent value is drawn again from where the source left the generator.
func flatMapGen(self *arbGen, f func(*genState, Value) *arbGen) *arbGen {
	return makeGen(self.minCost, func(st *genState) *sample {
		if !st.shrinks {
			source := self.generate(st)
			if source == nil {
				return nil
			}
			target := f(st, source.value)
			residual := *st.budget
			*st.budget = residual + target.minCost
			s := target.generate(st)
			*st.budget = math.Min(residual, *st.budget)
			return s
		}
		sourceState := st.with(st.random.clone(), *st.budget)
		source := self.generate(sourceState)
		if source == nil {
			*st.random = *sourceState.random
			*st.budget = *sourceState.budget
			return nil
		}
		target := f(st, source.value)
		residual := *sourceState.budget
		checkpoint := sourceState.random.clone()
		targetState := st.with(checkpoint.clone(), residual+target.minCost)
		s := target.generate(targetState)
		*st.random = *targetState.random
		*st.budget = math.Min(residual, *targetState.budget)
		if s == nil {
			return nil
		}
		return flatMapSample(source, s, f, checkpoint, st, residual)
	})
}

func flatMapSample(source, target *sample, f func(*genState, Value) *arbGen, checkpoint *xoshiro, st *genState, residual float64) *sample {
	if source.shrinks == nil {
		return target
	}
	sources := flatMapSourcePull(source.shrinks, f, checkpoint, st, residual)
	if target.shrinks == nil {
		return &sample{target.value, sources}
	}
	return &sample{target.value, concatPulls([]shrinkPull{sources, target.shrinks})}
}

func flatMapSourcePull(source shrinkPull, f func(*genState, Value) *arbGen, checkpoint *xoshiro, st *genState, residual float64) shrinkPull {
	stack := []shrinkPull{source}
	return func() (*sample, bool) {
		for len(stack) > 0 {
			s, ok := stack[len(stack)-1]()
			if !ok {
				stack = stack[:len(stack)-1]
				continue
			}
			if s == nil {
				return nil, true
			}
			target := f(st, s.value)
			t := target.generate(st.with(checkpoint.clone(), residual+target.minCost))
			if t == nil {
				if s.shrinks != nil {
					stack = append(stack, s.shrinks)
				}
				return nil, true
			}
			return flatMapSample(s, t, f, checkpoint, st, residual), true
		}
		return nil, false
	}
}

// generateAttempt is an attempt's value: its generator, size and bias.
func generateAttempt(fb *Fiber, g *arbGen, seed arbSeed, attempt, size float64, shrinks bool) *sample {
	bias := 2 + floorLog10(attempt+1)
	budget := g.minCost + size
	return g.generate(&genState{fb, size, shrinks, bias, attemptRandom(seed, attempt), &budget})
}

// arbCount is an option of a count: Effect's `natural`.
func arbCount(v Value, otherwise float64, label string, positive bool) float64 {
	out := otherwise
	if n, ok := some(v); ok {
		out = float64(n.(int32))
	}
	if out < 0 {
		panic(Defect{"Error: " + label + " must be a non-negative safe integer"})
	}
	if positive && out == 0 {
		panic(Defect{"Error: " + label + " must be greater than zero"})
	}
	return out
}

// arbSeedOf is the seed of the options, or one of the system's randomness.
func arbSeedOf(v Value) Value {
	if s, ok := some(v); ok {
		return s
	}
	return float64(int32(globalRandom().nextUint32()))
}

// arbSample is `Arbitrary.sample`: values of attempts without shrinks.
func arbSample(fb *Fiber, g *arbGen, options Value) []Value {
	o := record(options)
	count := arbCount(o.Get("count"), 10, "count", false)
	size := arbCount(o.Get("size"), 10, "size", false)
	maxDiscards := arbCount(o.Get("maxDiscards"), math.Max(100, count*10), "maxDiscards", false)
	seed := arbSeedOf(o.Get("seed"))
	hashed := hashSeed(seed)
	var out []Value
	discards, attempt := 0.0, 0.0
	for float64(len(out)) < count {
		s := generateAttempt(fb, g, hashed, attempt, size, false)
		attempt++
		if s != nil {
			out = append(out, s.value)
		} else if discards++; discards > maxDiscards {
			Fail(NewData(sampleErrorTag, Rec(
				"generated", int32(len(out)), "discards", int32(discards), "seed", jsString(seed))))
		}
	}
	if out == nil {
		out = []Value{}
	}
	return out
}

// propertyFailed is the tag of a failure, "" when the property passed.
func propertyFailed(failure Value) string {
	switch {
	case failure == nil:
		return ""
	case failure.(*Data).Tag == propertyErrorTag:
		return "PropertyError"
	}
	return "ReturnedFalse"
}

// shrinkFailure walks to the first candidate that fails the same way,
// at most `maximum` candidates in all; the path is the index of each.
func shrinkFailure(fb *Fiber, initial *sample, failure Value, property Value, maximum float64) (*sample, Value, []int) {
	current, tag := initial, propertyFailed(failure)
	inspected := 0.0
	path := []int{}
	for inspected < maximum {
		index := 0
		var found *sample
		for inspected < maximum && current.shrinks != nil {
			candidate, ok := current.shrinks()
			if !ok {
				break
			}
			inspected++
			if candidate == nil {
				continue
			}
			outcome := evaluate(fb, property, candidate.value)
			if propertyFailed(outcome) == tag {
				found, failure = candidate, outcome
				path = append(path, index)
				break
			}
			index++
		}
		if found == nil {
			break
		}
		current = found
	}
	return current, failure, path
}

// followReplay takes the recorded path of shrinks again.
func followReplay(fb *Fiber, initial *sample, failure Value, path []Value, property Value) (*sample, Value, string) {
	current, tag := initial, propertyFailed(failure)
	for _, at := range path {
		target := int(at.(float64))
		if current.shrinks == nil {
			return nil, nil, "ShrinkPathUnavailable"
		}
		var selected *sample
		for index := 0; index <= target; {
			candidate, ok := current.shrinks()
			if !ok {
				return nil, nil, "ShrinkPathUnavailable"
			}
			if candidate == nil {
				continue
			}
			if index == target {
				selected = candidate
			}
			index++
		}
		outcome := evaluate(fb, property, selected.value)
		if propertyFailed(outcome) != tag {
			return nil, nil, "ShrinkPassed"
		}
		current, failure = selected, outcome
	}
	return current, failure, ""
}

func makeReplay(seed Value, attempt, size float64, path []int, tag string) string {
	kind := "1"
	if _, ok := seed.(float64); ok {
		kind = "0"
	}
	steps := make([]string, len(path))
	for i, p := range path {
		steps[i] = strconv.Itoa(p)
	}
	return "[" + kind + "," + QuoteJSON(jsString(seed)) + "," + jsString(attempt) + "," + jsString(size) +
		",[" + strings.Join(steps, ",") + "]," + QuoteJSON(tag) + "]"
}

func falsified(initial, shrunk, failure Value, runs, discards float64, shrinks int, replay string) Value {
	return NewData(falsifiedTag, Rec(
		"initialInput", initial, "shrunkInput", shrunk, "failure", failure, "runs", int32(runs),
		"discards", int32(discards), "shrinks", int32(shrinks), "replay", replay,
	))
}

// arbCheck is `Arbitrary.check`, Effect's `checkEffect`.
func arbCheck(fb *Fiber, g *arbGen, property, options Value) Value {
	o := record(options)
	if replay, ok := some(o.Get("replay")); ok {
		return replayCheck(fb, g, replay.(string), property)
	}
	runsTarget := arbCount(o.Get("runs"), 100, "runs", true)
	size := arbCount(o.Get("size"), 10, "size", false)
	maxDiscards := arbCount(o.Get("maxDiscards"), math.Max(100, runsTarget*10), "maxDiscards", false)
	maxShrinks := arbCount(o.Get("maxShrinks"), 100, "maxShrinks", false)
	seed := arbSeedOf(o.Get("seed"))
	hashed := hashSeed(seed)
	runs, discards := 0.0, 0.0
	for attempt := 0.0; runs < runsTarget; attempt++ {
		currentSize := size
		if runsTarget != 1 {
			currentSize = math.Floor(runs*size/(runsTarget-1) + 0.5)
		}
		s := generateAttempt(fb, g, hashed, attempt, currentSize, true)
		if s == nil {
			if discards++; discards > maxDiscards {
				return NewData(exhaustedTag, Rec("runs", int32(runs), "discards", int32(discards), "seed", jsString(seed)))
			}
			continue
		}
		failure := evaluate(fb, property, s.value)
		if failure == nil {
			runs++
			continue
		}
		shrunk, last, path := shrinkFailure(fb, s, failure, property, maxShrinks)
		replay := makeReplay(seed, attempt, currentSize, path, propertyFailed(failure))
		return falsified(s.value, shrunk.value, last, runs+1, discards, len(path), replay)
	}
	return NewData(passedTag, Rec("runs", int32(runs), "discards", int32(discards)))
}

// replayCheck reruns the attempt a replay token records and its shrinks.
func replayCheck(fb *Fiber, g *arbGen, replay string, property Value) Value {
	data, reason := parseJSON(replay)
	parts, ok := data.([]Value)
	if reason != "" || !ok || len(parts) != 6 {
		panic(Defect{"SyntaxError: Invalid Arbitrary replay token"})
	}
	var seed Value = parts[1]
	if parts[0] == 0.0 {
		seed = jsNumber(parts[1])
	}
	path, _ := parts[4].([]Value)
	s := generateAttempt(fb, g, hashSeed(seed), parts[2].(float64), parts[3].(float64), true)
	if s == nil {
		return NewData(replayMismatchTag, "AttemptDiscarded")
	}
	failure := evaluate(fb, property, s.value)
	switch tag := propertyFailed(failure); {
	case tag == "":
		return NewData(replayMismatchTag, "PropertyPassed")
	case Value(tag) != parts[5]:
		return NewData(replayMismatchTag, "ShrinkPassed")
	}
	shrunk, last, mismatch := followReplay(fb, s, failure, path, property)
	if mismatch != "" {
		return NewData(replayMismatchTag, mismatch)
	}
	return falsified(s.value, shrunk.value, last, 1, 0, len(path), replay)
}
