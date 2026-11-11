package flickrt

import (
	"math"
	"math/big"
	"regexp"
	"strconv"
	"strings"
)

// Duration is whole milliseconds, nanoseconds, or infinite. Flick's
// durations are never negative; its checked constructors and arithmetic
// refuse what would be.
type Duration struct {
	tag    durationTag
	millis float64
	nanos  *big.Int
}

type durationTag int

const (
	durationMillis durationTag = iota
	durationNanos
	durationInfinite
	durationNegativeInfinite
)

func dur(v Value) *Duration { return v.(*Duration) }

var (
	DurationZero     Value = &Duration{tag: durationMillis}
	DurationInfinity Value = &Duration{tag: durationInfinite}
	bigMillion             = big.NewInt(1e6)
)

// roundAway is an integral float rounded half away from zero, as a BigInt.
func roundAway(x float64) *big.Int {
	n, _ := new(big.Float).SetFloat64(math.Copysign(math.Round(math.Abs(x)), x)).Int(nil)
	return n
}

// fromMillis is the duration of a number of milliseconds.
func fromMillis(ms float64) *Duration {
	switch {
	case ms != ms || ms == 0:
		return dur(DurationZero)
	case math.IsInf(ms, 1):
		return dur(DurationInfinity)
	case math.IsInf(ms, -1):
		return &Duration{tag: durationNegativeInfinite}
	case ms != math.Trunc(ms):
		return &Duration{tag: durationNanos, nanos: roundAway(ms * 1e6)}
	}
	return &Duration{tag: durationMillis, millis: ms}
}

// zeroMillis is `Duration.isZero(Duration.millis(ms))`: under half a
// nanosecond rounds to zero.
func zeroMillis(ms float64) bool {
	d := fromMillis(ms)
	return d == dur(DurationZero) || d.tag == durationNanos && d.nanos.Sign() == 0
}

// fromNanos is the duration of a BigInt of nanoseconds.
func fromNanos(n *big.Int) *Duration {
	if n.Sign() == 0 {
		return dur(DurationZero)
	}
	return &Duration{tag: durationNanos, nanos: n}
}

func (d *Duration) infinite() bool {
	return d.tag == durationInfinite || d.tag == durationNegativeInfinite
}

// toNanos is the nanoseconds of a finite duration.
func (d *Duration) toNanos() *big.Int {
	if d.tag == durationNanos {
		return d.nanos
	}
	if d.millis == math.Trunc(d.millis) {
		return new(big.Int).Mul(exactInt(d.millis), bigMillion)
	}
	return roundAway(d.millis * 1e6)
}

func (d *Duration) negative() bool {
	switch d.tag {
	case durationMillis:
		return d.millis < 0
	case durationNanos:
		return d.nanos.Sign() < 0
	}
	return d.tag == durationNegativeInfinite
}

var durationTagged = Tag("Duration.DurationError.DurationError")

func durationError(message string) Value { return Err(NewData(durationTagged, message)) }

func nonNegative(what string, d *Duration) Value {
	if d.negative() {
		return durationError(what + ": the result is negative")
	}
	return Ok(d)
}

func checkedDuration(unit string, n, millisPer float64) Value {
	switch {
	case math.IsNaN(n):
		return durationError(unit + ": NaN is not a duration")
	case n < 0:
		return durationError(unit + ": " + ShowNumber(n) + " is negative")
	}
	ms := n * millisPer
	if math.IsInf(ms, 0) {
		return durationError(unit + ": " + ShowNumber(n) + " overflows; use Duration.infinity for an unbounded duration")
	}
	return Ok(fromMillis(ms))
}

func DurationMillis(n Value) Value  { return checkedDuration("millis", n.(float64), 1) }
func DurationSeconds(n Value) Value { return checkedDuration("seconds", n.(float64), 1000) }
func DurationMinutes(n Value) Value { return checkedDuration("minutes", n.(float64), 60000) }
func DurationHours(n Value) Value   { return checkedDuration("hours", n.(float64), 3600000) }
func DurationDays(n Value) Value    { return checkedDuration("days", n.(float64), 86400000) }
func DurationWeeks(n Value) Value   { return checkedDuration("weeks", n.(float64), 604800000) }

