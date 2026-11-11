package flickrt

import (
	"math"
	"math/big"
	"sort"
)

// BigInts and BigDecimals of Arbitrary: magnitudes by families of bit
// lengths, shrinking by bisection toward the closest passing value.

var semanticMagnitudes = func() []*big.Int {
	maxValue, _ := new(big.Float).SetFloat64(math.MaxFloat64).Int(nil)
	return []*big.Int{
		big.NewInt(1), big.NewInt(2), big.NewInt(0x7fffffff), big.NewInt(0x80000000),
		big.NewInt(maxSafe), big.NewInt(maxSafe + 1), maxValue, new(big.Int).Add(maxValue, bigOne),
		new(big.Int).Lsh(bigOne, 1024),
	}
}()

func bigAdd(a, b *big.Int) *big.Int { return new(big.Int).Add(a, b) }
func bigSub(a, b *big.Int) *big.Int { return new(big.Int).Sub(a, b) }
func bigMul(a, b *big.Int) *big.Int { return new(big.Int).Mul(a, b) }

func within(v, minimum, maximum *big.Int) bool {
	return (minimum == nil || v.Cmp(minimum) >= 0) && (maximum == nil || v.Cmp(maximum) <= 0)
}

func randomMagnitude(st *genState, maximum *big.Int) *big.Int {
	if maximum != nil && maximum.Sign() == 0 {
		return new(big.Int)
	}
	family := st.randomInt(0, 7)
	if family == 0 {
		return new(big.Int)
	}
	maximumBits := 2048.0
	if maximum != nil {
		maximumBits = float64(maximum.BitLen())
	}
	upperBits := maximumBits
	if family <= 3 {
		upperBits = math.Min(20, maximumBits)
	}
	bits := uint(st.randomInt(1, upperBits))
	lowest := new(big.Int).Lsh(bigOne, bits-1)
	upper := bigSub(new(big.Int).Lsh(lowest, 1), bigOne)
	if maximum != nil && maximum.Cmp(upper) < 0 {
		upper = maximum
	}
	return st.randomBigInt(lowest, upper)
}

func bigintGenerator(minimum, maximum *big.Int) func(*genState) *big.Int {
	if minimum != nil && maximum != nil {
		return numericBigInt(minimum, maximum)
	}
	special := []*big.Int{new(big.Int)}
	if minimum != nil {
		special = append(special, minimum, bigAdd(minimum, bigOne))
	}
	if maximum != nil {
		special = append(special, maximum, bigSub(maximum, bigOne))
	}
	for _, m := range semanticMagnitudes {
		special = append(special, m, new(big.Int).Neg(m))
	}
	kept := special[:0]
	for _, v := range special {
		if within(v, minimum, maximum) {
			kept = append(kept, v)
		}
	}
	special = kept
	boundary, direction := minimum, big.NewInt(1)
	if minimum == nil {
		boundary, direction = maximum, big.NewInt(-1)
	}
	var finiteMagnitude *big.Int
	if boundary != nil && bigMul(boundary, direction).Sign() <= 0 {
		finiteMagnitude = new(big.Int).Neg(bigMul(boundary, direction))
	}
	return func(st *genState) *big.Int {
		if st.randomInt(0, 7) == 0 {
			return special[st.index(len(special))]
		}
		if boundary != nil && (bigMul(boundary, direction).Sign() > 0 || st.randomBool()) {
			return bigAdd(boundary, bigMul(direction, randomMagnitude(st, nil)))
		}
		sign := big.NewInt(-1)
		if st.randomBool() {
			sign = big.NewInt(1)
		}
		var limit *big.Int
		if sign.Cmp(direction) != 0 {
			limit = finiteMagnitude
		}
		return bigMul(sign, randomMagnitude(st, limit))
	}
}

