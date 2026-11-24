package flickrt

import (
	"math"
	"strconv"
)

// Ordering converts a -1/0/+1 comparison integer to Flick's LT/EQ/GT singleton.
func Ordering(c int) Value { return ordering(c) }

// ShowArg parenthesizes a constructor argument string when it contains a space
// and does not start with a bracket or quote, matching Instances.show_arg.
func ShowArg(text string) string { return atom(text) }

// CombineHash folds a field hash into an ADT/record hash accumulator (h*31 + x).
func CombineHash(h, x int32) int32 { return combine(h, x) }

func cmpOrdered[T ~int8 | ~int16 | ~int32 | ~int64 | ~uint8 | ~uint16 | ~uint32 | ~uint64](a, b T) int {
	if a < b {
		return -1
	}
	if a > b {
		return 1
	}
	return 0
}

// Typed total-order comparisons returning -1/0/+1 for closed ADT Ord helpers.
func CmpInt(a, b int32) int       { return cmpOrdered(a, b) }
func CmpInt8(a, b int8) int       { return cmpOrdered(a, b) }
func CmpInt16(a, b int16) int     { return cmpOrdered(a, b) }
func CmpInt64(a, b int64) int     { return cmpOrdered(a, b) }
func CmpUInt8(a, b uint8) int     { return cmpOrdered(a, b) }
func CmpUInt16(a, b uint16) int   { return cmpOrdered(a, b) }
func CmpUInt32(a, b uint32) int   { return cmpOrdered(a, b) }
func CmpUInt64(a, b uint64) int   { return cmpOrdered(a, b) }
func CmpBool(a, b bool) int       { return boolInt(a) - boolInt(b) }
func CmpString(a, b string) int   { return CompareStrings(a, b) }

func CmpFloat(a, b float64) int {
	xn, yn := math.IsNaN(a), math.IsNaN(b)
	if xn || yn {
		return boolInt(xn) - boolInt(yn)
	}
	if a < b {
		return -1
	}
	if a > b {
		return 1
	}
	return 0
}

func CmpFloat32(a, b float32) int {
	xn, yn := math.IsNaN(float64(a)), math.IsNaN(float64(b))
	if xn || yn {
		return boolInt(xn) - boolInt(yn)
	}
	if a < b {
		return -1
	}
	if a > b {
		return 1
	}
	return 0
}

// EqualFloat32 compares two float32 values under SameValue (strictZero=true)
// or SameValueZero (strictZero=false).
func EqualFloat32(a, b float32, strictZero bool) bool {
	return EqualFloat(float64(a), float64(b), strictZero)
}

// Fixed-width scalar Compare/Hash/Show helpers matching Compare/Hash/Show.
func CompareInt8(a, b int8) Value       { return ordering(cmpOrdered(a, b)) }
func CompareInt16(a, b int16) Value     { return ordering(cmpOrdered(a, b)) }
func CompareInt64(a, b int64) Value     { return ordering(cmpOrdered(a, b)) }
func CompareUInt8(a, b uint8) Value     { return ordering(cmpOrdered(a, b)) }
func CompareUInt16(a, b uint16) Value   { return ordering(cmpOrdered(a, b)) }
func CompareUInt32(a, b uint32) Value   { return ordering(cmpOrdered(a, b)) }
func CompareUInt64(a, b uint64) Value   { return ordering(cmpOrdered(a, b)) }
func CompareFloat32(a, b float32) Value { return ordering(CmpFloat32(a, b)) }

func HashInt8(x int8) int32       { return optimize(int32(x)) }
func HashInt16(x int16) int32     { return optimize(int32(x)) }
func HashInt64(x int64) int32     { return hashString(strconv.FormatInt(x, 10)) }
func HashUInt8(x uint8) int32     { return optimize(int32(x)) }
func HashUInt16(x uint16) int32   { return optimize(int32(x)) }
func HashUInt32(x uint32) int32   { return hashNumber(float64(x)) }
func HashUInt64(x uint64) int32   { return hashString(strconv.FormatUint(x, 10)) }
func HashFloat32(x float32) int32 { return hashNumber(float64(x)) }

func ShowInt8(x int8) string       { return strconv.FormatInt(int64(x), 10) }
func ShowInt16(x int16) string     { return strconv.FormatInt(int64(x), 10) }
func ShowUInt8(x uint8) string     { return strconv.FormatUint(uint64(x), 10) }
func ShowUInt16(x uint16) string   { return strconv.FormatUint(uint64(x), 10) }
func ShowUInt32(x uint32) string   { return strconv.FormatUint(uint64(x), 10) }
func ShowFloat32(x float32) string { return ShowNumber(float64(x)) }

// Typed Option derived-operation and boundary helpers for Option fields inside
// by-value closed ADTs.
func EqualWithOptFloat(a, b Opt[float64], strictZero bool) bool {
	if a.Ok != b.Ok {
		return false
	}
	return !a.Ok || EqualFloat(a.Val, b.Val, strictZero)
}

func EqualWithOpt[T any](a, b Opt[T], eq func(T, T, bool) bool, strictZero bool) bool {
	if a.Ok != b.Ok {
		return false
	}
	return !a.Ok || eq(a.Val, b.Val, strictZero)
}

func CmpOptBool(a, b OptBool) int {
	if a < b {
		return -1
	}
	if a > b {
		return 1
	}
	return 0
}

func HashOptBool(o OptBool) int32 {
	switch o {
	case OptBoolFalse:
		return hashFalse
	case OptBoolTrue:
		return hashTrue
	default:
		return 0
	}
}

func ShowOptBool(o OptBool) string {
	switch o {
	case OptBoolFalse:
		return "Some false"
	case OptBoolTrue:
		return "Some true"
	default:
		return "None"
	}
}

func CmpOpt[T any](a, b Opt[T], cmp func(T, T) int) int {
	if !a.Ok && !b.Ok {
		return 0
	}
	if !a.Ok {
		return -1
	}
	if !b.Ok {
		return 1
	}
	return cmp(a.Val, b.Val)
}

func HashOpt[T any](o Opt[T], h func(T) int32) int32 {
	if !o.Ok {
		return 0
	}
	return h(o.Val)
}

func ShowOpt[T any](o Opt[T], s func(T) string) string {
	if !o.Ok {
		return "None"
	}
	return "Some " + atom(s(o.Val))
}

func FieldOptBool(v Value) OptBool {
	if v == nil {
		return OptBoolNone
	}
	return AsOptBool(v)
}

func FieldOptInt(v Value) Opt[int32] {
	if v == nil {
		return Opt[int32]{}
	}
	return AsOptInt(v)
}

func FieldOptFloat(v Value) Opt[float64] {
	if v == nil {
		return Opt[float64]{}
	}
	return AsOptFloat(v)
}

func FieldOptString(v Value) Opt[string] {
	if v == nil {
		return Opt[string]{}
	}
	return AsOptString(v)
}

func FieldOpt[T any](v Value, unbox func(Value) T) Opt[T] {
	if v == nil {
		return Opt[T]{}
	}
	return UnboxOpt(v, unbox)
}