func DurationNanos(n Value) Value {
	if x := n.(*big.Int); x.Sign() < 0 {
		return durationError("nanos: " + x.String() + " is negative")
	}
	return Ok(fromNanos(n.(*big.Int)))
}

func DurationMicros(n Value) Value {
	x := n.(*big.Int)
	if x.Sign() < 0 {
		return durationError("micros: " + x.String() + " is negative")
	}
	return Ok(fromNanos(new(big.Int).Mul(x, big.NewInt(1000))))
}

var durationInput = regexp.MustCompile(`^(-?\d+(?:\.\d+)?)\s+(nanos?|micros?|millis?|seconds?|minutes?|hours?|days?|weeks?)$`)

// parseNanos is a decimal number times `scale`, rounded half away from zero.
func parseNanos(text string, scale int64) *big.Int {
	whole, frac, found := strings.Cut(text, ".")
	if !found {
		n, _ := new(big.Int).SetString(text, 10)
		return n.Mul(n, big.NewInt(scale))
	}
	negative := strings.HasPrefix(whole, "-")
	w, _ := new(big.Int).SetString(strings.TrimPrefix(whole, "-"), 10)
	f, _ := new(big.Int).SetString(frac, 10)
	den := pow10(int64(len(frac)))
	num := new(big.Int).Mul(new(big.Int).Add(new(big.Int).Mul(w, den), f), big.NewInt(scale))
	q, r := new(big.Int).QuoRem(num, den, new(big.Int))
	if r.Mul(r, big.NewInt(2)).Cmp(den) >= 0 {
		q.Add(q, bigOne)
	}
	if negative {
		q.Neg(q)
	}
	return q
}

var unitMillis = map[string]float64{"milli": 1, "second": 1000, "minute": 60000, "hour": 3600000, "day": 86400000, "week": 604800000}

// DurationFromInput reads `"Infinity"` or `"<number> <unit>"`.
func DurationFromInput(s Value) Value {
	quoted := QuoteJSON(s.(string))
	d, ok := parseDuration(s.(string))
	if !ok {
		return durationError("not a duration: " + quoted)
	}
	return nonNegative("from_input "+quoted, d)
}

// parseDuration is the duration of a string, negative ones too.
func parseDuration(text string) (*Duration, bool) {
	if text == "Infinity" {
		return dur(DurationInfinity), true
	}
	m := durationInput.FindStringSubmatch(text)
	if !strings.Contains(text, " ") || m == nil {
		return nil, false
	}
	switch unit := strings.TrimSuffix(m[2], "s"); unit {
	case "nano":
		return fromNanos(parseNanos(m[1], 1)), true
	case "micro":
		return fromNanos(parseNanos(m[1], 1000)), true
	default:
		n, _ := strconv.ParseFloat(m[1], 64)
		return fromMillis(n * unitMillis[unit]), true
	}
}

func DurationIsFinite(d Value) Value { return !dur(d).infinite() }

func DurationIsZero(d Value) Value {
	x := dur(d)
	return x.tag == durationMillis && x.millis == 0 || x.tag == durationNanos && x.nanos.Sign() == 0
}

func DurationIsPositive(d Value) Value {
	x := dur(d)
	return x.tag == durationInfinite || !x.infinite() && !x.negative() && !DurationIsZero(d).(bool)
}

func bigToFloat(n *big.Int) float64 {
	f, _ := new(big.Float).SetInt(n).Float64()
	return f
}

// durationIn is the duration in a unit of `per` milliseconds, if finite.
func durationIn(per float64) func(Value) Value {
	return func(d Value) Value {
		x := dur(d)
		switch x.tag {
		case durationMillis:
			return Some(x.millis / per)
		case durationNanos:
			return Some(bigToFloat(x.nanos) / (per * 1e6))
		}
		return None
	}
}

var (
	DurationToMillis  = durationIn(1)
	DurationToSeconds = durationIn(1000)
	DurationToMinutes = durationIn(60000)
	DurationToHours   = durationIn(3600000)
	DurationToDays    = durationIn(86400000)
	DurationToWeeks   = durationIn(604800000)
)

