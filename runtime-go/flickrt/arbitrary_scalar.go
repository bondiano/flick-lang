package flickrt

import (
	"math"
	"math/big"
)

// Numbers and strings of Arbitrary: within the bounds their checks state,
// edge cases first, shrinking toward zero or the empty string.

// numShrink is a candidate and the closest value known to pass.
type numShrink struct {
	value, context float64
	has            bool
}

func shrinkInteger(current, target float64, asap bool) []numShrink {
	var out []numShrink
	gap := current - target
	previous, has := target, !asap
	toRemove := math.Trunc(gap / 2)
	if asap {
		toRemove = gap
	}
	for ; toRemove != 0; toRemove = math.Trunc(toRemove / 2) {
		v := current - toRemove
		if toRemove == gap {
			v = target
		}
		out = append(out, numShrink{v, previous, has})
		previous, has = v, true
	}
	return out
}

// shrinkNumber halves the distance of the doubles' indexes.
func shrinkNumber(current, target float64, asap bool) []numShrink {
	if math.IsNaN(current) {
		return []numShrink{{value: target}}
	}
	at := big.NewInt(numberIndex(current))
	gap := new(big.Int).Sub(at, big.NewInt(numberIndex(target)))
	previous, has := target, !asap
	toRemove := new(big.Int).Quo(gap, big.NewInt(2))
	if asap {
		toRemove.Set(gap)
	}
	var out []numShrink
	for toRemove.Sign() != 0 {
		v := target
		if toRemove.Cmp(gap) != 0 {
			v = indexNumber(new(big.Int).Sub(at, toRemove).Int64())
		}
		out = append(out, numShrink{v, previous, has})
		previous, has = v, true
		toRemove.Quo(toRemove, big.NewInt(2))
	}
	return out
}

// numberSample is a number and its shrinks, which keep the bounds.
func numberSample(value float64, minimum, maximum *float64, integer bool, context *numShrink) *sample {
	var candidates []numShrink
	switch {
	case !integer && context == nil:
		target := 0.0
		if minimum != nil && *minimum > 0 {
			target = *minimum
		} else if maximum != nil && *maximum < 0 {
			target = *maximum
		}
		candidates = shrinkNumber(value, target, true)
	case !integer:
		if !math.IsNaN(value) && (numberIndex(value) == numberIndex(context.context)+1 || numberIndex(value) == numberIndex(context.context)-1) {
			candidates = []numShrink{{value: context.context}}
		} else {
			candidates = shrinkNumber(value, context.context, false)
		}
	case context == nil:
		lo, hi := 0.0, 0.0
		if minimum != nil {
			lo = *minimum
		}
		if maximum != nil {
			hi = *maximum
		}
		candidates = shrinkInteger(value, math.Min(hi, math.Max(lo, 0)), true)
	case value > 0 && value == context.context+1 && (minimum == nil || value > *minimum) ||
		value < 0 && value == context.context-1 && (maximum == nil || value < *maximum):
		candidates = []numShrink{{value: context.context}}
	default:
		candidates = shrinkInteger(value, context.context, false)
	}
	if len(candidates) == 0 {
		return &sample{value, nil}
	}
	return &sample{value, pullEach(candidates, func(c numShrink) *sample {
		var next *numShrink
		if c.has {
			next = &c
		}
		return numberSample(c.value, minimum, maximum, integer, next)
	})}
}

