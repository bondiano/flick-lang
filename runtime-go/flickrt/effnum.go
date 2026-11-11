package flickrt

import (
	"math"
	"math/big"
	"strconv"
	"strings"
)

// The `Number`, `BigInt` and `BigDecimal` functions the standard library
// binds. A data-last function, `f(a)(self)`, takes `a`, then `self`.

// numberOrder is the order of numbers: NaN is less than every other number
// and equal to itself, and -0 equals 0.
func numberOrder(a, b Value) int {
	x, y := a.(float64), b.(float64)
	switch {
	case x == y, math.IsNaN(x) && math.IsNaN(y):
		return 0
	case math.IsNaN(x), x < y:
		return -1
	}
	return 1
}

func bigOrder(a, b Value) int { return a.(*big.Int).Cmp(b.(*big.Int)) }

// The comparisons of an order.
func less(o func(a, b Value) int, a, self Value) Value      { return o(self, a) == -1 }
func lessEq(o func(a, b Value) int, a, self Value) Value    { return o(self, a) != 1 }
func greater(o func(a, b Value) int, a, self Value) Value   { return o(self, a) == 1 }
func greaterEq(o func(a, b Value) int, a, self Value) Value { return o(self, a) != -1 }

func orderMin(o func(a, b Value) int, self, that Value) Value {
	if o(self, that) < 1 {
		return self
	}
	return that
}

func orderMax(o func(a, b Value) int, self, that Value) Value {
	if o(self, that) > -1 {
		return self
	}
	return that
}

func bounds(options Value) (Value, Value) {
	r := record(options)
	return r.Get("minimum"), r.Get("maximum")
}

func between(o func(a, b Value) int, options, self Value) Value {
	lo, hi := bounds(options)
	return o(self, lo) != -1 && o(self, hi) != 1
}

func clamp(o func(a, b Value) int, options, self Value) Value {
	lo, hi := bounds(options)
	return orderMin(o, hi, orderMax(o, lo, self))
}

func NumberMin(a, self Value) Value        { return orderMin(numberOrder, self, a) }
func NumberMax(a, self Value) Value        { return orderMax(numberOrder, self, a) }
func NumberSum(a, self Value) Value        { return self.(float64) + a.(float64) }
func NumberSubtract(a, self Value) Value   { return self.(float64) - a.(float64) }
func NumberMultiply(a, self Value) Value   { return self.(float64) * a.(float64) }
func NumberIncrement(n Value) Value        { return n.(float64) + 1 }
func NumberDecrement(n Value) Value        { return n.(float64) - 1 }
func NumberLess(a, self Value) Value       { return less(numberOrder, a, self) }
func NumberLessEq(a, self Value) Value     { return lessEq(numberOrder, a, self) }
func NumberGreater(a, self Value) Value    { return greater(numberOrder, a, self) }
func NumberGreaterEq(a, self Value) Value  { return greaterEq(numberOrder, a, self) }
func NumberBetween(options, x Value) Value { return between(numberOrder, options, x) }
func NumberClamp(options, x Value) Value   { return clamp(numberOrder, options, x) }
func NumberRoundTo(precision, x Value) Value {
	factor := math.Pow(10, precision.(float64))
	return FloatRound(x.(float64)*factor).(float64) / factor
}

func NumberDivide(a, self Value) Value {
	if a.(float64) == 0 {
		return None
	}
	return Some(self.(float64) / a.(float64))
}

func NumberSumAll(xs Value) Value {
	out := 0.0
	for _, x := range List(xs) {
		out += x.(float64)
	}
	return out
}

func NumberMultiplyAll(xs Value) Value {
	out := 1.0
	for _, x := range List(xs) {
		if x.(float64) == 0 {
			return 0.0
		}
		out *= x.(float64)
	}
	return out
}

// NumberSign is `Float.sign`: NaN is less.
func NumberSign(n Value) Value {
	x := n.(float64)
	switch {
	case x > 0:
		return GT
	case x == 0:
		return EQ
	}
	return LT
}

