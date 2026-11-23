package flickrt

import (
	"math"
	"math/big"
	"strconv"
)

// Special Float32 values that Go cannot spell as typed literals.
var (
	NaN32     Value = float32(math.NaN())
	Inf32     Value = float32(math.Inf(1))
	NegInf32  Value = float32(math.Inf(-1))
	NegZero32 Value = float32(math.Copysign(0, -1))
)

// Unboxers for fixed-width native Go scalars.
func AsInt8(v Value) int8       { return v.(int8) }
func AsInt16(v Value) int16     { return v.(int16) }
func AsInt64(v Value) int64     { return v.(int64) }
func AsUInt8(v Value) uint8     { return v.(uint8) }
func AsUInt16(v Value) uint16   { return v.(uint16) }
func AsUInt32(v Value) uint32   { return v.(uint32) }
func AsUInt64(v Value) uint64   { return v.(uint64) }
func AsFloat32(v Value) float32 { return v.(float32) }

// FieldInt8..FieldFloat32 unbox constructor fields at a dynamic value boundary,
// returning Go's zero value for nil so runtime `tagOf` probing does not panic.
func FieldInt8(v Value) int8 {
	if v == nil {
		return 0
	}
	return v.(int8)
}

func FieldInt16(v Value) int16 {
	if v == nil {
		return 0
	}
	return v.(int16)
}

func FieldInt64(v Value) int64 {
	if v == nil {
		return 0
	}
	return v.(int64)
}

func FieldUInt8(v Value) uint8 {
	if v == nil {
		return 0
	}
	return v.(uint8)
}

func FieldUInt16(v Value) uint16 {
	if v == nil {
		return 0
	}
	return v.(uint16)
}

func FieldUInt32(v Value) uint32 {
	if v == nil {
		return 0
	}
	return v.(uint32)
}

func FieldUInt64(v Value) uint64 {
	if v == nil {
		return 0
	}
	return v.(uint64)
}

func FieldFloat32(v Value) float32 {
	if v == nil {
		return 0
	}
	return v.(float32)
}

// Wrapping arithmetic for fixed-width signed and unsigned integers.
// Go's signed `/` and `%` guarantee MinIntN / -1 == MinIntN and MinIntN % -1 == 0.

func AddInt8(a, b int8) int8 { return a + b }
func SubInt8(a, b int8) int8 { return a - b }
func MulInt8(a, b int8) int8 { return a * b }
func NegInt8(a int8) int8    { return -a }
func DivInt8(a, b int8) int8 {
	if b == 0 {
		return 0
	}
	return a / b
}
func RemInt8(a, b int8) int8 {
	if b == 0 {
		return 0
	}
	return a % b
}
func PowInt8(base, e int8) int8 {
	if e < 0 {
		return negSignedPow8(base, e)
	}
	return int8(powUint64(uint64(uint8(base)), uint64(e)))
}

func negSignedPow8(base, e int8) int8 {
	switch {
	case base == 1:
		return 1
	case base == -1 && e&1 == 0:
		return 1
	case base == -1:
		return -1
	}
	return 0
}

func AddInt16(a, b int16) int16 { return a + b }
func SubInt16(a, b int16) int16 { return a - b }
func MulInt16(a, b int16) int16 { return a * b }
func NegInt16(a int16) int16    { return -a }
func DivInt16(a, b int16) int16 {
	if b == 0 {
		return 0
	}
	return a / b
}
func RemInt16(a, b int16) int16 {
	if b == 0 {
		return 0
	}
	return a % b
}
func PowInt16(base, e int16) int16 {
	if e < 0 {
		switch {
		case base == 1:
			return 1
		case base == -1 && e&1 == 0:
			return 1
		case base == -1:
			return -1
		}
		return 0
	}
	return int16(powUint64(uint64(uint16(base)), uint64(e)))
}