// numberBounds are the bounds a constraint states, made inclusive.
func numberBounds(c *arbConstraint, integer bool) (*float64, *float64) {
	what := "number constraints"
	if integer {
		what = "integer constraints"
	}
	var minimum, maximum *float64
	if c != nil && c.order == "number" {
		if c.minimum != nil {
			v := c.minimum.(float64)
			minimum = &v
		}
		if c.maximum != nil {
			v := c.maximum.(float64)
			maximum = &v
		}
	}
	if minimum != nil && math.IsNaN(*minimum) || maximum != nil && math.IsNaN(*maximum) {
		arbFail(what)
	}
	exclusiveMin := c != nil && c.order == "number" && c.exclusiveMinimum
	exclusiveMax := c != nil && c.order == "number" && c.exclusiveMaximum
	if integer {
		if minimum != nil {
			if exclusiveMin {
				*minimum = math.Floor(*minimum) + 1
			} else {
				*minimum = math.Ceil(*minimum)
			}
		}
		if maximum != nil {
			if exclusiveMax {
				*maximum = math.Ceil(*maximum) - 1
			} else {
				*maximum = math.Floor(*maximum)
			}
		}
	} else {
		if minimum != nil && exclusiveMin && math.IsInf(*minimum, 1) || maximum != nil && exclusiveMax && math.IsInf(*maximum, -1) {
			arbFail(what)
		}
		if minimum != nil {
			if exclusiveMin {
				*minimum = nextNumber(*minimum)
			} else if *minimum == 0 {
				*minimum = math.Copysign(0, -1)
			}
		}
		if maximum != nil {
			if exclusiveMax {
				*maximum = previousNumber(*maximum)
			} else if *maximum == 0 {
				*maximum = 0
			}
		}
	}
	if integer || c != nil && c.number == "finite" {
		if minimum != nil && math.IsInf(*minimum, 1) || maximum != nil && math.IsInf(*maximum, -1) {
			arbFail(what)
		}
		if minimum != nil && math.IsInf(*minimum, -1) {
			*minimum = -math.MaxFloat64
			if integer {
				*minimum = -maxSafe
			}
		}
		if maximum != nil && math.IsInf(*maximum, 1) {
			*maximum = math.MaxFloat64
			if integer {
				*maximum = maxSafe
			}
		}
	}
	if integer {
		if minimum != nil && *minimum > maxSafe || maximum != nil && *maximum < -maxSafe {
			arbFail(what)
		}
		if minimum != nil {
			*minimum = math.Max(*minimum, -maxSafe)
		}
		if maximum != nil {
			*maximum = math.Min(*maximum, maxSafe)
		}
	}
	if minimum != nil && maximum != nil &&
		(integer && *minimum > *maximum || !integer && numberIndex(*minimum) > numberIndex(*maximum)) {
		arbFail(what)
	}
	return minimum, maximum
}

func numberNode(c *arbConstraint) *arbGen {
	number := ""
	if c != nil {
		number = c.number
	}
	integer := number == "integer"
	minimum, maximum := numberBounds(c, integer)
	lo, hi := math.Inf(-1), math.Inf(1)
	if number != "" {
		lo, hi = -math.MaxFloat64, math.MaxFloat64
	}
	if minimum != nil {
		lo = *minimum
	}
	if maximum != nil {
		hi = *maximum
	}
	var random func(*genState) float64
	if !integer {
		random = randomNumber(lo, hi, number == "" && minimum == nil)
	}
	intLo, intHi := math.NaN(), math.NaN()
	return compiledGen(nil, func() float64 { return 0 }, func(st *genState) *sample {
		if integer {
			magnitude := math.Max(1, st.size*st.size)
			center := 0.0
			if minimum != nil && *minimum > 0 {
				center = *minimum
			} else if maximum != nil && *maximum < 0 {
				center = *maximum
			}
			from, to := center-magnitude, center+magnitude
			if minimum != nil {
				from = *minimum
			}
			if maximum != nil {
				to = *maximum
			}
			if random == nil || from != intLo || to != intHi {
				intLo, intHi = from, to
				random = numericInt(from, to)
			}
		}
		v := random(st)
		if st.shrinks {
			return numberSample(v, minimum, maximum, integer, nil)
		}
		return &sample{v, nil}
	})
}

// stringEdgeCases are the edge cases of strings, in UTF-16 code units.
var stringEdgeCases = func() [][]uint16 {
	out := [][]uint16{}
	for _, s := range []string{"", " ", "\t", "\n", "\x00", "0", "-1", "4294967295", "__proto__", "constructor", "prototype", "toString"} {
		out = append(out, utf16Units(s))
	}
	return append(out, []uint16{0xD800}, []uint16{0xDC00}, []uint16{0xD83D, 0xDE00})
}()

func randomString(st *genState, minimum, maximum float64) []uint16 {
	if st.biased() {
		var eligible [][]uint16
		for _, e := range stringEdgeCases {
			if n := float64(len(e)); n >= minimum && n <= maximum {
				eligible = append(eligible, e)
			}
		}
		if len(eligible) > 0 {
			return eligible[st.index(len(eligible))]
		}
	}
	out := make([]uint16, int(st.randomLength(minimum, maximum)))
	for i := range out {
		out[i] = uint16(st.randomInt(32, 126))
	}
	return out
}