func DurationToNanos(d Value) Value {
	if dur(d).infinite() {
		return None
	}
	return Some(dur(d).toNanos())
}

func DurationToHrTime(d Value) Value {
	if dur(d).infinite() {
		return None
	}
	n := new(big.Int).Abs(dur(d).toNanos())
	s, ns := new(big.Int).QuoRem(n, big.NewInt(1e9), new(big.Int))
	sign := 1.0
	if dur(d).negative() {
		sign = -1
	}
	return Some([]Value{sign * bigToFloat(s), sign * bigToFloat(ns)})
}

func saturate(n *big.Int) int32 {
	if !n.IsInt64() || n.Int64() > math.MaxInt32 {
		return math.MaxInt32
	}
	return int32(n.Int64())
}

// DurationParts is the days, hours, minutes, seconds, milliseconds and
// nanoseconds of a duration, each saturated into an Int.
func DurationParts(d Value) Value {
	if dur(d).infinite() {
		m := int32(math.MaxInt32)
		return Rec("days", m, "hours", m, "minutes", m, "seconds", m, "millis", m, "nanos", m)
	}
	n := new(big.Int).Abs(dur(d).toNanos())
	part := func(per, mod int64) int32 {
		q := new(big.Int).Quo(n, big.NewInt(per))
		if mod > 0 {
			q.Rem(q, big.NewInt(mod))
		}
		return saturate(q)
	}
	return Rec("days", part(86400e9, 0), "hours", part(3600e9, 24), "minutes", part(60e9, 60),
		"seconds", part(1e9, 60), "millis", part(1e6, 1000), "nanos", part(1, 1e6))
}

// DurationFormat is `Duration.format`: the non-zero parts of days down to
// nanoseconds, the days as JS writes their Number.
func DurationFormat(d Value) Value {
	x := dur(d)
	switch {
	case x.tag == durationNegativeInfinite:
		return "-Infinity"
	case x.infinite():
		return "Infinity"
	case DurationIsZero(d).(bool):
		return "0"
	}
	p := record(DurationParts(d))
	var pieces []string
	days, _ := new(big.Float).SetInt(new(big.Int).Quo(new(big.Int).Abs(x.toNanos()), big.NewInt(86400e9))).Float64()
	if days != 0 {
		pieces = append(pieces, formatNumber(days)+"d")
	}
	for i, suffix := range []string{"h", "m", "s", "ms", "ns"} {
		if v := p.vals[i+1].(int32); v != 0 {
			pieces = append(pieces, strconv.Itoa(int(v))+suffix)
		}
	}
	out := strings.Join(pieces, " ")
	if x.negative() {
		return "-" + out
	}
	return out
}

func DurationMatch(fb *Fiber, options, d Value) Value {
	r, x := record(options), dur(d)
	switch x.tag {
	case durationMillis:
		return Apply(fb, r.Get("onMillis"), x.millis)
	case durationNanos:
		return Apply(fb, r.Get("onNanos"), x.nanos)
	}
	return Apply(fb, r.Get("onInfinity"), Unit)
}

// matchPair runs the case both durations fit: milliseconds when both are,
// nanoseconds otherwise, or infinity when one is.
func matchPair(self, that *Duration, millis func(a, b float64) Value, nanos func(a, b *big.Int) Value, inf func() Value) Value {
	switch {
	case self.infinite() || that.infinite():
		return inf()
	case self.tag == durationMillis && that.tag == durationMillis:
		return millis(self.millis, that.millis)
	}
	return nanos(self.toNanos(), that.toNanos())
}

func DurationMatchPair(fb *Fiber, that, options, self Value) Value {
	r := record(options)
	return matchPair(dur(self), dur(that),
		func(a, b float64) Value { return Apply2(fb, r.Get("onMillis"), a, b) },
		func(a, b *big.Int) Value { return Apply2(fb, r.Get("onNanos"), a, b) },
		func() Value { return Apply2(fb, r.Get("onInfinity"), self, that) })
}

