package flickrt

import (
	"context"
	"fmt"
	"reflect"
)

// BytesToGo copies an immutable Flick Bytes value into a fresh Go []byte slice.
// Mutating the returned slice never affects the Flick Bytes value.
func BytesToGo(v Value) []byte {
	s := string(v.(packedBytes))
	if len(s) == 0 {
		return []byte{}
	}
	out := make([]byte, len(s))
	copy(out, s)
	return out
}

// BytesFromGo copies a Go []byte slice into an immutable Flick Bytes value.
// Mutating b after the call never affects the returned Flick Bytes value.
func BytesFromGo(b []byte) Value {
	if len(b) == 0 {
		return BytesEmpty
	}
	return packedBytes(string(b))
}

// ListToGo copies a Flick List into a fresh Go slice []T, converting each element.
func ListToGo[T any](v Value, elem func(Value) T) []T {
	xs := List(v)
	out := make([]T, len(xs))
	for i, item := range xs {
		out[i] = elem(item)
	}
	return out
}

// ListFromGo copies a Go slice []T into a fresh Flick List, converting each element.
func ListFromGo[T any](xs []T, elem func(T) Value) Value {
	out := make([]Value, len(xs))
	for i, item := range xs {
		out[i] = elem(item)
	}
	return out
}

func FromInt(v int32) Value         { return v }
func FromFloat(v float64) Value     { return v }
func FromBool(v bool) Value         { return v }
func FromString(v string) Value     { return v }
func FromUnit(_ struct{}) Value     { return Unit }
func AsUnit(_ Value) struct{}       { return struct{}{} }
func IdentityValue(v Value) Value   { return v }
func Ptr[T any](v T) *T             { return &v }
func OptToPtr[T any](o Opt[T]) *T {
	if !o.Ok {
		return nil
	}
	v := o.Val
	return &v
}

func PtrToOpt[T any](p *T) Opt[T] {
	if p == nil {
		return Opt[T]{}
	}
	return Opt[T]{Val: *p, Ok: true}
}

func FlagToOpt[T any](v T, ok bool) Opt[T] {
	if !ok {
		return Opt[T]{}
	}
	return Opt[T]{Val: v, Ok: true}
}

func OptBoolToPtr(o OptBool) *bool {
	if o == OptBoolNone {
		return nil
	}
	v := o == OptBoolTrue
	return &v
}

func PtrToOptBool(p *bool) OptBool {
	if p == nil {
		return OptBoolNone
	}
	return SomeBool(*p)
}

func FlagToOptBool(v bool, ok bool) OptBool {
	if !ok {
		return OptBoolNone
	}
	return SomeBool(v)
}

func OptRefToPtr[T any](o OptRef, conv func(Value) T) *T {
	if o.val == nil {
		return nil
	}
	v := conv(o.val)
	return &v
}

func PtrToOptRef[T any](p *T, conv func(T) Value) OptRef {
	if p == nil {
		return NoneRef
	}
	return SomeRef(conv(*p))
}

func FlagToOptRef[T any](v T, ok bool, conv func(T) Value) OptRef {
	if !ok {
		return NoneRef
	}
	return SomeRef(conv(v))
}

func OptValueToPtr[T any](o Opt[Value], conv func(Value) T) *T {
	if !o.Ok {
		return nil
	}
	v := conv(o.Val)
	return &v
}

func PtrToOptValue[T any](p *T, conv func(T) Value) Opt[Value] {
	if p == nil {
		return Opt[Value]{}
	}
	return Opt[Value]{Val: conv(*p), Ok: true}
}

func OptFlagMap[A, B any](o Opt[A], conv func(A) B) struct {
	Val B
	Ok  bool
} {
	if !o.Ok {
		return struct {
			Val B
			Ok  bool
		}{}
	}
	return struct {
		Val B
		Ok  bool
	}{Val: conv(o.Val), Ok: true}
}

func OptRefToFlag[T any](o OptRef, conv func(Value) T) struct {
	Val T
	Ok  bool
} {
	if o.val == nil {
		return struct {
			Val T
			Ok  bool
		}{}
	}
	return struct {
		Val T
		Ok  bool
	}{Val: conv(o.val), Ok: true}
}

func OptBoolToFlag(o OptBool) struct {
	Val bool
	Ok  bool
} {
	if o == OptBoolNone {
		return struct {
			Val bool
			Ok  bool
		}{}
	}
	return struct {
		Val bool
		Ok  bool
	}{Val: o == OptBoolTrue, Ok: true}
}

