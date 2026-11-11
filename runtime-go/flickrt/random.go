package flickrt

import (
	"math"
	"math/rand/v2"
)

// Random is the fiber's source of randomness, in its environment as the
// clock is, so that `Random.with_seed` repeats draws.
type Random interface {
	nextDouble() float64
}

const randomKey = "$random"

// liveRandom is the default: math/rand.
type liveRandom struct{}

func (liveRandom) nextDouble() float64 { return rand.Float64() }

// randomOf is the fiber's Random.
func randomOf(fb *Fiber) Random {
	if r, ok := fb.env.get(randomKey); ok {
		return r.(Random)
	}
	return liveRandom{}
}

func RandomNext(fb *Fiber) Value        { return randomOf(fb).nextDouble() }
func RandomNextBoolean(fb *Fiber) Value { return randomOf(fb).nextDouble() > 0.5 }

// RandomNextInt draws over Flick's 32-bit `Int`, as the JS adapter's.
func RandomNextInt(fb *Fiber) Value { return randomIntBetween(fb, math.MinInt32, math.MaxInt32, 1) }

// randomIntBetween is an integer from `min` to `max`; `extra` is 0 half open.
func randomIntBetween(fb *Fiber, min, max, extra float64) Value {
	lo, hi := math.Ceil(min), math.Floor(max)
	return int32(math.Floor(randomOf(fb).nextDouble()*(hi-lo+extra)) + lo)
}

func RandomNextIntBetween(fb *Fiber, options, min, max Value) Value {
	extra := 1.0
	if o := record(options).Get("halfOpen"); isSome(o) && value(o).(bool) {
		extra = 0
	}
	return randomIntBetween(fb, float64(min.(int32)), float64(max.(int32)), extra)
}

func RandomNextBetween(fb *Fiber, min, max Value) Value {
	return nextBetween(min.(float64), max.(float64), randomOf(fb).nextDouble())
}

// nextBetween is `draw` scaled between `lo` and `hi`: a draw that rounds
// up to the excluded `max` gives its predecessor.
func nextBetween(lo, hi, draw float64) float64 {
	v := draw*(hi-lo) + lo
	if v != hi || lo >= hi || math.IsInf(hi, 0) || math.IsNaN(hi) {
		return v
	}
	return math.Nextafter(hi, math.Inf(-1))
}

func RandomShuffle(fb *Fiber, xs Value) Value {
	r := randomOf(fb)
	buffer := append([]Value{}, List(xs)...)
	for i := len(buffer) - 1; i >= 1; i-- {
		index := min(i, int(math.Floor(r.nextDouble()*float64(i+1))))
		buffer[i], buffer[index] = buffer[index], buffer[i]
	}
	return buffer
}

func RandomChoice(fb *Fiber, xs Value) Value {
	buffer := List(xs)
	n := len(buffer)
	return buffer[min(n-1, int(math.Floor(randomOf(fb).nextDouble()*float64(n))))]
}

// RandomWithSeed runs `task` with the ISAAC generator seeded by `seed`.
func RandomWithSeed(fb *Fiber, seed, task Value) Value {
	return withEnv(fb, randomKey, newIsaac([]float64{seed.(float64)}), task)
}

// isaac is ISAAC_CSPRNG, bit for bit as on the JS target: its JS numbers
// that only pass through 32-bit operators are uint32s here.
type isaac struct {
	memory, result                 [256]uint32
	accumulator, lastResult, count uint32
	generation                     int
}

// newIsaac seeds the generator with a seed array: one number, or a
// string's `toIntArray`, each added to its slot as a JS number.
func newIsaac(seed []float64) *isaac {
	r := &isaac{}
	var slots [256]float64
	for i, s := range seed {
		slots[i&0xff] += s
	}
	for i, s := range slots {
		r.result[i] = jsUint32(s)
	}
	a, b, c, d, e, f, g, h := uint32(0x9e3779b9), uint32(0x9e3779b9), uint32(0x9e3779b9), uint32(0x9e3779b9),
		uint32(0x9e3779b9), uint32(0x9e3779b9), uint32(0x9e3779b9), uint32(0x9e3779b9)
	mix := func() {
		a ^= b << 11
		d += a
		b += c
		b ^= c >> 2
		e += b
		c += d
		c ^= d << 8
		f += c
		d += e
		d ^= e >> 16
		g += d
		e += f
		e ^= f << 10
		h += e
		f += g
		f ^= g >> 4
		a += f
		g += h
		g ^= h << 8
		b += g
		h += a
		h ^= a >> 9
		c += h
		a += b
	}
	for i := 0; i < 4; i++ {
		mix()
	}
	for _, from := range []*[256]uint32{&r.result, &r.memory} {
		for i := 0; i < 256; i += 8 {
			a, b, c, d = a+from[i], b+from[i+1], c+from[i+2], d+from[i+3]
			e, f, g, h = e+from[i+4], f+from[i+5], g+from[i+6], h+from[i+7]
			mix()
			r.memory[i], r.memory[i+1], r.memory[i+2], r.memory[i+3] = a, b, c, d
			r.memory[i+4], r.memory[i+5], r.memory[i+6], r.memory[i+7] = e, f, g, h
		}
	}
	r.pnrg()
	r.generation = 256
	return r
}

func (r *isaac) pnrg() {
	r.count++
	r.lastResult += r.count
	for i := 0; i < 256; i++ {
		switch i & 3 {
		case 0:
			r.accumulator ^= r.accumulator << 13
		case 1:
			r.accumulator ^= r.accumulator >> 6
		case 2:
			r.accumulator ^= r.accumulator << 2
		case 3:
			r.accumulator ^= r.accumulator >> 16
		}
		r.accumulator += r.memory[(i+128)&0xff]
		x := r.memory[i]
		r.memory[i] = r.memory[(x>>2)&0xff] + r.accumulator + r.lastResult
		y := r.memory[i]
		r.result[i] = r.memory[(y>>10)&0xff] + x
		r.lastResult = r.result[i]
	}
}

func (r *isaac) nextInt32() uint32 {
	if r.generation == 0 {
		r.pnrg()
		r.generation = 256
	}
	r.generation--
	return r.result[r.generation]
}

func (r *isaac) nextDouble() float64 {
	hi := r.nextInt32() & 0x1fffff
	lo := r.nextInt32()
	return (float64(hi)*4294967296 + float64(lo)) / (1 << 53)
}

// toIntArray is a string seed's UTF-8 bytes as little-endian 32-bit words,
// signed as JS's `|` gives them.
func toIntArray(seed string) []float64 {
	out := []float64{}
	for i := 0; i < len(seed); i += 4 {
		var w uint32
		for j := 0; j < 4 && i+j < len(seed); j++ {
			w |= uint32(seed[i+j]) << (8 * j)
		}
		out = append(out, float64(int32(w)))
	}
	return out
}

// jsUint32 is JS's ToUint32: the integer part modulo 2^32, 0 for NaN and
// infinities.
func jsUint32(x float64) uint32 {
	if math.IsNaN(x) || math.IsInf(x, 0) {
		return 0
	}
	return uint32(int64(math.Mod(math.Trunc(x), 1<<32)))
}
