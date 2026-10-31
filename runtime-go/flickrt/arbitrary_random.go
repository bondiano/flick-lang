package flickrt

import (
	"math"
	"math/big"
)

// The randomness of Effect's Arbitrary (internal/arbitrary/runner.ts and
// model.ts), draw for draw: a seed hashed to two words, a xoshiro128**
// generator per attempt, and the uniform and edge-biased draws over it.
// Numbers are JS doubles, so the arithmetic is float64's.

// arbSeed is a hashed seed: `hashSeed`.
type arbSeed struct{ first, second uint32 }

// mix32 is MurmurHash3's fmix32.
func mix32(v uint32) uint32 {
	v ^= v >> 16
	v *= 0x85ebca6b
	v ^= v >> 13
	v *= 0xc2b2ae35
	return v ^ v>>16
}

// hashSeed hashes `typeof seed + ":" + seed`; a number seed is a float64,
// any other a string.
func hashSeed(seed Value) arbSeed {
	text := "string:" + jsString(seed)
	if _, ok := seed.(float64); ok {
		text = "number:" + jsString(seed)
	}
	units := utf16Units(text)
	first, second := uint32(0x811c9dc5), uint32(0x9e3779b9)
	for i, c := range units {
		first = (first ^ uint32(c)) * 0x01000193
		second = (second ^ uint32(c) ^ uint32(i)) * 0x85ebca6b
	}
	return arbSeed{mix32(first), mix32(second ^ uint32(len(units)))}
}

// xoshiro is xoshiro128** 1.1, the generator of an attempt.
type xoshiro struct{ s0, s1, s2, s3 uint32 }

func rotl(v uint32, k uint) uint32 { return v<<k | v>>(32-k) }

func (x *xoshiro) nextUint32() uint32 {
	result := rotl(x.s1*5, 7) * 9
	t := x.s1 << 9
	x.s2 ^= x.s0
	x.s3 ^= x.s1
	x.s1 ^= x.s2
	x.s0 ^= x.s3
	x.s2 ^= t
	x.s3 = rotl(x.s3, 11)
	return result
}

// nextDouble is a double in [0, 1) of 53 random bits.
func (x *xoshiro) nextDouble() float64 {
	high := float64(x.nextUint32() >> 5)
	low := float64(x.nextUint32() >> 6)
	return (high*0x4000000 + low) / 0x20000000000000
}

func (x *xoshiro) clone() *xoshiro { c := *x; return &c }

// attemptRandom is `makeAttemptRandom`: the state of an attempt derived
// from the seed directly, so a replay jumps to it.
func attemptRandom(seed arbSeed, attempt float64) *xoshiro {
	low := uint32(uint64(attempt))
	high := uint32(uint64(math.Floor(attempt / 0x100000000)))
	x := &xoshiro{
		mix32(seed.first ^ low ^ high*0x9e3779b9),
		mix32(seed.second ^ high ^ low*0x85ebca6b),
		mix32(seed.first ^ high ^ low*0xc2b2ae35 ^ 0x243f6a88),
		mix32(seed.second ^ low ^ high*0x27d4eb2f ^ 0xb7e15162),
	}
	if x.s0|x.s1|x.s2|x.s3 == 0 {
		x.s0 = 0x9e3779b9
	}
	return x
}

// genState is Effect's GenerationState; a copy shares the generator and
// the budget unless it replaces them.
type genState struct {
	fb         *Fiber
	size       float64
	shrinks    bool
	biasFactor float64
	random     *xoshiro
	budget     *float64
}

func (st *genState) withSize(size float64) *genState {
	c := *st
	c.size = size
	return &c
}

const (
	uint32Values = 0x100000000
	doubleValues = 0x20000000000000
)

// below is a uniform integer in [0, n), n at most 2^32.
func (st *genState) below(n float64) float64 {
	bucket := math.Floor(uint32Values / n)
	accepted := bucket * n
	v := float64(st.random.nextUint32())
	for v >= accepted {
		v = float64(st.random.nextUint32())
	}
	return math.Floor(v / bucket)
}