func NumberNextPow2(n Value) Value {
	return math.Max(math.Pow(2, math.Ceil(math.Log(n.(float64))/math.Log(2))), 2)
}

// NumberParse is JS `Number(s)` but blank is nothing, and the names of the
// non-finite numbers are read.
func NumberParse(s Value) Value {
	switch text := s.(string); text {
	case "NaN":
		return Some(math.NaN())
	case "Infinity":
		return Some(math.Inf(1))
	case "-Infinity":
		return Some(math.Inf(-1))
	}
	return StringToNumber(s)
}

// shortest is the shortest digits of |x| and the exponent of their last.
func shortest(x float64) (*big.Int, int) {
	sci := strconv.FormatFloat(math.Abs(x), 'e', -1, 64)
	mantissa, exp, _ := strings.Cut(sci, "e")
	e, _ := strconv.Atoi(exp)
	digits := evenTie(math.Abs(x), strings.Replace(mantissa, ".", "", 1), e)
	coefficient, _ := new(big.Int).SetString(digits, 10)
	if x < 0 {
		coefficient.Neg(coefficient)
	}
	return coefficient, e - (len(digits) - 1)
}

// NumberRemainder is the remainder, exact on the decimal spellings of
// both, with the sign of `self`.
func NumberRemainder(divisor, self Value) Value {
	x, y := self.(float64), divisor.(float64)
	if math.IsNaN(x) || math.IsInf(x, 0) || math.IsNaN(y) || math.IsInf(y, 0) || y == 0 {
		return math.NaN()
	}
	if x == math.Trunc(x) && y == math.Trunc(y) {
		return math.Mod(x, y)
	}
	xc, xe := shortest(x)
	yc, ye := shortest(y)
	if x == math.Trunc(x) {
		xc, xe = exactInt(x), 0
	}
	if y == math.Trunc(y) {
		yc, ye = exactInt(y), 0
	}
	e := min(xe, ye)
	xi := new(big.Int).Mul(xc, pow10(int64(xe-e)))
	yi := new(big.Int).Mul(yc, pow10(int64(ye-e)))
	out := new(big.Int).Rem(xi, yi)
	if out.Sign() == 0 {
		if x < 0 || math.Signbit(x) {
			return math.Copysign(0, -1)
		}
		return 0.0
	}
	r, _ := strconv.ParseFloat(out.String()+"e"+strconv.Itoa(e), 64)
	if r == 0 {
		return math.Copysign(math.SmallestNonzeroFloat64, x)
	}
	return r
}

// exactInt is an integral Float as a BigInt, every digit kept.
func exactInt(x float64) *big.Int {
	n, _ := new(big.Float).SetFloat64(x).Int(nil)
	return n
}

func big2(f func(z, x, y *big.Int) *big.Int, a, b Value) Value {
	return f(new(big.Int), a.(*big.Int), b.(*big.Int))
}

func BigIntSum(a, self Value) Value      { return big2((*big.Int).Add, self, a) }
func BigIntSubtract(a, self Value) Value { return big2((*big.Int).Sub, self, a) }
func BigIntMultiply(a, self Value) Value { return big2((*big.Int).Mul, self, a) }
func BigIntIncrement(n Value) Value      { return new(big.Int).Add(n.(*big.Int), bigOne) }
func BigIntDecrement(n Value) Value      { return new(big.Int).Sub(n.(*big.Int), bigOne) }
func BigIntAbs(n Value) Value            { return new(big.Int).Abs(n.(*big.Int)) }
func BigIntMin(a, self Value) Value      { return orderMin(bigOrder, self, a) }
func BigIntMax(a, self Value) Value      { return orderMax(bigOrder, self, a) }
func BigIntLess(a, self Value) Value     { return less(bigOrder, a, self) }
func BigIntLessEq(a, self Value) Value   { return lessEq(bigOrder, a, self) }
func BigIntGreater(a, self Value) Value  { return greater(bigOrder, a, self) }
func BigIntGreaterEq(a, self Value) Value {
	return greaterEq(bigOrder, a, self)
}
func BigIntBetween(options, x Value) Value { return between(bigOrder, options, x) }
func BigIntClamp(options, x Value) Value   { return clamp(bigOrder, options, x) }
func BigIntSign(n Value) Value             { return ordering(n.(*big.Int).Sign()) }