func AddInt64(a, b int64) int64 { return a + b }
func SubInt64(a, b int64) int64 { return a - b }
func MulInt64(a, b int64) int64 { return a * b }
func NegInt64(a int64) int64    { return -a }
func DivInt64(a, b int64) int64 {
	if b == 0 {
		return 0
	}
	return a / b
}
func RemInt64(a, b int64) int64 {
	if b == 0 {
		return 0
	}
	return a % b
}
func PowInt64(base, e int64) int64 {
	if e < 0 {
		switch {
		case base == 1:
			return 1
		case base == -1 && e&1 == 0:
			return 1
		case base == -1:
			return -1
		}
		return 0
	}
	return int64(powUint64(uint64(base), uint64(e)))
}

func AddUInt8(a, b uint8) uint8 { return a + b }
func SubUInt8(a, b uint8) uint8 { return a - b }
func MulUInt8(a, b uint8) uint8 { return a * b }
func NegUInt8(a uint8) uint8    { return -a }
func DivUInt8(a, b uint8) uint8 {
	if b == 0 {
		return 0
	}
	return a / b
}
func RemUInt8(a, b uint8) uint8 {
	if b == 0 {
		return 0
	}
	return a % b
}
func PowUInt8(base, e uint8) uint8 { return uint8(powUint64(uint64(base), uint64(e))) }

func AddUInt16(a, b uint16) uint16 { return a + b }
func SubUInt16(a, b uint16) uint16 { return a - b }
func MulUInt16(a, b uint16) uint16 { return a * b }
func NegUInt16(a uint16) uint16    { return -a }
func DivUInt16(a, b uint16) uint16 {
	if b == 0 {
		return 0
	}
	return a / b
}
func RemUInt16(a, b uint16) uint16 {
	if b == 0 {
		return 0
	}
	return a % b
}
func PowUInt16(base, e uint16) uint16 { return uint16(powUint64(uint64(base), uint64(e))) }

func AddUInt32(a, b uint32) uint32 { return a + b }
func SubUInt32(a, b uint32) uint32 { return a - b }
func MulUInt32(a, b uint32) uint32 { return a * b }
func NegUInt32(a uint32) uint32    { return -a }
func DivUInt32(a, b uint32) uint32 {
	if b == 0 {
		return 0
	}
	return a / b
}
func RemUInt32(a, b uint32) uint32 {
	if b == 0 {
		return 0
	}
	return a % b
}
func PowUInt32(base, e uint32) uint32 { return uint32(powUint64(uint64(base), uint64(e))) }

func AddUInt64(a, b uint64) uint64 { return a + b }
func SubUInt64(a, b uint64) uint64 { return a - b }
func MulUInt64(a, b uint64) uint64 { return a * b }
func NegUInt64(a uint64) uint64    { return -a }
func DivUInt64(a, b uint64) uint64 {
	if b == 0 {
		return 0
	}
	return a / b
}
func RemUInt64(a, b uint64) uint64 {
	if b == 0 {
		return 0
	}
	return a % b
}
func PowUInt64(base, e uint64) uint64 { return powUint64(base, e) }

func powUint64(base, e uint64) uint64 {
	result := uint64(1)
	for e > 0 {
		if e&1 == 1 {
			result *= base
		}
		e >>= 1
		if e > 0 {
			base *= base
		}
	}
	return result
}

// IEEE 754 binary32 arithmetic and equality.
func AddFloat32(a, b float32) float32 { return a + b }
func SubFloat32(a, b float32) float32 { return a - b }
func MulFloat32(a, b float32) float32 { return a * b }
func DivFloat32(a, b float32) float32 { return a / b }
func NegFloat32(a float32) float32    { return -a }
func RemFloat32(a, b float32) float32 { return float32(math.Mod(float64(a), float64(b))) }
func PowFloat32(a, b float32) float32 { return float32(PowFloat(float64(a), float64(b))) }

// EqFloat32 is SameValueZero on Float32: NaN == NaN and -0 == +0.
func EqFloat32(a, b float32) bool { return a == b || (a != a && b != b) }

