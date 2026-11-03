package flickrt

import (
	"math"
	"math/big"
	"regexp"
	"strconv"
	"strings"
)

// Decimal is a BigDecimal as Effect's: value × 10^-scale. It is never
// mutated, and its scale is a safe integer.
type Decimal struct {
	value *big.Int
	scale int64
}

var (
	bigZero = big.NewInt(0)
	bigOne  = big.NewInt(1)
	bigTen  = big.NewInt(10)
)

func dec(v Value) *Decimal { return v.(*Decimal) }

func pow10(n int64) *big.Int { return new(big.Int).Exp(bigTen, big.NewInt(n), nil) }

// Effect 4.1 bounds a parsed scale and the alignment of two scales.
const (
	maxDecimalScale     = 10_000
	maxDecimalAlignment = 2 * maxDecimalScale
)

// powerOfTen is Effect's: 10^n for aligning scales, a RangeError past the bound.
func powerOfTen(n int64) *big.Int {
	if n > maxDecimalAlignment {
		panic(rangeError("Scale alignment must not exceed " + strconv.Itoa(maxDecimalAlignment) + " digits, got " + strconv.FormatInt(n, 10)))
	}
	return pow10(n)
}

var decimalTag = Tag("BigDecimal.DecimalError.DecimalError")

func decimalError(message string) Value { return Err(NewData(decimalTag, message)) }

const maxSafe = 1<<53 - 1

func safeScale(f float64) bool { return f == math.Trunc(f) && math.Abs(f) <= maxSafe }

// normalize drops the trailing zeros of the value.
func (d *Decimal) normalize() *Decimal {
	if d.value.Sign() == 0 {
		return &Decimal{new(big.Int), 0}
	}
	digits := d.value.String()
	end := len(digits)
	for digits[end-1] == '0' {
		end--
	}
	if end == len(digits) {
		return d
	}
	v, _ := new(big.Int).SetString(digits[:end], 10)
	return &Decimal{v, d.scale - int64(len(digits)-end)}
}

// rescale is `d` at `scale`, truncating digits it drops.
func (d *Decimal) rescale(scale int64) *Decimal {
	switch {
	case scale > d.scale:
		return &Decimal{new(big.Int).Mul(d.value, powerOfTen(scale-d.scale)), scale}
	case scale < d.scale:
		return &Decimal{new(big.Int).Quo(d.value, powerOfTen(d.scale-scale)), scale}
	}
	return d
}

func (d *Decimal) sign() int { return d.value.Sign() }

func decSum(a, b *Decimal) *Decimal {
	switch {
	case b.sign() == 0:
		return a
	case a.sign() == 0:
		return b
	}
	s := max(a.scale, b.scale)
	return &Decimal{new(big.Int).Add(a.rescale(s).value, b.rescale(s).value), s}
}

func decCompare(a, b *Decimal) int {
	if a.scale == b.scale {
		return a.value.Cmp(b.value)
	}
	if sa, sb := a.sign(), b.sign(); sa != sb {
		return sa - sb
	} else if sa == 0 {
		return 0
	}
	s := max(a.scale, b.scale)
	return a.rescale(s).value.Cmp(b.rescale(s).value)
}

var finiteInt = regexp.MustCompile(`^[+-]?\d+$`)

// parseDecimal is Effect's `BigDecimal.fromString`: an empty string is 0.
func parseDecimal(s string) (*Decimal, bool) {
	if s == "" {
		return &Decimal{new(big.Int), 0}, true
	}
	base, exp := s, int64(0)
	if i := strings.IndexAny(s, "eE"); i >= 0 {
		trail := s[i+1:]
		base = s[:i]
		e, err := strconv.ParseInt(trail, 10, 64)
		if base == "" || err != nil || !finiteInt.MatchString(trail) || math.Abs(float64(e)) > maxSafe {
			return nil, false
		}
		exp = e
	}
	digits, offset := base, int64(0)
	if dot := strings.IndexByte(base, '.'); dot >= 0 {
		trail := base[dot+1:]
		if strings.TrimLeft(trail, "0123456789") != "" {
			return nil, false
		}
		digits = base[:dot] + trail
		offset = int64(len(trail))
	}
	if !finiteInt.MatchString(digits) {
		return nil, false
	}
	scale := offset - exp
	v, _ := new(big.Int).SetString(strings.TrimPrefix(digits, "+"), 10)
	// as Effect 4.1: a scale `format` can print back
	normalized := scale
	if v.Sign() != 0 {
		normalized -= int64(len(digits) - len(strings.TrimRight(digits, "0")))
	}
	if scale > maxDecimalScale || normalized < -maxDecimalScale {
		return nil, false
	}
	return &Decimal{v, scale}, true
}