// index is `randomIndex`: no draw for one choice.
func (st *genState) index(n int) int {
	if n == 1 {
		return 0
	}
	return int(st.below(float64(n)))
}

// randomInt is a uniform integer in [minimum, maximum].
func (st *genState) randomInt(minimum, maximum float64) float64 {
	minimum, maximum = math.Ceil(minimum), math.Floor(maximum)
	if minimum == maximum {
		return minimum
	}
	width := maximum - minimum + 1
	if width <= uint32Values {
		return minimum + st.below(width)
	}
	if width <= doubleValues {
		bucket := math.Floor(doubleValues / width)
		accepted := bucket * width
		for {
			v := math.Floor(st.random.nextDouble() * doubleValues)
			if v < accepted {
				return minimum + math.Floor(v/bucket)
			}
		}
	}
	lo, _ := new(big.Float).SetFloat64(minimum).Int(nil)
	hi, _ := new(big.Float).SetFloat64(maximum).Int(nil)
	f, _ := new(big.Float).SetInt(st.randomBigInt(lo, hi)).Float64()
	return f
}

func (st *genState) randomBool() bool { return st.random.nextUint32()&1 == 1 }

// biased is whether this draw goes to the edges: one in biasFactor.
func (st *genState) biased() bool { return st.randomInt(1, st.biasFactor) == 1 }

// randomLength is a length in [minimum, maximum], sometimes a bound.
func (st *genState) randomLength(minimum, maximum float64) float64 {
	if minimum == maximum {
		return minimum
	}
	if st.biased() {
		if st.randomBool() {
			return minimum
		}
		return maximum
	}
	return st.randomInt(minimum, maximum)
}

// randomBigInt is a uniform integer in [minimum, maximum]: rejection over
// its bits, the leading word first.
func (st *genState) randomBigInt(minimum, maximum *big.Int) *big.Int {
	if minimum.Cmp(maximum) == 0 {
		return minimum
	}
	width := new(big.Int).Sub(maximum, minimum)
	bits := width.BitLen()
	width.Add(width, bigOne)
	leading := (bits-1)%32 + 1
	for {
		v := big.NewInt(int64(st.below(math.Exp2(float64(leading)))))
		for rest := bits - leading; rest > 0; rest -= 32 {
			v.Lsh(v, 32)
			v.Or(v, new(big.Int).SetUint64(uint64(st.random.nextUint32())))
		}
		if v.Cmp(width) < 0 {
			return v.Add(v, minimum)
		}
	}
}

// selectBiased picks the first range two times in three.
func selectBiased[T any](st *genState, values []T) T {
	if len(values) == 1 {
		return values[0]
	}
	n := float64(len(values))
	i := st.randomInt(-2*(n-1), n-2)
	if i < 0 {
		return values[0]
	}
	return values[int(i)+1]
}

// numericInt is `makeRandomNumericInt`: uniform, or near an edge.
func numericInt(minimum, maximum float64) func(*genState) float64 {
	type rng struct{ lo, hi float64 }
	var ranges []rng
	switch {
	case minimum == maximum:
		ranges = []rng{{minimum, maximum}}
	case minimum < 0 && maximum > 0:
		low, high := math.Floor(math.Log2(-minimum)), math.Floor(math.Log2(maximum))
		ranges = []rng{{-low, high}, {maximum - high, maximum}, {minimum, minimum + low}}
	default:
		gap := math.Floor(math.Log2(maximum - minimum))
		ranges = []rng{{minimum, minimum + gap}, {maximum - gap, maximum}}
		if minimum < 0 {
			ranges[0], ranges[1] = ranges[1], ranges[0]
		}
	}
	return func(st *genState) float64 {
		if st.biased() {
			r := selectBiased(st, ranges)
			return st.randomInt(r.lo, r.hi)
		}
		return st.randomInt(minimum, maximum)
	}
}

