package flickrt

import (
	"math"
	"math/big"
	"strconv"
	"strings"
)

// The foreign functions of `Int`, `Float` and `Numeric`, with the JS
// semantics of the bindings they implement.

// StringFromNumber is JS `String(n)` of a numeric scalar, `-0` kept.
func StringFromNumber(n Value) Value {
	switch x := n.(type) {
	case int8:
		return strconv.FormatInt(int64(x), 10)
	case int16:
		return strconv.FormatInt(int64(x), 10)
	case int32:
		return strconv.Itoa(int(x))
	case int64:
		return strconv.FormatInt(x, 10)
	case uint8:
		return strconv.FormatUint(uint64(x), 10)
	case uint16:
		return strconv.FormatUint(uint64(x), 10)
	case uint32:
		return strconv.FormatUint(uint64(x), 10)
	case uint64:
		return strconv.FormatUint(x, 10)
	case float32:
		return ShowNumber(float64(x))
	default:
		return ShowNumber(n.(float64))
	}
}

var digits = lazyRegexp{pattern: `^[+-]?[0-9]+$`}

// IntParse is `Int.parse`: optional ASCII sign and decimal ASCII digits only,
// in ECMAScript white space and line terminators, in signed 32-bit range, with
// -0 normalized to 0.
func IntParse(s Value) Value {
	text := strings.TrimFunc(s.(string), isJSSpace)
	if !digits.re().MatchString(text) {
		return None
	}
	n, err := strconv.ParseInt(text, 10, 32)
	if err != nil {
		return None
	}
	return Some(int32(n))
}

func IntToFloat(n int32) float64 { return float64(n) }

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
func IntTruncate(n float64) int32 { return toInt32(n) }

func toInt32(x float64) int32 {
	if math.IsNaN(x) || math.IsInf(x, 0) {
		return 0
	}
	return int32(uint32(int64(math.Mod(math.Trunc(x), 4294967296))))
}