func FlagToOptMap[A, B any](v A, ok bool, conv func(A) B) Opt[B] {
	if !ok {
		return Opt[B]{}
	}
	return Opt[B]{Val: conv(v), Ok: true}
}

func StructToOptMap[A, B any](o struct {
	Val A
	Ok  bool
}, conv func(A) B) Opt[B] {
	if !o.Ok {
		return Opt[B]{}
	}
	return Opt[B]{Val: conv(o.Val), Ok: true}
}

func ErasedOptToPtr[T any](v Value, conv func(Value) T) *T {
	if IsNone(v) {
		return nil
	}
	x := conv(UnwrapSome(v))
	return &x
}

func PtrToErasedOpt[T any](p *T, conv func(T) Value) Value {
	if p == nil {
		return None
	}
	return Some(conv(*p))
}

func ErasedOptToFlag[T any](v Value, conv func(Value) T) struct {
	Val T
	Ok  bool
} {
	if IsNone(v) {
		return struct {
			Val T
			Ok  bool
		}{}
	}
	return struct {
		Val T
		Ok  bool
	}{Val: conv(UnwrapSome(v)), Ok: true}
}

func FlagToErasedOpt[T any](v T, ok bool, conv func(T) Value) Value {
	if !ok {
		return None
	}
	return Some(conv(v))
}

func StructToErasedOpt[T any](o struct {
	Val T
	Ok  bool
}, conv func(T) Value) Value {
	if !o.Ok {
		return None
	}
	return Some(conv(o.Val))
}

// IsNil reports whether v is untyped nil or a typed nil Go reference value.
func IsNil(v Value) bool {
	if v == nil {
		return true
	}
	rv := reflect.ValueOf(v)
	switch rv.Kind() {
	case reflect.Pointer, reflect.Interface, reflect.Slice, reflect.Map, reflect.Func, reflect.Chan:
		return rv.IsNil()
	default:
		return false
	}
}

// FromNullableGo converts untyped nil or a typed nil Go pointer/reference to None,
// and any other value to Some(v).
func FromNullableGo(v Value) Value {
	if IsNil(v) {
		return None
	}
	return Some(v)
}

// NullableToOptValue converts a nullable Go handle directly to Opt[Value].
func NullableToOptValue(v Value) Opt[Value] {
	if IsNil(v) {
		return Opt[Value]{}
	}
	return Opt[Value]{Val: v, Ok: true}
}

func failCaught(fb *Fiber, catchFn Value, recovered any) Value {
	var errVal Value
	switch r := recovered.(type) {
	case error:
		errVal = r.Error()
	case string:
		errVal = r
	default:
		errVal = fmt.Sprint(r)
	}
	return Fail(Apply(fb, catchFn, errVal))
}

// ForeignAsyncCatch runs a cancellable direct Go binding returning (Value, error)
// in a goroutine. Interruption cancels ctx immediately and tracks goroutine completion
// on fb.owner; errors and panics are mapped through catchFn on the calling fiber.
func ForeignAsyncCatch(fb *Fiber, call func(Context) (Value, error), catchFn Value) Value {
	ctx, cancel := context.WithCancel(context.Background())
	type result struct {
		value    Value
		err      error
		panicVal any
		cause    *Cause
	}
	ended := make(chan result, 1)
	done := make(chan struct{})
	go func() {
		defer close(done)
		var r result
		func() {
			defer func() {
				if rec := recover(); rec != nil {
					if f, ok := rec.(*Failure); ok {
						r.cause = f.Cause
					} else {
						r.panicVal = rec
					}
				}
			}()
			r.value, r.err = call(ctx)
		}()
		ended <- r
	}()
	signal := signalOf(fb)
	if c := parking(fb); c != nil {
		defer c.park(func() bool { return len(ended) > 0 || isClosed(signal) })()
	}
	select {
	case r := <-ended:
		cancel()
		if r.cause != nil {
			panic(&Failure{r.cause})
		}
		if r.panicVal != nil {
			return failCaught(fb, catchFn, r.panicVal)
		}
		if r.err != nil {
			return Fail(Apply(fb, catchFn, r.err.Error()))
		}
		return r.value
	case <-signal:
		cancel()
		fb.owner.track(done)
		panic(fb.state.stopped())
	}
}