func bigintSample(value, target, context *big.Int) *sample {
	return lazyShrinks(value, func() shrinkPull {
		from := target
		if context != nil {
			from = context
		}
		gap := bigSub(value, from)
		if gap.Sign() == 0 {
			return nil
		}
		if context != nil && gap.CmpAbs(bigOne) == 0 {
			return pullEach([]*big.Int{context}, func(c *big.Int) *sample { return bigintSample(c, target, nil) })
		}
		direction := big.NewInt(int64(gap.Sign()))
		seen := map[string]bool{}
		var candidates []*big.Int
		put := func(c *big.Int) {
			if k := c.String(); !seen[k] {
				seen[k] = true
				candidates = append(candidates, c)
			}
		}
		add := func(c *big.Int) {
			if bigMul(bigSub(c, from), direction).Sign() > 0 && bigMul(bigSub(value, c), direction).Sign() > 0 {
				put(c)
			}
		}
		ones := func(bits int) *big.Int { return bigSub(new(big.Int).Lsh(bigOne, uint(bits)), bigOne) }
		if context == nil {
			put(target)
			for _, m := range semanticMagnitudes {
				add(bigAdd(target, bigMul(direction, m)))
			}
			for bits := 1; bits < gap.BitLen(); bits *= 2 {
				add(bigAdd(target, bigMul(direction, ones(bits))))
			}
		} else {
			passing, failing := context.BitLen(), value.BitLen()
			if (failing-passing > 1 || passing-failing > 1) && (context.Sign() == 0 || (context.Sign() > 0) == (value.Sign() > 0)) {
				add(bigMul(direction, ones((passing+failing)/2)))
			}
		}
		toRemove := new(big.Int).Quo(gap, big.NewInt(2))
		for step := 0; toRemove.Sign() != 0 && step < 256; step++ {
			add(bigSub(value, toRemove))
			toRemove = new(big.Int).Quo(toRemove, big.NewInt(2))
		}
		put(bigSub(value, direction))
		sort.SliceStable(candidates, func(i, j int) bool {
			return candidates[i].Cmp(candidates[j]) == direction.Sign()*-1
		})
		previous := context
		return pullEach(candidates, func(c *big.Int) *sample {
			child := bigintSample(c, target, previous)
			previous = c
			return child
		})
	})
}

func bigintNode(c *arbConstraint) *arbGen {
	var minimum, maximum *big.Int
	if c != nil && c.order == "bigint" {
		if c.minimum != nil {
			minimum = new(big.Int).Set(c.minimum.(*big.Int))
			if c.exclusiveMinimum {
				minimum.Add(minimum, bigOne)
			}
		}
		if c.maximum != nil {
			maximum = new(big.Int).Set(c.maximum.(*big.Int))
			if c.exclusiveMaximum {
				maximum.Sub(maximum, bigOne)
			}
		}
		if minimum != nil && maximum != nil && minimum.Cmp(maximum) > 0 {
			arbFail("bigint constraints")
		}
	}
	generate := bigintGenerator(minimum, maximum)
	target := new(big.Int)
	if minimum != nil && minimum.Sign() > 0 {
		target = minimum
	} else if maximum != nil && maximum.Sign() < 0 {
		target = maximum
	}
	return compiledGen(nil, func() float64 { return 0 }, func(st *genState) *sample {
		v := generate(st)
		if st.shrinks {
			return bigintSample(v, target, nil)
		}
		return &sample{v, nil}
	})
}

var shrinkExponents = []int64{-512, -256, -128, -64, -32, -16, -8, -4, -2, -1, 0, 1, 2, 4, 8, 16, 32, 64, 128, 256, 512}

func addScale(scale, offset, maximum int64) int64 { return min(maximum, scale+offset) }

// projectExact is the value at a finer scale, nil past 4096 digits.
func projectExact(d *Decimal, scale int64) *big.Int {
	diff := scale - d.scale
	if diff < 0 || diff > 4096 {
		return nil
	}
	return bigMul(d.value, pow10(diff))
}

func midpoint(passing, failing *Decimal, maximumScale int64) *Decimal {
	scale := addScale(max(passing.scale, failing.scale), 1, maximumScale)
	left, right := projectExact(passing, scale), projectExact(failing, scale)
	if left == nil || right == nil {
		return nil
	}
	v := bigAdd(left, new(big.Int).Quo(bigSub(right, left), big.NewInt(2)))
	if v.Cmp(left) == 0 || v.Cmp(right) == 0 {
		return nil
	}
	return &Decimal{v, scale}
}