func DecimalFromString(s Value) Value {
	text := s.(string)
	if d, ok := parseDecimal(strings.TrimFunc(text, isJSSpace)); ok {
		return Ok(d)
	}
	return decimalError("not a decimal: " + QuoteJSON(text))
}

func DecimalFromBigInt(v, scale Value) Value {
	if s := scale.(float64); safeScale(s) {
		return Ok(&Decimal{v.(*big.Int), int64(s)})
	}
	return decimalError("scale " + ShowNumber(scale.(float64)) + " is not a safe integer")
}

func DecimalFromInteger(v Value) Value { return &Decimal{v.(*big.Int), 0} }

// DecimalFromNumber reads a Float through its JS spelling.
func DecimalFromNumber(n Value) Value {
	x := n.(float64)
	if math.IsNaN(x) || math.IsInf(x, 0) {
		return decimalError(ShowNumber(x) + " is not finite")
	}
	text := ShowNumber(x)
	if text == "-0" {
		text = "0"
	}
	if strings.Contains(text, "e") {
		if d, ok := parseDecimal(text); ok {
			return Ok(d)
		}
		return decimalError(text + " has no decimal form")
	}
	lead, trail, _ := strings.Cut(text, ".")
	v, _ := new(big.Int).SetString(lead+trail, 10)
	return Ok(&Decimal{v, int64(len(trail))})
}

// DecimalToString is Effect's `format`: normalized, in exponent form from
// a scale of 16 either way.
func DecimalToString(d Value) Value {
	n := dec(d).normalize()
	if n.scale >= 16 || n.scale <= -16 {
		return DecimalToExponential(n)
	}
	negative := n.sign() < 0
	abs := new(big.Int).Abs(n.value).String()
	var out string
	if n.scale > 0 {
		digits := strings.Repeat("0", max(0, int(n.scale)+1-len(abs))) + abs
		point := len(digits) - int(n.scale)
		out = digits[:point] + "." + digits[point:]
	} else {
		out = abs + strings.Repeat("0", int(-n.scale))
	}
	if negative {
		return "-" + out
	}
	return out
}

func DecimalNormalString(d Value) Value { return DecimalToString(dec(d).normalize()) }
func DecimalScale(d Value) Value        { return float64(dec(d).scale) }
func DecimalAdd(a, b Value) Value       { return decSum(dec(a), dec(b)) }

func DecimalSubtract(a, b Value) Value {
	y := dec(b)
	return decSum(dec(a), &Decimal{new(big.Int).Neg(y.value), y.scale})
}

func decMultiply(a, b *Decimal) *Decimal {
	if a.sign() == 0 || b.sign() == 0 {
		return &Decimal{new(big.Int), 0}
	}
	return &Decimal{new(big.Int).Mul(a.value, b.value), a.scale + b.scale}
}

func DecimalMultiply(a, b Value) Value {
	x, y := dec(a), dec(b)
	if s := x.scale + y.scale; !safeScale(float64(s)) {
		return decimalError("the product's scale " + strconv.FormatInt(s, 10) + " is not a safe integer")
	}
	return Ok(decMultiply(x, y))
}

func DecimalCompare(a, b Value) Value { return ordering(decCompare(dec(a), dec(b))) }

// roundQuotient is `num / den` rounded to an integer by the mode named;
// false when Exact cannot.
func roundQuotient(num, den *big.Int, mode string) (*big.Int, bool) {
	q, r := new(big.Int).QuoRem(num, den, new(big.Int))
	if r.Sign() == 0 {
		return q, true
	}
	sign := int64(1)
	if (num.Sign() < 0) != (den.Sign() < 0) {
		sign = -1
	}
	half := new(big.Int).Sub(new(big.Int).Mul(big.NewInt(2), new(big.Int).Abs(r)), new(big.Int).Abs(den)).Sign()
	step := func(up bool) (*big.Int, bool) {
		if up {
			return q.Add(q, big.NewInt(sign)), true
		}
		return q, true
	}
	even := new(big.Int).Rem(q, big.NewInt(2)).Sign() == 0
	switch mode {
	case "TowardZero":
		return q, true
	case "AwayFromZero":
		return step(true)
	case "Floor":
		return step(sign < 0)
	case "Ceiling":
		return step(sign > 0)
	case "HalfAwayFromZero":
		return step(half >= 0)
	case "HalfTowardZero":
		return step(half > 0)
	case "HalfFloor":
		return step(half > 0 || half == 0 && sign < 0)
	case "HalfCeiling":
		return step(half > 0 || half == 0 && sign > 0)
	case "HalfEven":
		return step(half > 0 || half == 0 && !even)
	case "HalfOdd":
		return step(half > 0 || half == 0 && even)
	}
	return nil, false
}

