package flickrt

import (
	"math"
	"math/big"
	"strconv"
	"strings"
)

// The Float values a literal cannot spell in Go.
var (
	NaN     Value = math.NaN()
	Inf     Value = math.Inf(1)
	NegInf  Value = math.Inf(-1)
	NegZero Value = math.Copysign(0, -1)
	Pi      Value = math.Pi
)

// Int arithmetic wraps around in 32 bits, as Go's int32 does; division and
// remainder by zero are 0, and the minimum divided by -1 is the minimum.

func AddI(a, b Value) Value { return a.(int32) + b.(int32) }
func SubI(a, b Value) Value { return a.(int32) - b.(int32) }
func MulI(a, b Value) Value { return a.(int32) * b.(int32) }
func NegI(a Value) Value    { return -a.(int32) }

func DivI(a, b Value) Value {
	x, y := a.(int32), b.(int32)
	if y == 0 {
		return int32(0)
	}
	return x / y
}

func RemI(a, b Value) Value {
	x, y := a.(int32), b.(int32)
	if y == 0 {
		return int32(0)
	}
	return x % y
}

// PowI is `a ** b` on Int; a negative power is truncated like a division.
func PowI(a, b Value) Value {
	base, e := a.(int32), b.(int32)
	if e < 0 {
		switch {
		case base == 1:
			return int32(1)
		case base == -1 && e%2 == 0:
			return int32(1)
		case base == -1:
			return int32(-1)
		}
		return int32(0)
	}
	result := int32(1)
	for u := uint32(e); u > 0; u >>= 1 {
		if u&1 == 1 {
			result *= base
		}
		base *= base
	}
	return result
}

func AddF(a, b Value) Value { return a.(float64) + b.(float64) }
func SubF(a, b Value) Value { return a.(float64) - b.(float64) }
func MulF(a, b Value) Value { return a.(float64) * b.(float64) }
func DivF(a, b Value) Value { return a.(float64) / b.(float64) }
func NegF(a Value) Value    { return -a.(float64) }

// RemF is JS `%`: the remainder with the sign of the dividend.
func RemF(a, b Value) Value { return math.Mod(a.(float64), b.(float64)) }

// PowF is JS `Math.pow`, which differs from math.Pow on ±1 to a NaN or an
// infinite power: NaN.
func PowF(a, b Value) Value {
	x, y := a.(float64), b.(float64)
	if math.IsNaN(y) || (math.Abs(x) == 1 && math.IsInf(y, 0)) {
		return math.NaN()
	}
	return math.Pow(x, y)
}

// BigInt values are never mutated: each operation makes a new one.

func AddB(a, b Value) Value { return new(big.Int).Add(a.(*big.Int), b.(*big.Int)) }
func SubB(a, b Value) Value { return new(big.Int).Sub(a.(*big.Int), b.(*big.Int)) }
func MulB(a, b Value) Value { return new(big.Int).Mul(a.(*big.Int), b.(*big.Int)) }
func NegB(a Value) Value    { return new(big.Int).Neg(a.(*big.Int)) }

// DivB truncates towards zero; division by zero is 0.
func DivB(a, b Value) Value {
	if b.(*big.Int).Sign() == 0 {
		return new(big.Int)
	}
	return new(big.Int).Quo(a.(*big.Int), b.(*big.Int))
}

// RemB has the sign of the dividend; by zero it is 0.
func RemB(a, b Value) Value {
	if b.(*big.Int).Sign() == 0 {
		return new(big.Int)
	}
	return new(big.Int).Rem(a.(*big.Int), b.(*big.Int))
}

// PowB is `a ** b` on BigInt; a negative power is a defect, as JS throws.
func PowB(a, b Value) Value {
	e := b.(*big.Int)
	if e.Sign() < 0 {
		panic(Defect{"RangeError: Exponent must be non-negative"})
	}
	return new(big.Int).Exp(a.(*big.Int), e, nil)
}

// BigLit is a BigInt literal in decimal or `0x`/`0o`/`0b` digits.
func BigLit(text string) Value {
	n, ok := new(big.Int).SetString(text, 0)
	if !ok {
		panic(Defect{"malformed BigInt literal " + text})
	}
	return n
}

// ShowNumber is JS `String(x)`, but `-0` for negative zero like `show`.
func ShowNumber(x float64) string {
	switch {
	case math.IsNaN(x):
		return "NaN"
	case math.IsInf(x, 1):
		return "Infinity"
	case math.IsInf(x, -1):
		return "-Infinity"
	case x == 0 && math.Signbit(x):
		return "-0"
	case x == 0:
		return "0"
	}
	sign := ""
	if x < 0 {
		sign, x = "-", -x
	}
	sci := strconv.FormatFloat(x, 'e', -1, 64)
	mantissa, exp, _ := strings.Cut(sci, "e")
	e, _ := strconv.Atoi(exp)
	digits := evenTie(x, strings.Replace(mantissa, ".", "", 1), e)
	k := len(digits)
	// the decimal point sits after `n` digits
	n := e + 1
	var body string
	switch {
	case k <= n && n <= 21:
		body = digits + strings.Repeat("0", n-k)
	case 0 < n && n <= 21:
		body = digits[:n] + "." + digits[n:]
	case -6 < n && n <= 0:
		body = "0." + strings.Repeat("0", -n) + digits
	default:
		body = digits[:1]
		if k > 1 {
			body += "." + digits[1:]
		}
		if n-1 < 0 {
			body += "e-" + strconv.Itoa(1-n)
		} else {
			body += "e+" + strconv.Itoa(n-1)
		}
	}
	return sign + body
}

// evenTie picks the even digit of an exact tie between two shortest
// spellings, as JS does; the shortest-digit search may pick either.
func evenTie(x float64, digits string, exp int) string {
	last := digits[len(digits)-1]
	if (last-'0')%2 == 0 {
		return digits
	}
	lower := digits[:len(digits)-1] + string(last-1)
	exact := strconv.FormatFloat(x, 'e', 800, 64)
	mantissa, e, _ := strings.Cut(exact, "e")
	exactExp, _ := strconv.Atoi(e)
	tie := exactExp == exp && strings.TrimRight(strings.Replace(mantissa, ".", "", 1), "0") == lower+"5"
	if !tie {
		return digits
	}
	back, err := strconv.ParseFloat(lower[:1]+"."+lower[1:]+"e"+strconv.Itoa(exp), 64)
	if err == nil && back == x {
		return lower
	}
	return digits
}
