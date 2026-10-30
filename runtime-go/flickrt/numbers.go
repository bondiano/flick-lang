package flickrt

import (
	"math"
	"math/big"
	"regexp"
	"strconv"
	"strings"
)

// The foreign functions of `Int`, `Float` and `Numeric`, with the JS
// semantics of the bindings they implement.

// StringFromNumber is JS `String(n)` of an Int or a Float, `-0` kept.
func StringFromNumber(n Value) Value {
	if i, ok := n.(int32); ok {
		return strconv.Itoa(int(i))
	}
	return ShowNumber(n.(float64))
}

var digits = regexp.MustCompile(`^[+-]?[0-9]+$`)

// IntParse is the Int the decimal digits spell, with an optional sign.
func IntParse(s Value) Value {
	text := strings.TrimSpace(s.(string))
	if !digits.MatchString(text) {
		return None
	}
	n, err := strconv.ParseInt(text, 10, 32)
	if err != nil {
		return None
	}
	return Some(int32(n))
}

func IntToFloat(n Value) Value { return float64(n.(int32)) }

// IntFromFloat is the Int equal to a Float, if any; -0 is 0.
func IntFromFloat(n Value) Value {
	x := n.(float64)
	if x != math.Trunc(x) || x < math.MinInt32 || x > math.MaxInt32 {
		return None
	}
	return Some(int32(x))
}

// IntTruncate is JS `n | 0`: the fraction dropped, wrapped into 32 bits;
// NaN and the infinities are 0.
func IntTruncate(n Value) Value { return toInt32(n.(float64)) }

func toInt32(x float64) int32 {
	if math.IsNaN(x) || math.IsInf(x, 0) {
		return 0
	}
	return int32(uint32(int64(math.Mod(math.Trunc(x), 4294967296))))
}

func IntAbs(n Value) Value {
	x := n.(int32)
	if x < 0 {
		return -x
	}
	return x
}

var arithmeticTag = Tag("Int.ArithmeticError.ArithmeticError")

func checked(name string, a, b Value, f func(x, y int64) int64) Value {
	x, y := int64(a.(int32)), int64(b.(int32))
	if (name == "checked_div" || name == "checked_rem") && y == 0 {
		return Err(NewData(arithmeticTag, "Int."+name+": division by zero"))
	}
	r := f(x, y)
	if r < math.MinInt32 || r > math.MaxInt32 {
		return Err(NewData(arithmeticTag, "Int."+name+": "+strconv.FormatInt(r, 10)+" does not fit in Int"))
	}
	return Ok(int32(r))
}

func IntCheckedAdd(a, b Value) Value {
	return checked("checked_add", a, b, func(x, y int64) int64 { return x + y })
}
func IntCheckedSub(a, b Value) Value {
	return checked("checked_sub", a, b, func(x, y int64) int64 { return x - y })
}
func IntCheckedMul(a, b Value) Value {
	return checked("checked_mul", a, b, func(x, y int64) int64 { return x * y })
}
func IntCheckedDiv(a, b Value) Value {
	return checked("checked_div", a, b, func(x, y int64) int64 { return x / y })
}
func IntCheckedRem(a, b Value) Value {
	return checked("checked_rem", a, b, func(x, y int64) int64 { return x % y })
}

func IntUnsafeDiv(a, b Value) Value {
	if b.(int32) == 0 {
		panic(Defect{"RangeError: Int.unsafe_div: division by zero"})
	}
	return a.(int32) / b.(int32)
}

func IntUnsafeRem(a, b Value) Value {
	if b.(int32) == 0 {
		panic(Defect{"RangeError: Int.unsafe_rem: division by zero"})
	}
	return a.(int32) % b.(int32)
}

func BitAnd(a, b Value) Value { return a.(int32) & b.(int32) }
func BitOr(a, b Value) Value  { return a.(int32) | b.(int32) }
func BitXor(a, b Value) Value { return a.(int32) ^ b.(int32) }
func BitNot(a Value) Value    { return ^a.(int32) }

// Shifts take the count modulo 32, as JS does.
func ShiftLeft(a, b Value) Value  { return a.(int32) << (uint32(b.(int32)) & 31) }
func ShiftRight(a, b Value) Value { return a.(int32) >> (uint32(b.(int32)) & 31) }
func ShiftRightUnsignedInt(a, b Value) Value {
	return int32(uint32(a.(int32)) >> (uint32(b.(int32)) & 31))
}

func NumberStrictEqual(a, b Value) Value { return a.(float64) == b.(float64) }

// NumberSameValue is JS `Object.is`: NaN is itself, and 0 is not -0.
func NumberSameValue(a, b Value) Value {
	x, y := a.(float64), b.(float64)
	if math.IsNaN(x) || math.IsNaN(y) {
		return math.IsNaN(x) && math.IsNaN(y)
	}
	return x == y && math.Signbit(x) == math.Signbit(y)
}

func BigIntOfInt(n Value) Value { return big.NewInt(int64(n.(int32))) }

// BigIntOfFloat is `BigInt.from_number`: the integer a number holds
// exactly, of any size, and None for a fraction, NaN or an infinity.
func BigIntOfFloat(x Value) Value {
	if FloatIsInteger(x) != true {
		return None
	}
	n, _ := new(big.Float).SetFloat64(x.(float64)).Int(nil)
	return Some(n)
}

// BigIntString is JS `String(n)`: the decimal digits without `n`.
func BigIntString(n Value) Value { return n.(*big.Int).String() }

// BigIntParse is `BigInt.parse`: optional sign and decimal digits only,
// in JS white space.
func BigIntParse(s Value) Value {
	text := strings.TrimFunc(s.(string), isJSSpace)
	if !finiteInt.MatchString(text) {
		return None
	}
	n, _ := new(big.Int).SetString(text, 10)
	return Some(n)
}

// FloatOfBigInt is JS `Number(n)`: the nearest Float.
func FloatOfBigInt(n Value) Value {
	f, _ := new(big.Float).SetInt(n.(*big.Int)).Float64()
	return f
}

func FloatIsNaN(x Value) Value    { return math.IsNaN(x.(float64)) }
func FloatIsFinite(x Value) Value { return !math.IsNaN(x.(float64)) && !math.IsInf(x.(float64), 0) }
func FloatIsInteger(x Value) Value {
	f := x.(float64)
	return !math.IsInf(f, 0) && f == math.Trunc(f)
}
func FloatIsSafeInteger(x Value) Value {
	f := x.(float64)
	return FloatIsInteger(x).(bool) && math.Abs(f) <= 1<<53-1
}
func FloatAbs(x Value) Value   { return math.Abs(x.(float64)) }
func FloatFloor(x Value) Value { return math.Floor(x.(float64)) }
func FloatCeil(x Value) Value  { return math.Ceil(x.(float64)) }
func FloatTrunc(x Value) Value { return math.Trunc(x.(float64)) }
func FloatSqrt(x Value) Value  { return math.Sqrt(x.(float64)) }

// FloatRound is JS `Math.round`: the nearest integer, halves up, and the
// sign of zero kept. `x - floor(x)` is exact, so no half is misread.
func FloatRound(x Value) Value {
	f := x.(float64)
	if math.IsNaN(f) || math.IsInf(f, 0) {
		return f
	}
	r := math.Floor(f)
	if f-r >= 0.5 {
		r++
	}
	if r == 0 && math.Signbit(f) {
		return math.Copysign(0, -1)
	}
	return r
}

func Identity(v Value) Value { return v }