// ShowInt64 and ShowUInt64 format 64-bit fixed integers as decimal without `n`.
func ShowInt64(n int64) string   { return strconv.FormatInt(n, 10) }
func ShowUInt64(n uint64) string { return strconv.FormatUint(n, 10) }

// 64-bit bitwise and shift primitives (shift counts masked mod 64).
func Int64BitAnd(a, b int64) int64             { return a & b }
func Int64BitOr(a, b int64) int64              { return a | b }
func Int64BitXor(a, b int64) int64             { return a ^ b }
func Int64BitNot(a int64) int64                { return ^a }
func Int64ShiftLeft(a int64, n int32) int64    { return a << (uint32(n) & 63) }
func Int64ShiftRight(a int64, n int32) int64   { return a >> (uint32(n) & 63) }
func Int64ShiftRightUnsigned(a int64, n int32) int64 {
	return int64(uint64(a) >> (uint32(n) & 63))
}

var mask64 = new(big.Int).SetUint64(math.MaxUint64)

func valueToUint64Wrap(v Value) uint64 {
	switch x := v.(type) {
	case int8:
		return uint64(x)
	case int16:
		return uint64(x)
	case int32:
		return uint64(x)
	case int64:
		return uint64(x)
	case uint8:
		return uint64(x)
	case uint16:
		return uint64(x)
	case uint32:
		return uint64(x)
	case uint64:
		return x
	case *big.Int:
		return new(big.Int).And(x, mask64).Uint64()
	}
	panic(Defect{"invalid integer conversion"})
}

func ToInt8(v Value) Value   { return int8(valueToUint64Wrap(v)) }
func ToInt16(v Value) Value  { return int16(valueToUint64Wrap(v)) }
func ToInt32(v Value) Value  { return int32(valueToUint64Wrap(v)) }
func ToInt64(v Value) Value  { return int64(valueToUint64Wrap(v)) }
func ToUInt8(v Value) Value  { return uint8(valueToUint64Wrap(v)) }
func ToUInt16(v Value) Value { return uint16(valueToUint64Wrap(v)) }
func ToUInt32(v Value) Value { return uint32(valueToUint64Wrap(v)) }
func ToUInt64(v Value) Value { return valueToUint64Wrap(v) }

func ToFloat32(v Value) Value {
	switch x := v.(type) {
	case int8:
		return float32(x)
	case int16:
		return float32(x)
	case int32:
		return float32(x)
	case int64:
		return float32(x)
	case uint8:
		return float32(x)
	case uint16:
		return float32(x)
	case uint32:
		return float32(x)
	case uint64:
		return float32(x)
	case float32:
		return x
	case float64:
		return float32(x)
	case *big.Int:
		f, _ := new(big.Float).SetInt(x).Float32()
		return f
	}
	panic(Defect{"invalid Float32 conversion"})
}

func ToFloat64(v Value) Value {
	switch x := v.(type) {
	case int8:
		return float64(x)
	case int16:
		return float64(x)
	case int32:
		return float64(x)
	case int64:
		return float64(x)
	case uint8:
		return float64(x)
	case uint16:
		return float64(x)
	case uint32:
		return float64(x)
	case uint64:
		return float64(x)
	case float32:
		return float64(x)
	case float64:
		return x
	case *big.Int:
		f, _ := new(big.Float).SetInt(x).Float64()
		return f
	}
	panic(Defect{"invalid Float conversion"})
}

func ToBigInt(v Value) Value {
	switch x := v.(type) {
	case int8:
		return big.NewInt(int64(x))
	case int16:
		return big.NewInt(int64(x))
	case int32:
		return big.NewInt(int64(x))
	case int64:
		return big.NewInt(x)
	case uint8:
		return new(big.Int).SetUint64(uint64(x))
	case uint16:
		return new(big.Int).SetUint64(uint64(x))
	case uint32:
		return new(big.Int).SetUint64(uint64(x))
	case uint64:
		return new(big.Int).SetUint64(x)
	case *big.Int:
		return x
	}
	panic(Defect{"invalid BigInt conversion"})
}