func DurationAdd(a, b Value) Value {
	return matchPair(dur(a), dur(b),
		func(x, y float64) Value { return fromMillis(x + y) },
		func(x, y *big.Int) Value { return fromNanos(new(big.Int).Add(x, y)) },
		func() Value { return DurationInfinity })
}

func DurationSubtract(that, self Value) Value {
	d := matchPair(dur(self), dur(that),
		func(x, y float64) Value { return fromMillis(x - y) },
		func(x, y *big.Int) Value { return fromNanos(new(big.Int).Sub(x, y)) },
		func() Value {
			if dur(self).tag == durationInfinite {
				if dur(that).tag == durationInfinite {
					return DurationZero
				}
				return DurationInfinity
			}
			return &Duration{tag: durationNegativeInfinite}
		})
	return nonNegative("subtract", dur(d))
}

func DurationTimes(n, self Value) Value {
	k, x := n.(float64), dur(self)
	if k != k || k < 0 {
		return durationError("times: " + ShowNumber(k) + " is negative or NaN")
	}
	var d *Duration
	switch {
	case x.infinite():
		d = x
		if k == 0 {
			d = dur(DurationZero)
		}
	case x.tag == durationMillis:
		d = fromMillis(x.millis * k)
	case k == math.Trunc(k) && !math.IsInf(k, 0):
		d = fromNanos(new(big.Int).Mul(x.nanos, exactInt(k)))
	default:
		scaled := bigToFloat(x.nanos) * k
		if math.IsInf(k, 0) || math.Abs(bigToFloat(x.nanos)) <= maxSafe && math.Abs(scaled) <= maxSafe {
			if math.IsInf(scaled, 0) || scaled != scaled {
				d = fromMillis(scaled)
			} else {
				d = fromNanos(roundAway(scaled))
			}
		} else {
			num, den := k, big.NewInt(1)
			for num != math.Trunc(num) {
				num *= 2
				den.Lsh(den, 1)
			}
			p := new(big.Int).Mul(x.nanos, exactInt(num))
			q, r := new(big.Int).QuoRem(new(big.Int).Abs(p), den, new(big.Int))
			if r.Lsh(r, 1).Cmp(den) >= 0 {
				q.Add(q, bigOne)
			}
			if p.Sign() < 0 {
				q.Neg(q)
			}
			d = fromNanos(q)
		}
	}
	return nonNegative("times", d)
}

func DurationDivide(n, self Value) Value {
	by, x := n.(float64), dur(self)
	fail := durationError("divide: cannot divide by " + ShowNumber(by))
	if by < 0 || by == 0 || math.IsInf(by, 0) || by != by {
		return fail
	}
	var d *Duration
	switch {
	case x.infinite():
		d = x
	case x.tag == durationMillis:
		d = fromMillis(x.millis / by)
	case by != math.Trunc(by):
		return fail
	default:
		d = fromNanos(new(big.Int).Quo(x.nanos, exactInt(by)))
	}
	return nonNegative("divide", d)
}

func durationOrder(a, b Value) int {
	x, y := dur(a), dur(b)
	o := matchPair(x, y,
		func(p, q float64) Value { return numberOrder(p, q) },
		func(p, q *big.Int) Value { return p.Cmp(q) },
		func() Value {
			switch {
			case x.tag == y.tag:
				return 0
			case x.tag == durationInfinite || y.tag == durationNegativeInfinite:
				return 1
			}
			return -1
		})
	return o.(int)
}

func DurationCompare(a, b Value) Value       { return ordering(durationOrder(a, b)) }
func DurationMin(a, self Value) Value        { return orderMin(durationOrder, self, a) }
func DurationMax(a, self Value) Value        { return orderMax(durationOrder, self, a) }
func DurationLess(a, self Value) Value       { return less(durationOrder, a, self) }
func DurationLessEq(a, self Value) Value     { return lessEq(durationOrder, a, self) }
func DurationGreater(a, self Value) Value    { return greater(durationOrder, a, self) }
func DurationGreaterEq(a, self Value) Value  { return greaterEq(durationOrder, a, self) }
func DurationBetween(options, d Value) Value { return between(durationOrder, options, d) }
func DurationClamp(options, d Value) Value   { return clamp(durationOrder, options, d) }