// DecimalDivideToScale is `a / b` with `scale` digits, rounded by `mode`.
func DecimalDivideToScale(scale, mode, a, b Value) Value {
	s := scale.(float64)
	if !safeScale(s) {
		return decimalError("scale " + ShowNumber(s) + " is not a safe integer")
	}
	x, y := dec(a), dec(b)
	if y.sign() == 0 {
		return decimalError("division by zero")
	}
	k := y.scale + int64(s) - x.scale
	num, den := x.value, y.value
	if k >= 0 {
		num = new(big.Int).Mul(num, pow10(k))
	} else {
		den = new(big.Int).Mul(den, pow10(-k))
	}
	q, ok := roundQuotient(num, den, ConstructorName(mode.(*Data).Tag))
	if !ok {
		return decimalError("the quotient needs rounding, and the mode is Exact")
	}
	return Ok(&Decimal{q, int64(s)})
}

func DecimalNormalize(d Value) Value { return dec(d).normalize() }

func scaled(scale Value, f func(int64) Value) Value {
	if s := scale.(float64); safeScale(s) {
		return Ok(f(int64(s)))
	}
	return decimalError("scale " + ShowNumber(scale.(float64)) + " is not a safe integer")
}

func DecimalSetScale(scale, d Value) Value {
	return scaled(scale, func(s int64) Value { return dec(d).rescale(s) })
}

func decTruncate(d *Decimal, s int64) *Decimal {
	if d.scale <= s {
		return d
	}
	return d.rescale(s)
}

func decCeil(d *Decimal, s int64) *Decimal {
	t := decTruncate(d, s)
	if d.sign() > 0 && decCompare(t, d) < 0 {
		return decSum(t, &Decimal{bigOne, s})
	}
	return t
}

func decFloor(d *Decimal, s int64) *Decimal {
	t := decTruncate(d, s)
	if d.sign() < 0 && decCompare(t, d) > 0 {
		return decSum(t, &Decimal{big.NewInt(-1), s})
	}
	return t
}

func DecimalTruncate(scale, d Value) Value {
	return scaled(scale, func(s int64) Value { return decTruncate(dec(d), s) })
}
func DecimalCeil(scale, d Value) Value {
	return scaled(scale, func(s int64) Value { return decCeil(dec(d), s) })
}
func DecimalFloor(scale, d Value) Value {
	return scaled(scale, func(s int64) Value { return decFloor(dec(d), s) })
}

// decRound is Effect's `BigDecimal.round`.
func decRound(d *Decimal, s int64, mode string) *Decimal {
	if d.scale <= s {
		return d
	}
	five := func(sign int64) *Decimal { return &Decimal{big.NewInt(5 * sign), s + 1} }
	halfCeil := func() *Decimal { return decFloor(decSum(d, five(1)), s) }
	halfFloor := func() *Decimal { return decCeil(decSum(d, five(-1)), s) }
	switch mode {
	case "Ceiling":
		return decCeil(d, s)
	case "Floor":
		return decFloor(d, s)
	case "TowardZero":
		return decTruncate(d, s)
	case "AwayFromZero":
		if d.sign() > 0 {
			return decCeil(d, s)
		}
		return decFloor(d, s)
	case "HalfCeiling":
		return halfCeil()
	case "HalfFloor":
		return halfFloor()
	case "HalfTowardZero":
		if d.sign() < 0 {
			return halfCeil()
		}
		return halfFloor()
	case "HalfAwayFromZero":
		if d.sign() < 0 {
			return halfFloor()
		}
		return halfCeil()
	}
	up, down := halfCeil(), halfFloor()
	if decCompare(up, down) == 0 {
		return up
	}
	digit := new(big.Int)
	if up.scale >= s {
		digit.Rem(new(big.Int).Quo(up.value, pow10(up.scale-s)), bigTen)
	}
	even := digit.Bit(0) == 0
	if (mode == "HalfEven") == even {
		return up
	}
	return down
}