// decimalContext is the closest passing value and how often it was refined.
type decimalContext struct {
	passing    *Decimal
	refinement int64
}

func decimalSample(value, target *Decimal, context *decimalContext, maximumScale int64) *sample {
	return lazyShrinks(value, func() shrinkPull {
		from := target
		if context != nil {
			from = context.passing
		}
		direction := sign(decCompare(value, from))
		if direction == 0 {
			return nil
		}
		var raw []*Decimal
		if context == nil {
			for _, e := range shrinkExponents {
				raw = append(raw, &Decimal{big.NewInt(int64(direction)), -e})
			}
		} else {
			normalized := context.passing.normalize()
			scale := addScale(normalized.scale, min(context.refinement+1, 128), maximumScale)
			if p := projectExact(normalized, scale); p != nil {
				raw = append(raw, &Decimal{bigAdd(p, big.NewInt(int64(direction))), scale})
			}
		}
		var candidates []*Decimal
		if context == nil {
			candidates = append(candidates, target)
		}
		for _, c := range raw {
			if c.scale <= maximumScale && sign(decCompare(c, from)) == direction && sign(decCompare(c, value)) == -direction {
				candidates = append(candidates, c)
			}
		}
		previous, index := context, 0
		return func() (*sample, bool) {
			var c *Decimal
			if index < len(candidates) {
				c = candidates[index]
				index++
			} else if previous != nil {
				c = midpoint(previous.passing, value, maximumScale)
			}
			if c == nil {
				return nil, false
			}
			child := decimalSample(c, target, previous, maximumScale)
			refinement := int64(0)
			if previous != nil {
				refinement = previous.refinement
			}
			previous = &decimalContext{c, refinement + 1}
			return child, true
		}
	})
}

func sign(n int) int {
	switch {
	case n < 0:
		return -1
	case n > 0:
		return 1
	}
	return 0
}

// decimalNode is a BigDecimal without bounds, the only one a schema of
// the program states.
func decimalNode() *arbGen {
	zero := &Decimal{new(big.Int), 0}
	special := []*Decimal{
		zero, {big.NewInt(1), 1}, {big.NewInt(1), 0}, {big.NewInt(10), 0},
		{big.NewInt(-1), 1}, {big.NewInt(-1), 0}, {big.NewInt(-10), 0},
	}
	return compiledGen(nil, func() float64 { return 0 }, func(st *genState) *sample {
		var v *Decimal
		if st.randomInt(0, 7) == 0 {
			v = special[st.index(len(special))]
		} else {
			mode := st.randomInt(0, 7)
			var scale int64
			if mode == 6 {
				scale = int64(st.randomInt(-6, 20))
			} else {
				precision := st.randomInt(1, 128)
				scale = int64(precision - 1 - st.randomInt(-512, 512))
			}
			bits := 0.0
			if st.randomInt(0, 3) != 0 {
				bits = st.randomInt(0, 20)
			} else {
				bits = st.randomInt(1, 425)
			}
			distance := new(big.Int)
			if bits > 0 {
				lowest := new(big.Int).Lsh(bigOne, uint(bits-1))
				distance = st.randomBigInt(lowest, bigSub(new(big.Int).Lsh(lowest, 1), bigOne))
			}
			if !st.randomBool() {
				distance = new(big.Int).Neg(distance)
			}
			v = &Decimal{distance, scale}
			if st.randomInt(0, 7) == 0 && scale < maxSafe {
				trailing := int64(st.randomInt(1, math.Min(8, float64(maxSafe-scale))))
				v = &Decimal{bigMul(distance, pow10(trailing)), scale + trailing}
			}
		}
		if st.shrinks {
			return decimalSample(v, zero, nil, addScale(max(v.scale, 0), 128, maxSafe))
		}
		return &sample{v, nil}
	})
}