// logLike is the number of decimal digits, 0 for 0.
func logLike(v *big.Int) *big.Int {
	if v.Sign() == 0 {
		return big.NewInt(0)
	}
	return big.NewInt(int64(len(v.String())))
}

// numericBigInt is `makeRandomNumericBigInt`.
func numericBigInt(minimum, maximum *big.Int) func(*genState) *big.Int {
	type rng struct{ lo, hi *big.Int }
	add := func(a, b *big.Int) *big.Int { return new(big.Int).Add(a, b) }
	sub := func(a, b *big.Int) *big.Int { return new(big.Int).Sub(a, b) }
	var ranges []rng
	switch {
	case minimum.Cmp(maximum) == 0:
		ranges = []rng{{minimum, maximum}}
	case minimum.Sign() < 0 && maximum.Sign() > 0:
		low, high := logLike(new(big.Int).Neg(minimum)), logLike(maximum)
		ranges = []rng{{new(big.Int).Neg(low), high}, {sub(maximum, high), maximum}, {minimum, add(minimum, low)}}
	default:
		gap := logLike(sub(maximum, minimum))
		ranges = []rng{{minimum, add(minimum, gap)}, {sub(maximum, gap), maximum}}
		if minimum.Sign() < 0 {
			ranges[0], ranges[1] = ranges[1], ranges[0]
		}
	}
	return func(st *genState) *big.Int {
		if st.biased() {
			r := selectBiased(st, ranges)
			return st.randomBigInt(r.lo, r.hi)
		}
		return st.randomBigInt(minimum, maximum)
	}
}

// numberIndex is the monotone index of a double: -0 is -1, +0 is 0.
func numberIndex(v float64) int64 {
	bits := math.Float64bits(v)
	if bits>>63 == 0 {
		return int64(bits)
	}
	return int64(^bits - 1<<63)
}

func indexNumber(i int64) float64 {
	u := uint64(i) + 1<<63
	if u>>63 == 0 {
		return math.Float64frombits(^u)
	}
	return math.Float64frombits(u ^ 1<<63)
}

func nextNumber(v float64) float64 {
	switch {
	case math.IsNaN(v) || math.IsInf(v, 1):
		return v
	case v == 0:
		return math.SmallestNonzeroFloat64
	}
	return indexNumber(numberIndex(v) + 1)
}

func previousNumber(v float64) float64 {
	switch {
	case math.IsNaN(v) || math.IsInf(v, -1):
		return v
	case v == 0:
		return -math.SmallestNonzeroFloat64
	}
	return indexNumber(numberIndex(v) - 1)
}

// randomNumber is `makeRandomNumber`: an index between the bounds' with
// edge bias, and NaN one past the end when allowed.
func randomNumber(minimum, maximum float64, allowNaN bool) func(*genState) float64 {
	lo, hi := numberIndex(minimum), numberIndex(maximum)
	nanBelow := allowNaN && hi <= 0
	from, to := big.NewInt(lo), big.NewInt(hi)
	if nanBelow {
		from.Sub(from, bigOne)
	}
	if allowNaN && !nanBelow {
		to.Add(to, bigOne)
	}
	index := numericBigInt(from, to)
	return func(st *genState) float64 {
		i := index(st)
		if !i.IsInt64() || i.Int64() < lo || i.Int64() > hi {
			return math.NaN()
		}
		return indexNumber(i.Int64())
	}
}

// shuffle is Fisher–Yates over a copy.
func shuffle[T any](st *genState, xs []T) []T {
	out := append([]T(nil), xs...)
	for i := len(out) - 1; i >= 1; i-- {
		j := int(st.below(float64(i + 1)))
		out[i], out[j] = out[j], out[i]
	}
	return out
}

// floorLog10 is `Math.floor(Math.log10(n))` of a positive integer, exactly.
func floorLog10(n float64) float64 {
	d := 0.0
	for n >= 10 {
		n = math.Floor(n / 10)
		d++
	}
	return d
}
