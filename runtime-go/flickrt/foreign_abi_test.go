package flickrt

import (
	"errors"
	"sync/atomic"
	"testing"
	"time"
)

func TestBytesCopyIsolation(t *testing.T) {
	orig := BytesEncodeUtf8("hello")
	buf := BytesToGo(orig)
	if string(buf) != "hello" {
		t.Fatalf("BytesToGo = %q, want %q", string(buf), "hello")
	}
	buf[0] = 'X'
	if got := string(BytesToGo(orig)); got != "hello" {
		t.Fatalf("mutating Go []byte corrupted Flick Bytes: got %q", got)
	}

	raw := []byte("world")
	fv := BytesFromGo(raw)
	raw[0] = 'Z'
	if got := string(BytesToGo(fv)); got != "world" {
		t.Fatalf("mutating source []byte corrupted Flick Bytes: got %q", got)
	}

	if got := len(BytesToGo(BytesEmpty)); got != 0 {
		t.Fatalf("empty Bytes len = %d, want 0", got)
	}
	if !Equals(BytesFromGo(nil), BytesEmpty) {
		t.Fatalf("BytesFromGo(nil) should equal BytesEmpty")
	}
}

func TestListCopyIsolation(t *testing.T) {
	orig := []Value{int32(1), int32(2), int32(3)}
	goSlice := ListToGo(orig, AsInt)
	if len(goSlice) != 3 || goSlice[0] != 1 || goSlice[1] != 2 || goSlice[2] != 3 {
		t.Fatalf("unexpected ListToGo result: %v", goSlice)
	}
	goSlice[1] = 99
	if orig[1].(int32) != 2 {
		t.Fatalf("mutating Go slice corrupted Flick List")
	}

	src := []string{"a", "b"}
	flickList := ListFromGo(src, FromString)
	src[0] = "changed"
	xs := List(flickList)
	if xs[0].(string) != "a" || xs[1].(string) != "b" {
		t.Fatalf("mutating source Go slice corrupted Flick List: %v", xs)
	}
}

func TestOptionPointerAndNestedFlagPreservation(t *testing.T) {
	// Scalar pointer round-trip distinguishes None from Some(0) and Some(42).
	if p := OptToPtr(NoneOpt[int32]()); p != nil {
		t.Fatalf("NoneOpt should become nil, got %v", *p)
	}
	zeroPtr := OptToPtr(SomeOpt(int32(0)))
	if zeroPtr == nil || *zeroPtr != 0 {
		t.Fatalf("SomeOpt(0) should become non-nil pointer to 0")
	}
	if got := PtrToOpt(zeroPtr); !got.Ok || got.Val != 0 {
		t.Fatalf("PtrToOpt(&0) = %+v", got)
	}
	if got := PtrToOpt((*int32)(nil)); got.Ok {
		t.Fatalf("PtrToOpt(nil) should be None")
	}

	// Bool sentinel pointer round-trip distinguishes None, Some(false), Some(true).
	if p := OptBoolToPtr(OptBoolNone); p != nil {
		t.Fatalf("OptBoolNone should become nil")
	}
	fp := OptBoolToPtr(OptBoolFalse)
	if fp == nil || *fp {
		t.Fatalf("OptBoolFalse should become &false")
	}
	if got := PtrToOptBool(fp); got != OptBoolFalse {
		t.Fatalf("PtrToOptBool(&false) = %v", got)
	}

	// Nested Option (Option Int): None, Some None, Some (Some 7) stay distinct!
	n0 := NoneOpt[Opt[int32]]()
	n1 := SomeOpt(NoneOpt[int32]())
	n2 := SomeOpt(SomeOpt(int32(7)))

	f0 := OptFlagMap(n0, OptToPtr[int32])
	f1 := OptFlagMap(n1, OptToPtr[int32])
	f2 := OptFlagMap(n2, OptToPtr[int32])
	if f0.Ok {
		t.Fatalf("None should have Ok=false")
	}
	if !f1.Ok || f1.Val != nil {
		t.Fatalf("Some None should have Ok=true, Val=nil, got %+v", f1)
	}
	if !f2.Ok || f2.Val == nil || *f2.Val != 7 {
		t.Fatalf("Some (Some 7) should have Ok=true, *Val=7, got %+v", f2)
	}

	r0 := FlagToOptMap(f0.Val, f0.Ok, PtrToOpt[int32])
	r1 := FlagToOptMap(f1.Val, f1.Ok, PtrToOpt[int32])
	r2 := FlagToOptMap(f2.Val, f2.Ok, PtrToOpt[int32])
	if r0.Ok || !r1.Ok || r1.Val.Ok || !r2.Ok || !r2.Val.Ok || r2.Val.Val != 7 {
		t.Fatalf("nested round-trip lost constructor nesting: %+v %+v %+v", r0, r1, r2)
	}

	// Typed nil pointer for nullable opaque handle.
	var typedNil *int
	if !IsNil(typedNil) {
		t.Fatalf("typed nil pointer must satisfy IsNil")
	}
	if !IsNone(FromNullableGo(typedNil)) {
		t.Fatalf("FromNullableGo(typedNil) must be None")
	}
	val := 5
	if got := FromNullableGo(&val); !IsSome(got) {
		t.Fatalf("FromNullableGo(&val) must be Some")
	}
	if got := NullableToOptRef(nil); got != NoneRef {
		t.Fatalf("NullableToOptRef(nil) must be NoneRef")
	}
	if got := NullableToOptRef(typedNil); got != NoneRef {
		t.Fatalf("NullableToOptRef(typedNil) must be NoneRef")
	}
	if got := NullableToOptRef(&val); got == NoneRef || got.val != &val {
		t.Fatalf("NullableToOptRef(&val) = %+v", got)
	}
	absent := struct {
		Val Value
		Ok  bool
	}{}
	present := struct {
		Val Value
		Ok  bool
	}{Val: &val, Ok: true}
	if got := StructToOptRef(absent, IdentityValue); got != NoneRef {
		t.Fatalf("StructToOptRef(absent) must be NoneRef")
	}
	if got := StructToOptRef(present, IdentityValue); got == NoneRef || got.val != &val {
		t.Fatalf("StructToOptRef(present) = %+v", got)
	}
}

