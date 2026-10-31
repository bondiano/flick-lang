package flickrt

import "math"

// The model of Effect's Arbitrary (internal/arbitrary/model.ts and
// array.ts): a generated value with the lazy tree of its shrinks. A shrinkPull
// is a cursor over candidates, each a sample or a discard; Effect's pulls
// are effects, the go target's are synchronous, as the property and the
// filters run on the calling fiber.

// sample is a generated value and its shrinks, nil when it has none. A
// nil *sample is a discarded attempt.
type sample struct {
	value   Value
	shrinks shrinkPull
}

// shrinkPull is the next candidate; false when there are no more.
type shrinkPull func() (*sample, bool)

func donePull() (*sample, bool) { return nil, false }

// gen is a generator: Effect's Generator, and the Compiled node of a
// schema, whose cost and recursion the compiler settles.
type arbGen struct {
	minCost        float64
	recursive      bool
	mayRecurse     bool
	deps           []*arbGen
	computeMinCost func() float64
	generate       func(st *genState) *sample
}

func makeGen(minCost float64, generate func(*genState) *sample) *arbGen {
	return &arbGen{minCost: minCost, generate: generate}
}

// pullEach yields f of each of `xs` in order, made when pulled.
func pullEach[T any](xs []T, f func(T) *sample) shrinkPull {
	i := 0
	return func() (*sample, bool) {
		if i >= len(xs) {
			return nil, false
		}
		i++
		return f(xs[i-1]), true
	}
}

func concatPulls(pulls []shrinkPull) shrinkPull {
	i := 0
	return func() (*sample, bool) {
		for i < len(pulls) {
			if s, ok := pulls[i](); ok {
				return s, true
			}
			i++
		}
		return nil, false
	}
}

// lazyShrinks makes the shrinks when first pulled; they are never nil.
func lazyShrinks(value Value, make func() shrinkPull) *sample {
	var p shrinkPull
	return &sample{value, func() (*sample, bool) {
		if p == nil {
			if p = make(); p == nil {
				p = donePull
			}
		}
		return p()
	}}
}

// retained is a sample whose shrinks each caller walks with its own
// cursor over one shared, lazily filled history. A nil one is a discard.
type retained struct {
	value   Value
	shrinks func() func() (*retained, bool)
}

func retain(s *sample) *retained {
	if s.shrinks == nil {
		return &retained{s.value, nil}
	}
	var history []*retained
	ended := false
	return &retained{s.value, func() func() (*retained, bool) {
		i := 0
		return func() (*retained, bool) {
			if i < len(history) {
				i++
				return history[i-1], true
			}
			if ended {
				return nil, false
			}
			a, ok := s.shrinks()
			if !ok {
				ended = true
				return nil, false
			}
			var item *retained
			if a != nil {
				item = retain(a)
			}
			history = append(history, item)
			i++
			return item, true
		}
	}}
}

func fromRetained(r *retained) *sample {
	if r.shrinks == nil {
		return &sample{r.value, nil}
	}
	return &sample{r.value, mapRetained(r.shrinks(), fromRetained)}
}

// mapRetained is the samples `f` makes of a cursor's candidates.
func mapRetained(next func() (*retained, bool), f func(*retained) *sample) shrinkPull {
	return func() (*sample, bool) {
		r, ok := next()
		if !ok {
			return nil, false
		}
		if r == nil {
			return nil, true
		}
		return f(r), true
	}
}

// mapPull maps the samples of a shrinkPull, discards as they are.
func mapPull(p shrinkPull, f func(*sample) *sample) shrinkPull {
	return func() (*sample, bool) {
		s, ok := p()
		if !ok || s == nil {
			return nil, ok
		}
		return f(s), true
	}
}

func mapSample(s *sample, f func(Value) Value) *sample {
	out := &sample{f(s.value), nil}
	if s.shrinks != nil {
		out.shrinks = mapPull(s.shrinks, func(c *sample) *sample { return mapSample(c, f) })
	}
	return out
}

func values(children []*sample) []Value {
	out := make([]Value, len(children))
	for i, c := range children {
		out[i] = c.value
	}
	return out
}

func retainedValues(children []*retained) []Value {
	out := make([]Value, len(children))
	for i, c := range children {
		out[i] = c.value
	}
	return out
}

func replaceAt[T any](xs []T, i int, x T) []T {
	out := append([]T(nil), xs...)
	out[i] = x
	return out
}

// productSample shrinks each child in turn, never an earlier one again.
func productSample(children []*sample, make func([]Value) Value) *sample {
	value := make(values(children))
	any := false
	for _, c := range children {
		any = any || c.shrinks != nil
	}
	if !any {
		return &sample{value, nil}
	}
	var rebuild func(children []*retained, start int) *sample
	rebuild = func(children []*retained, start int) *sample {
		var pulls []shrinkPull
		for i := start; i < len(children); i++ {
			if children[i].shrinks == nil {
				continue
			}
			pulls = append(pulls, mapRetained(children[i].shrinks(), func(r *retained) *sample {
				return rebuild(replaceAt(children, i, r), i)
			}))
		}
		out := &sample{make(retainedValues(children)), nil}
		if len(pulls) > 0 {
			out.shrinks = concatPulls(pulls)
		}
		return out
	}
	return lazyShrinks(value, func() shrinkPull { return rebuild(retainAll(children), 0).shrinks })
}

func retainAll(children []*sample) []*retained {
	out := make([]*retained, len(children))
	for i, c := range children {
		out[i] = retain(c)
	}
	return out
}