func bigintError(message string) Value { return Err(NewData(arithmeticTag, message)) }

// BigIntDivide and BigIntRemainder truncate towards zero; by zero they are
// an error.
func BigIntDivide(a, b Value) Value {
	if b.(*big.Int).Sign() == 0 {
		return bigintError("division by zero")
	}
	return Ok(big2((*big.Int).Quo, a, b))
}

func BigIntRemainder(a, b Value) Value {
	if b.(*big.Int).Sign() == 0 {
		return bigintError("division by zero")
	}
	return Ok(big2((*big.Int).Rem, a, b))
}

func BigIntGcd(a, self Value) Value { return new(big.Int).Abs(gcd(self.(*big.Int), a.(*big.Int))) }

func gcd(a, b *big.Int) *big.Int {
	x, y := new(big.Int).Set(a), new(big.Int).Set(b)
	for y.Sign() != 0 {
		x, y = y, new(big.Int).Rem(x, y)
	}
	return x
}

func BigIntLcm(a, self Value) Value {
	x, y := self.(*big.Int), a.(*big.Int)
	if x.Sign() == 0 || y.Sign() == 0 {
		return new(big.Int)
	}
	p := new(big.Int).Mul(x, y)
	return p.Abs(p.Quo(p, new(big.Int).Abs(gcd(x, y))))
}

// BigIntSqrt is the floor of the square root, nothing below zero.
func BigIntSqrt(n Value) Value {
	x := n.(*big.Int)
	if x.Sign() < 0 {
		return None
	}
	return Some(new(big.Int).Sqrt(x))
}

func BigIntSumAll(xs Value) Value {
	out := new(big.Int)
	for _, x := range List(xs) {
		out.Add(out, x.(*big.Int))
	}
	return out
}

func BigIntMultiplyAll(xs Value) Value {
	out := big.NewInt(1)
	for _, x := range List(xs) {
		if x.(*big.Int).Sign() == 0 {
			return new(big.Int)
		}
		out.Mul(out, x.(*big.Int))
	}
	return out
}

var maxSafeBig = big.NewInt(maxSafe)

func BigIntToNumber(n Value) Value {
	x := n.(*big.Int)
	if new(big.Int).Abs(x).Cmp(maxSafeBig) > 0 {
		return None
	}
	return Some(float64(x.Int64()))
}

func BigIntFromNumber(n Value) Value {
	x := n.(float64)
	if math.Abs(x) > maxSafe || x != math.Trunc(x) {
		return None
	}
	return Some(big.NewInt(int64(x)))
}

// BigIntFromString is JS `BigInt(s)`: decimal with a sign, or unsigned
// `0x`/`0o`/`0b` digits, in white space.
func BigIntFromString(s Value) Value {
	text := strings.TrimFunc(s.(string), isJSSpace)
	if text == "" {
		return None
	}
	if radix.MatchString(text) {
		n, _ := new(big.Int).SetString(text, 0)
		return Some(n)
	}
	if !finiteInt.MatchString(text) {
		return None
	}
	n, _ := new(big.Int).SetString(strings.TrimPrefix(text, "+"), 10)
	return Some(n)
}

func DecimalMin(a, self Value) Value        { return orderMin(decOrder, self, a) }
func DecimalMax(a, self Value) Value        { return orderMax(decOrder, self, a) }
func DecimalLess(a, self Value) Value       { return less(decOrder, a, self) }
func DecimalLessEq(a, self Value) Value     { return lessEq(decOrder, a, self) }
func DecimalGreater(a, self Value) Value    { return greater(decOrder, a, self) }
func DecimalGreaterEq(a, self Value) Value  { return greaterEq(decOrder, a, self) }
func DecimalBetween(options, x Value) Value { return between(decOrder, options, x) }
func DecimalClamp(options, x Value) Value   { return clamp(decOrder, options, x) }