func DecimalRound(scale, mode, d Value) Value {
	s := scale.(float64)
	if !safeScale(s) {
		return decimalError("scale " + ShowNumber(s) + " is not a safe integer")
	}
	name := ConstructorName(mode.(*Data).Tag)
	if name != "Exact" {
		return Ok(decRound(dec(d), int64(s), name))
	}
	if cut := decTruncate(dec(d), int64(s)); decCompare(cut, dec(d)) == 0 {
		return Ok(cut)
	}
	return decimalError("rounding is needed, and the mode is Exact")
}

func DecimalSumAll(xs Value) Value {
	out := &Decimal{new(big.Int), 0}
	for _, x := range List(xs) {
		out = decSum(out, dec(x))
	}
	return out
}

func DecimalMultiplyAll(xs Value) Value {
	out := &Decimal{bigOne, 0}
	for _, x := range List(xs) {
		if dec(x).sign() == 0 {
			return &Decimal{new(big.Int), 0}
		}
		out = decMultiply(out, dec(x))
	}
	return out
}

// DecimalDivide is `self / that` to 100 significant digits, rounding the
// last half up, as Effect divides.
func DecimalDivide(that, self Value) Value {
	a, b := dec(self), dec(that)
	switch {
	case b.sign() == 0:
		return None
	case a.sign() == 0:
		return Some(&Decimal{new(big.Int), 0})
	}
	scale := a.scale - b.scale
	if a.value.Cmp(b.value) == 0 {
		return Some(&Decimal{bigOne, scale})
	}
	num, den := new(big.Int).Abs(a.value), new(big.Int).Abs(b.value)
	negative := a.sign() != b.sign()
	for num.Cmp(den) < 0 {
		num.Mul(num, bigTen)
		scale++
	}
	q, r := new(big.Int).QuoRem(num, den, new(big.Int))
	if r.Sign() != 0 {
		count := len(q.String())
		r.Mul(r, bigTen)
		for r.Sign() != 0 && count < 100 {
			d, rest := new(big.Int).QuoRem(r, den, new(big.Int))
			q.Add(q.Mul(q, bigTen), d)
			r = rest.Mul(rest, bigTen)
			count++
			scale++
		}
		if r.Sign() != 0 && new(big.Int).Quo(r, den).String()[0] >= '5' {
			q.Add(q, bigOne)
		}
	}
	if negative {
		q.Neg(q)
	}
	return Some(&Decimal{q, scale})
}

func DecimalRemainder(divisor, self Value) Value {
	a, b := dec(self), dec(divisor)
	if b.sign() == 0 {
		return None
	}
	s := max(a.scale, b.scale)
	return Some(&Decimal{new(big.Int).Rem(a.rescale(s).value, b.rescale(s).value), s})
}

func decOrder(a, b Value) int { return decCompare(dec(a), dec(b)) }

func DecimalSign(d Value) Value { return ordering(dec(d).sign()) }

func DecimalAbs(d Value) Value {
	if x := dec(d); x.sign() < 0 {
		return &Decimal{new(big.Int).Neg(x.value), x.scale}
	}
	return d
}

func DecimalNegate(d Value) Value {
	x := dec(d)
	return &Decimal{new(big.Int).Neg(x.value), x.scale}
}

func DecimalToExponential(d Value) Value {
	if dec(d).sign() == 0 {
		return "0e+0"
	}
	n := dec(d).normalize()
	digits := n.value.String()
	point := 1
	if n.sign() < 0 {
		point = 2
	}
	head, tail := digits[:point], digits[point:]
	exp := int64(len(tail)) - n.scale
	out := head
	if tail != "" {
		out += "." + tail
	}
	if exp >= 0 {
		return out + "e+" + strconv.FormatInt(exp, 10)
	}
	return out + "e" + strconv.FormatInt(exp, 10)
}

func DecimalIsInteger(d Value) Value  { return dec(d).normalize().scale <= 0 }
func DecimalIsZero(d Value) Value     { return dec(d).sign() == 0 }
func DecimalIsNegative(d Value) Value { return dec(d).sign() < 0 }
func DecimalIsPositive(d Value) Value { return dec(d).sign() > 0 }
