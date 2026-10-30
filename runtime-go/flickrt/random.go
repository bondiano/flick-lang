package flickrt

import (
	"math"
	"math/rand/v2"
	"sync"
)

// Random is Effect's: the generator a fiber draws from, a service of its
// environment. The system's is the default; `Random.with_seed` runs a
// task with Effect's ISAAC generator seeded, so a seed draws the same
// numbers on both targets.
type Random interface {
	next() float64 // in [0, 1)
}

const randomKey = "$random"

func randomOf(fb *Fiber) Random {
	if r, ok := fb.env.get(randomKey); ok {
		return r.(Random)
	}
	return systemRandom{}
}

type systemRandom struct{}

func (systemRandom) next() float64 { return rand.Float64() }

// isaac is Effect's port of ISAAC (William P. McMeans' isaacCSPRNG, from
// Yves-Marie K. Rinquin's isaac.js). Its JS arithmetic is modulo 2^32, so
// it is exact in uint32.
type isaac struct {
	mu               sync.Mutex // fibers may draw at once
	memory, result   [256]uint32
	acc, last, count uint32
	generation       int
}

func newIsaac(seed float64) *isaac {
	g := &isaac{}
	// the seed is added to the zeroed results as a JS number, which add32 wraps
	g.result[0] = uint32(toInt32(seed))
	var s [8]uint32
	for i := range s {
		s[i] = 0x9e3779b9
	}
	mix := func() {
		a, b, c, d, e, f, gg, h := s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]
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
		gg += d
		e += f
		e ^= f << 10
		h += e
		f += gg
		f ^= gg >> 4
		a += f
		gg += h
		gg ^= h << 8
		b += gg
		h += a
		h ^= a >> 9
		c += h
		a += b
		s = [8]uint32{a, b, c, d, e, f, gg, h}
	}
	for range 4 {
		mix()
	}
	for _, from := range [2]*[256]uint32{&g.result, &g.memory} {
		for i := 0; i < 256; i += 8 {
			for j := range s {
				s[j] += from[i+j]
			}
			mix()
			copy(g.memory[i:i+8], s[:])
		}
	}
	g.round()
	g.generation = 256
	return g
}

// round makes the next 256 results.
func (g *isaac) round() {
	g.count++
	g.last += g.count
	for i := range 256 {
		switch i & 3 {
		case 0:
			g.acc ^= g.acc << 13
		case 1:
			g.acc ^= g.acc >> 6
		case 2:
			g.acc ^= g.acc << 2
		case 3:
			g.acc ^= g.acc >> 16
		}
		g.acc += g.memory[(i+128)&0xff]
		x := g.memory[i]
		g.memory[i] = g.memory[(x>>2)&0xff] + g.acc + g.last
		y := g.memory[i]
		g.result[i] = g.memory[(y>>10)&0xff] + x
		g.last = g.result[i]
	}
}

func (g *isaac) int32() uint32 {
	if g.generation == 0 {
		g.round()
		g.generation = 256
	}
	g.generation--
	return g.result[g.generation]
}

// next is a 53-bit fraction: 21 low bits of one draw over 32 of the next.
func (g *isaac) next() float64 {
	g.mu.Lock()
	defer g.mu.Unlock()
	hi := g.int32() & 0x1fffff
	lo := g.int32()
	return (float64(hi)*4294967296 + float64(lo)) / 0x20000000000000
}

// The `Random` module.

func RandomNext(fb *Fiber) Value        { return randomOf(fb).next() }
func RandomNextBoolean(fb *Fiber) Value { return randomOf(fb).next() > 0.5 }

// intBetween is Effect's nextIntBetween on Ints: [min, max], or [min, max)
// when half open.
func intBetween(fb *Fiber, min, max int32, halfOpen bool) int32 {
	extra := 1.0
	if halfOpen {
		extra = 0
	}
	return int32(math.Floor(randomOf(fb).next()*(float64(max)-float64(min)+extra)) + float64(min))
}

func RandomNextInt(fb *Fiber) Value { return intBetween(fb, math.MinInt32, math.MaxInt32, false) }

func RandomNextIntBetween(fb *Fiber, options, min, max Value) Value {
	half := record(options).Get("halfOpen").(*Data)
	return intBetween(fb, min.(int32), max.(int32), half.Tag == "Some" && half.Fields[0].(bool))
}

// RandomNextBetween is a Float in [min, max): a draw that rounds to `max`
// gives the Float just below it, as Effect does.
func RandomNextBetween(fb *Fiber, min, max Value) Value {
	lo, hi := min.(float64), max.(float64)
	v := randomOf(fb).next()*(hi-lo) + lo
	if v != hi || lo >= hi || math.IsInf(hi, 0) || hi != hi {
		return v
	}
	if hi == 0 {
		return -math.SmallestNonzeroFloat64
	}
	bits := math.Float64bits(hi)
	if hi > 0 {
		return math.Float64frombits(bits - 1)
	}
	return math.Float64frombits(bits + 1)
}

// RandomShuffle is Fisher-Yates from the end, as Effect's.
func RandomShuffle(fb *Fiber, xs Value) Value {
	out := append([]Value(nil), List(xs)...)
	r := randomOf(fb)
	for i := len(out) - 1; i >= 1; i-- {
		j := min(i, int(math.Floor(r.next()*float64(i+1))))
		out[i], out[j] = out[j], out[i]
	}
	return out
}

func RandomChoice(fb *Fiber, xs Value) Value {
	in := List(xs)
	return in[min(len(in)-1, int(math.Floor(randomOf(fb).next()*float64(len(in)))))]
}

func RandomWithSeed(fb *Fiber, seed, task Value) Value {
	return withEnv(fb, randomKey, newIsaac(seed.(float64)), task)
}
