package flickrt

// Typed allocation-free Option representations for concrete Flick Option types
// on the Go target.
//
// Two families exist:
//   - Sentinel layouts (OptBool, OptRef), used only when the concrete payload
//     layout excludes the sentinel from every valid Some value.
//   - Presence-flag layout (Opt[T]), used for payloads with no spare sentinel
//     (Int, Float, String, nested Option, and concrete payloads that may be nil).
//
// Every representation distinguishes all nesting levels (including Some None)
// and converts back to the erased *Data representation (*Data{Tag: "None"} /
// *Data{Tag: "Some", ...}) at erased calls, runtime observations, and FFI.

// Opt represents a typed Option whose payload layout has no None sentinel.
// The zero value is None (Ok == false).
type Opt[T any] struct {
	Val T
	Ok  bool
}

// SomeOpt constructs a present Opt[T] without allocation.
func SomeOpt[T any](v T) Opt[T] { return Opt[T]{Val: v, Ok: true} }

// NoneOpt constructs an absent Opt[T] without allocation.
func NoneOpt[T any]() Opt[T] { return Opt[T]{} }

func (o Opt[T]) IsSome() bool { return o.Ok }
func (o Opt[T]) IsNone() bool { return !o.Ok }
func (o Opt[T]) Unwrap() T    { return o.Val }

// OptBool represents Option Bool in a single byte using 0 as the None sentinel,
// 1 as Some(false), and 2 as Some(true).
type OptBool uint8

const (
	OptBoolNone  OptBool = 0
	OptBoolFalse OptBool = 1
	OptBoolTrue  OptBool = 2
)

// SomeBool constructs a present OptBool without allocation.
func SomeBool(v bool) OptBool {
	if v {
		return OptBoolTrue
	}
	return OptBoolFalse
}

func (o OptBool) IsSome() bool { return o != OptBoolNone }
func (o OptBool) IsNone() bool { return o == OptBoolNone }
func (o OptBool) Unwrap() bool { return o == OptBoolTrue }

// ToValue converts OptBool to the erased *Data representation.
func (o OptBool) ToValue() Value {
	if o == OptBoolNone {
		return None
	}
	return Some(o == OptBoolTrue)
}

// BoxOptBool converts OptBool to the erased *Data representation.
func BoxOptBool(o OptBool) Value { return o.ToValue() }

// AsOptBool converts an erased Flick Option Bool to OptBool.
func AsOptBool(v Value) OptBool {
	if IsNone(v) {
		return OptBoolNone
	}
	return SomeBool(AsBool(UnwrapSome(v)))
}

// OptRef represents a typed Option whose concrete non-Option payload is stored
// as a guaranteed non-nil flickrt.Value (records, tuples, lists, BigInt, Ref,
// functions, Result, and non-Option tagged ADTs). Its zero value (val == nil)
// is the None sentinel.
type OptRef struct {
	val Value
}

// NoneRef is the zero OptRef sentinel representing None.
var NoneRef = OptRef{}

// SomeRef constructs a present OptRef from a non-nil Flick payload.
func SomeRef(v Value) OptRef { return OptRef{val: v} }

func (o OptRef) IsSome() bool  { return o.val != nil }
func (o OptRef) IsNone() bool  { return o.val == nil }
func (o OptRef) Unwrap() Value { return o.val }

// ToValue converts OptRef to the erased *Data representation, preserving the
// Some wrapper around the payload.
func (o OptRef) ToValue() Value {
	if o.val == nil {
		return None
	}
	return Some(o.val)
}

// BoxOptRef converts OptRef to the erased *Data representation.
func BoxOptRef(o OptRef) Value { return o.ToValue() }

// AsOptRef converts an erased Flick Option to OptRef.
func AsOptRef(v Value) OptRef {
	if IsNone(v) {
		return OptRef{}
	}
	return OptRef{val: UnwrapSome(v)}
}

// Leaf boxing and unboxing helpers for Opt[T] payloads.
func BoxOptInt(o Opt[int32]) Value {
	if !o.Ok {
		return None
	}
	return Some(o.Val)
}

func BoxOptFloat(o Opt[float64]) Value {
	if !o.Ok {
		return None
	}
	return Some(o.Val)
}

func BoxOptString(o Opt[string]) Value {
	if !o.Ok {
		return None
	}
	return Some(o.Val)
}

func BoxOptValue(o Opt[Value]) Value {
	if !o.Ok {
		return None
	}
	return Some(o.Val)
}

func AsOptInt(v Value) Opt[int32] {
	if IsNone(v) {
		return Opt[int32]{}
	}
	return Opt[int32]{Val: AsInt(UnwrapSome(v)), Ok: true}
}

func AsOptFloat(v Value) Opt[float64] {
	if IsNone(v) {
		return Opt[float64]{}
	}
	return Opt[float64]{Val: AsFloat(UnwrapSome(v)), Ok: true}
}

func AsOptString(v Value) Opt[string] {
	if IsNone(v) {
		return Opt[string]{}
	}
	return Opt[string]{Val: AsString(UnwrapSome(v)), Ok: true}
}

func AsOptValue(v Value) Opt[Value] {
	if IsNone(v) {
		return Opt[Value]{}
	}
	return Opt[Value]{Val: UnwrapSome(v), Ok: true}
}

// BoxOpt converts a nested Opt[T] to the erased *Data representation while
// preserving every nesting level (including Some None).
func BoxOpt[T any](o Opt[T], boxInner func(T) Value) Value {
	if !o.Ok {
		return None
	}
	return Some(boxInner(o.Val))
}

// UnboxOpt converts an erased Flick Option to a nested Opt[T].
func UnboxOpt[T any](v Value, unboxInner func(Value) T) Opt[T] {
	if IsNone(v) {
		return Opt[T]{}
	}
	return Opt[T]{Val: unboxInner(UnwrapSome(v)), Ok: true}
}

// Typed structural equality helpers matching Flick's `==` (SameValueZero)
// without allocating.
func EqOptFloat(a, b Opt[float64]) bool {
	if a.Ok != b.Ok {
		return false
	}
	return !a.Ok || EqFloat(a.Val, b.Val)
}

func EqOptRef(a, b OptRef) bool {
	if a.val == nil || b.val == nil {
		return a.val == b.val
	}
	return Equals(a.val, b.val)
}

func EqOptValue(a, b Opt[Value]) bool {
	if a.Ok != b.Ok {
		return false
	}
	return !a.Ok || Equals(a.Val, b.Val)
}

func EqOpt[T any](a, b Opt[T], eq func(T, T) bool) bool {
	if a.Ok != b.Ok {
		return false
	}
	return !a.Ok || eq(a.Val, b.Val)
}