func IntAbs(x int32) int32 {
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

func IntUnsafeDiv(a, b int32) int32 {
	if b == 0 {
		panic(Defect{"RangeError: Int.unsafe_div: division by zero"})
	}
	return a / b
}

func IntUnsafeRem(a, b int32) int32 {
	if b == 0 {
		panic(Defect{"RangeError: Int.unsafe_rem: division by zero"})
	}
	return a % b
}

func BitAnd(a, b int32) int32 { return a & b }
func BitOr(a, b int32) int32  { return a | b }
func BitXor(a, b int32) int32 { return a ^ b }
func BitNot(a int32) int32    { return ^a }

// Shifts take the count modulo 32, as JS does.
func ShiftLeft(a, b int32) int32  { return a << (uint32(b) & 31) }
func ShiftRight(a, b int32) int32 { return a >> (uint32(b) & 31) }
func ShiftRightUnsignedInt(a, b int32) int32 {
	return int32(uint32(a) >> (uint32(b) & 31))
}

func NumberStrictEqual(a, b float64) bool { return a == b }

// NumberSameValue is JS `Object.is`: NaN is itself, and 0 is not -0.
func NumberSameValue(x, y float64) bool {
	if math.IsNaN(x) || math.IsNaN(y) {
		return math.IsNaN(x) && math.IsNaN(y)
	}
	return x == y && math.Signbit(x) == math.Signbit(y)
}

func BigIntOfInt(n Value) Value { return big.NewInt(int64(n.(int32))) }

// BigIntOfFloat is `BigInt.from_number`: the integer a number holds
// exactly, of any size, and None for a fraction, NaN or an infinity.
func BigIntOfFloat(x Value) Value {
	if !FloatIsInteger(x.(float64)) {
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
	if !finiteInt.re().MatchString(text) {
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

func FloatIsNaN(x float64) bool    { return math.IsNaN(x) }
func FloatIsFinite(x float64) bool { return !math.IsNaN(x) && !math.IsInf(x, 0) }
func FloatIsInteger(f float64) bool {
	return !math.IsInf(f, 0) && f == math.Trunc(f)
}
func FloatIsSafeInteger(f float64) bool {
	return FloatIsInteger(f) && math.Abs(f) <= 1<<53-1
}

// NumberSafeInteger is `Float.safe_integer`: `n` when it is a safe integer.
func NumberSafeInteger(x Value) Value {
	if FloatIsSafeInteger(x.(float64)) {
		return Some(x)
	}
	return None
}

func FloatAbs(x float64) float64   { return math.Abs(x) }
func FloatFloor(x float64) float64 { return math.Floor(x) }
func FloatCeil(x float64) float64  { return math.Ceil(x) }
func FloatTrunc(x float64) float64 { return math.Trunc(x) }
func FloatSqrt(x float64) float64  { return math.Sqrt(x) }

// FloatLog is JS's `Math.log`: the natural logarithm as fdlibm's
// `__ieee754_log` (e_log.c) computes it, which Go's math.Log does not do
// bit for bit. Each product is converted explicitly, so that no
// architecture fuses it into a multiply-add.
func FloatLog(x float64) float64 {
	const (
		ln2Hi = 6.93147180369123816490e-01
		ln2Lo = 1.90821492927058770002e-10
		two54 = 1.80143985094819840000e+16
		lg1   = 6.666666666666735130e-01
		lg2   = 3.999999999940941908e-01
		lg3   = 2.857142874366239149e-01
		lg4   = 2.222219843214978396e-01
		lg5   = 1.818357216161805012e-01
		lg6   = 1.531383769920937332e-01
		lg7   = 1.479819860511658591e-01
	)
	bits := math.Float64bits(x)
	hx := int32(bits >> 32)
	lx := uint32(bits)
	k := int32(0)
	if hx < 0x00100000 {
		if (uint32(hx&0x7fffffff) | lx) == 0 {
			return math.Inf(-1)
		}
		if hx < 0 {
			return math.NaN()
		}
		k -= 54
		x = float64(x * two54)
		hx = int32(math.Float64bits(x) >> 32)
	}
	if hx >= 0x7ff00000 {
		return x + x
	}
	k += (hx >> 20) - 1023
	hx &= 0x000fffff
	i := (hx + 0x95f64) & 0x100000
	x = math.Float64frombits(uint64(uint32(hx|(i^0x3ff00000)))<<32 | math.Float64bits(x)&0xffffffff)
	k += i >> 20
	f := x - 1.0
	dk := float64(k)
	if (0x000fffff & (2 + hx)) < 3 {
		if f == 0 {
			if k == 0 {
				return 0
			}
			return float64(dk*ln2Hi) + float64(dk*ln2Lo)
		}
		r := float64(float64(f*f) * (0.5 - float64(0.33333333333333333*f)))
		if k == 0 {
			return f - r
		}
		return float64(dk*ln2Hi) - ((r - float64(dk*ln2Lo)) - f)
	}
	s := f / (2.0 + f)
	z := float64(s * s)
	w := float64(z * z)
	t1 := float64(w * (lg2 + float64(w*(lg4+float64(w*lg6)))))
	t2 := float64(z * (lg1 + float64(w*(lg3+float64(w*(lg5+float64(w*lg7)))))))
	r := t2 + t1
	if ((hx - 0x6147a) | (0x6b851 - hx)) > 0 {
		hfsq := float64(float64(0.5*f) * f)
		if k == 0 {
			return f - (hfsq - float64(s*(hfsq+r)))
		}
		return float64(dk*ln2Hi) - ((hfsq - (float64(s*(hfsq+r)) + float64(dk*ln2Lo))) - f)
	}
	if k == 0 {
		return f - float64(s*(f-r))
	}
	return float64(dk*ln2Hi) - ((float64(s*(f-r)) - float64(dk*ln2Lo)) - f)
}

// FloatRound is JS `Math.round`: the nearest integer, halves up, and the
// sign of zero kept. `x - floor(x)` is exact, so no half is misread.
func FloatRound(f float64) float64 {
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