func randomCodePointString(st *genState, minimum, maximum, minCodePoints, maxCodePoints float64) []uint16 {
	count := st.randomLength(minCodePoints, maxCodePoints)
	length := st.randomLength(math.Max(minimum, count), math.Min(maximum, count*2))
	pairs := length - count
	var out []uint16
	for remaining := count; remaining > 0; remaining-- {
		if st.randomInt(1, remaining) <= pairs {
			r := rune(st.randomInt(0x10000, 0x10ffff)) - 0x10000
			out = append(out, uint16(0xD800+(r>>10)), uint16(0xDC00+r&0x3FF))
			pairs--
		} else {
			out = append(out, uint16(st.randomInt(32, 126)))
		}
	}
	return out
}

// shrinkString shortens, then lowers each code unit toward NUL.
func shrinkString(value []uint16, minimum int) [][]uint16 {
	var candidates [][]uint16
	if n := len(value); n > minimum {
		candidates = append(candidates, value[:minimum], value[:max(minimum, n/2)], value[:n-1])
	}
	for i, u := range value {
		for _, c := range shrinkInteger(float64(u), 0, true) {
			next := append([]uint16(nil), value...)
			next[i] = uint16(c.value)
			candidates = append(candidates, next)
		}
	}
	seen := map[string]bool{string(utf16Key(value)): true}
	var out [][]uint16
	for _, c := range candidates {
		if k := string(utf16Key(c)); !seen[k] {
			seen[k] = true
			out = append(out, c)
		}
	}
	return out
}

func utf16Key(units []uint16) []byte {
	out := make([]byte, 0, 2*len(units))
	for _, u := range units {
		out = append(out, byte(u>>8), byte(u))
	}
	return out
}

// shrinkSample is `sampleFromShrink`: the candidates of each value.
func shrinkSample(value []uint16, shrink func([]uint16) [][]uint16) *sample {
	s := &sample{fromUnits(value), nil}
	if candidates := shrink(value); len(candidates) > 0 {
		s.shrinks = pullEach(candidates, func(c []uint16) *sample { return shrinkSample(c, shrink) })
	}
	return s
}

func stringNode(c *arbConstraint) *arbGen {
	var patterns []*regexpGen
	var minLength, maxLength, minCP, maxCP *float64
	if c != nil {
		for _, source := range c.patterns {
			if p := compilePattern(source); p != nil {
				patterns = append(patterns, p)
			}
		}
		minLength, maxLength, minCP, maxCP = c.minLength, c.maxLength, c.minCodePoints, c.maxCodePoints
	}
	minimum, maximum := lengthBounds(minLength, maxLength, "string")
	hasCodePoints := minCP != nil || maxCP != nil
	minCodePoints, maxCodePoints := lengthBounds(minCP, maxCP, "string code point")
	if hasCodePoints {
		minimum = math.Max(minimum, minCodePoints)
		maximum = math.Min(maximum, maxCodePoints*2)
		minCodePoints = math.Max(minCodePoints, math.Ceil(minimum/2))
		maxCodePoints = math.Min(maxCodePoints, maximum)
		if minimum > maximum || minCodePoints > maxCodePoints {
			arbFail("string constraints")
		}
	}
	return compiledGen(nil, func() float64 { return 0 }, func(st *genState) *sample {
		var pattern *regexpGen
		switch {
		case len(patterns) == 1:
			pattern = patterns[0]
		case len(patterns) > 1:
			pattern = patterns[st.index(len(patterns))]
		}
		current := math.Max(minimum, st.size)
		if pattern != nil {
			current = math.Max(current, pattern.minimumLength)
			if hasCodePoints {
				current *= 2
			}
		}
		upper := math.Min(maximum, current)
		var v []uint16
		switch {
		case pattern != nil:
			var ok bool
			if v, ok = pattern.generate(st, minimum, upper); !ok {
				return nil
			}
		case hasCodePoints:
			v = randomCodePointString(st, minimum, upper, minCodePoints, math.Min(maxCodePoints, upper))
		default:
			v = randomString(st, minimum, upper)
		}
		if !st.shrinks {
			return &sample{fromUnits(v), nil}
		}
		if pattern != nil {
			return shrinkSample(v, func(s []uint16) [][]uint16 { return pattern.shrink(s, minimum) })
		}
		return shrinkSample(v, func(s []uint16) [][]uint16 { return shrinkString(s, int(minimum)) })
	})
}