// filterMapPull keeps the candidates `f` maps; a rejected one is a
// discard whose own shrinks are tried next.
func filterMapPull(source shrinkPull, f func(Value) (Value, bool)) shrinkPull {
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
			if v, keep := f(s.value); keep {
				out := &sample{v, nil}
				if s.shrinks != nil {
					out.shrinks = filterMapPull(s.shrinks, f)
				}
				return out, true
			}
			if s.shrinks != nil {
				stack = append(stack, s.shrinks)
			}
			return nil, true
		}
		return nil, false
	}
}

func filterMapSample(s *sample, f func(Value) (Value, bool)) *sample {
	v, keep := f(s.value)
	if !keep {
		return nil
	}
	out := &sample{v, nil}
	if s.shrinks != nil {
		out.shrinks = filterMapPull(s.shrinks, f)
	}
	return out
}

func filterSample(s *sample, keep func(Value) bool) *sample {
	if !keep(s.value) {
		return nil
	}
	if s.shrinks == nil {
		return s
	}
	return &sample{s.value, filterMapPull(s.shrinks, func(v Value) (Value, bool) { return v, keep(v) })}
}

// reserving generates `child` with `reserved` of the budget kept for the
// generators after it.
func reserving(child *arbGen, st *genState, reserved float64) *sample {
	if child.minCost+reserved > *st.budget {
		return nil
	}
	if reserved == 0 {
		return child.generate(st)
	}
	*st.budget -= reserved
	s := child.generate(st)
	*st.budget += reserved
	return s
}

// generateProduct generates each child in `order`, nil when one is
// discarded.
func generateProduct(children []*arbGen, st *genState, order []int, reserved float64) []*sample {
	if reserved > *st.budget {
		return nil
	}
	out := make([]*sample, len(children))
	for i := range children {
		at := i
		if order != nil {
			at = order[i]
		}
		reserved -= children[at].minCost
		s := reserving(children[at], st, reserved)
		if s == nil {
			return nil
		}
		out[at] = s
	}
	return out
}

// generateUnion picks an affordable member; a value of a costlier one
// first shrinks to a value of the cheapest member.
func generateUnion(members []*arbGen, st *genState) *sample {
	var eligible []*arbGen
	for _, m := range members {
		if m.minCost <= *st.budget {
			eligible = append(eligible, m)
		}
	}
	if len(eligible) == 0 {
		return nil
	}
	selected := eligible[st.index(len(eligible))]
	s := selected.generate(st)
	if !st.shrinks || s == nil {
		return s
	}
	fallback := members[0]
	for _, m := range members[1:] {
		if m.minCost < fallback.minCost {
			fallback = m
		}
	}
	if fallback.minCost >= selected.minCost {
		return s
	}
	random := st.random.clone()
	pulled := false
	fallbackPull := func() (*sample, bool) {
		if pulled {
			return nil, false
		}
		pulled = true
		c := *st
		budget := fallback.minCost
		c.random, c.budget = random, &budget
		return fallback.generate(&c), true
	}
	out := &sample{s.value, fallbackPull}
	if s.shrinks != nil {
		out.shrinks = concatPulls([]shrinkPull{fallbackPull, s.shrinks})
	}
	return out
}

// arrayShape is where the parts of a generated array are.
type arrayShape struct{ fixed, optional, repeat, tail, minimum int }

// rebuildArray removes optional elements, then runs of repeated ones,
// then shrinks each element from `start` on.
func rebuildArray(children []*retained, shape arrayShape, start int) *sample {
	index, removable, removableSet := 0, 0, false
	var items func() (*retained, bool)
	removableOptional := 0
	if shape.repeat == 0 && shape.tail == 0 {
		removableOptional = min(shape.optional, len(children)-shape.minimum)
	}
	repeatEnd := shape.fixed + shape.repeat
	next := func() (*sample, bool) {
		if removableOptional > 0 {
			count := removableOptional
			removableOptional /= 2
			s := shape
			s.fixed -= count
			s.optional -= count
			return rebuildArray(children[:len(children)-count], s, 0), true
		}
		for index < len(children) {
			if !removableSet {
				removable, removableSet = 0, true
				if index >= shape.fixed && index < repeatEnd {
					removable = min(repeatEnd-index, len(children)-shape.minimum)
				}
			}
			if removable > 0 {
				count := removable
				removable /= 2
				rest := append(append([]*retained(nil), children[:index]...), children[index+count:]...)
				s := shape
				s.repeat -= count
				return rebuildArray(rest, s, 0), true
			}
			if index >= start && items == nil && children[index].shrinks != nil {
				items = children[index].shrinks()
			}
			if items != nil {
				item, ok := items()
				if !ok {
					items, removableSet = nil, false
					index++
					continue
				}
				if item == nil {
					return nil, true
				}
				return rebuildArray(replaceAt(children, index, item), shape, index), true
			}
			index++
			removableSet = false
		}
		return nil, false
	}
	return &sample{retainedValues(children), next}
}

// arraySample is `Arrays.sample`: a list of the children's values.
func arraySample(children []*sample, shape arrayShape, shrinks bool) *sample {
	value := values(children)
	if !shrinks {
		return &sample{value, nil}
	}
	if len(children) <= shape.minimum || shape.repeat == 0 && shape.optional == 0 {
		return productSample(children, func(vs []Value) Value { return vs })
	}
	return lazyShrinks(value, func() shrinkPull { return rebuildArray(retainAll(children), shape, 0).shrinks })
}

var infinity = math.Inf(1)

func sumCosts(costs ...float64) float64 {
	out := 0.0
	for _, c := range costs {
		if c == infinity {
			return infinity
		}
		out += c
	}
	return out
}