func TestForeignAsyncCatchSuccessErrorAndCancellation(t *testing.T) {
	catchFn := Fn(func(_ *Fiber, msg Value) Value {
		return "mapped:" + msg.(string)
	})

	// 1. Success path.
	fb := rootFiber()
	got := ForeignAsyncCatch(fb, func(_ Context) (Value, error) {
		return "ok-value", nil
	}, catchFn)
	if got != "ok-value" {
		t.Fatalf("unexpected success value: %#v", got)
	}

	// 2. Error path mapped through catchFn.
	_, causeErr := catch(func() Value {
		return ForeignAsyncCatch(fb, func(_ Context) (Value, error) {
			return nil, errors.New("boom")
		}, catchFn)
	})
	if causeErr == nil || causeErr.kind != causeFail || causeErr.err != "mapped:boom" {
		t.Fatalf("expected mapped fail cause, got %#v", causeErr)
	}

	// 3. Cancellation path: fiber interrupted -> ctx.Done() fires and owner waits for goroutine.
	var observedCancel atomic.Bool
	var finished atomic.Bool
	var releasedAfter bool
	fb2 := rootFiber()
	owned(fb2, func(fb *Fiber, o *Owner) Value {
		o.add(fb, func(*Fiber, *Cause) Value {
			releasedAfter = finished.Load()
			return Unit
		})
		go func() {
			time.Sleep(10 * time.Millisecond)
			fb.state.interrupt(0)
		}()
		_, cause := catch(func() Value {
			return ForeignAsyncCatch(fb, func(ctx Context) (Value, error) {
				<-ctx.Done()
				observedCancel.Store(true)
				time.Sleep(15 * time.Millisecond)
				finished.Store(true)
				return nil, ctx.Err()
			}, catchFn)
		})
		if cause == nil || !cause.has(causeInterrupt) {
			t.Errorf("expected interruption cause, got %v", cause)
		}
		return Unit
	})
	if !observedCancel.Load() || !releasedAfter {
		t.Fatalf("expected cancellation observed (%v) and owner released after goroutine (%v)",
			observedCancel.Load(), releasedAfter)
	}
}
