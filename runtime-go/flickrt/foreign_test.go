package flickrt

import (
	"errors"
	"sync/atomic"
	"testing"
	"time"
)

func TestOwnerWaitsForAnInterruptedForeignCall(t *testing.T) {
	var finished atomic.Bool
	var releasedAfter bool
	fb := rootFiber()
	owned(fb, func(fb *Fiber, o *Owner) Value {
		o.add(fb, func(*Fiber, *Cause) Value {
			releasedAfter = finished.Load()
			return Unit
		})
		go func() {
			time.Sleep(5 * time.Millisecond)
			fb.state.interrupt(0)
		}()
		_, cause := catch(func() Value {
			return ForeignAsync(fb, func(ctx Context) Value {
				<-ctx.Done()
				time.Sleep(20 * time.Millisecond) // a call slow to notice the cancellation
				finished.Store(true)
				return Unit
			})
		})
		if cause == nil || !cause.has(causeInterrupt) {
			t.Errorf("the call ends with the interruption, not %v", cause)
		}
		return Unit
	})
	if !releasedAfter {
		t.Fatal("the owner released its resources before the foreign call ended")
	}
}

func TestOptVal(t *testing.T) {
	// None / nil
	noneVal := ToOptVal(None)
	if noneVal.Has || noneVal.Val != nil {
		t.Fatalf("expected empty OptVal for None, got %+v", noneVal)
	}
	if got := noneVal.Or("default"); got != "default" {
		t.Fatalf("expected default, got %v", got)
	}
	if got := noneVal.OrNil(); got != nil {
		t.Fatalf("expected nil, got %v", got)
	}
	if _, ok := noneVal.Value(); ok {
		t.Fatal("expected ok=false")
	}

	nilVal := ToOptVal(nil)
	if nilVal.Has {
		t.Fatal("expected nil to produce empty OptVal")
	}

	// Some
	someVal := ToOptVal(Some(int32(42)))
	if !someVal.Has || someVal.Val != int32(42) {
		t.Fatalf("expected Has=true, Val=42, got %+v", someVal)
	}
	if got := someVal.Or(int32(0)); got != int32(42) {
		t.Fatalf("expected 42, got %v", got)
	}
	if v, ok := someVal.Value(); !ok || v != int32(42) {
		t.Fatalf("expected (42, true), got (%v, %v)", v, ok)
	}
}

func TestFromNullable(t *testing.T) {
	if got := FromNullable(nil); got != None {
		t.Fatalf("expected None for nil, got %v", got)
	}
	if got := FromNullable(None); got != None {
		t.Fatalf("expected None for None, got %v", got)
	}
	if got := FromNullable(OptVal{Has: false}); got != None {
		t.Fatalf("expected None for empty OptVal, got %v", got)
	}
	some := FromNullable("hello")
	if d, ok := some.(*Data); !ok || d.Tag != "Some" || d.in[0] != "hello" {
		t.Fatalf("expected Some(hello), got %v", some)
	}
}

func TestToUnit(t *testing.T) {
	if got := ToUnit(42); got != Unit {
		t.Fatalf("expected Unit, got %v", got)
	}
}

func TestCatchForeign(t *testing.T) {
	fb := rootFiber()
	mapErr := Fn(func(_ *Fiber, err Value) Value {
		return "mapped: " + err.(string)
	})

	// Success case
	res := CatchForeign(fb, func() (Value, error) {
		return int32(100), nil
	}, mapErr)
	if res != int32(100) {
		t.Fatalf("expected 100, got %v", res)
	}

	// Error return case
	_, cause := catch(func() Value {
		return CatchForeign(fb, func() (Value, error) {
			return nil, errors.New("boom")
		}, mapErr)
	})
	if cause == nil || cause.kind != causeFail {
		t.Fatalf("expected fail cause, got %v", cause)
	}
	if cause.err != "mapped: boom" {
		t.Fatalf("expected 'mapped: boom', got %v", cause.err)
	}

	// Panic error case
	_, causePanic := catch(func() Value {
		return CatchForeign(fb, func() (Value, error) {
			panic(errors.New("panicked"))
		}, mapErr)
	})
	if causePanic == nil || causePanic.kind != causeFail {
		t.Fatalf("expected fail cause from panic, got %v", causePanic)
	}
	if causePanic.err != "mapped: panicked" {
		t.Fatalf("expected 'mapped: panicked', got %v", causePanic.err)
	}
}

func TestCallbackWrappers(t *testing.T) {
	fb := rootFiber()

	// Curried 2-arg Flick function: \x -> \y -> x + y
	add := Fn(func(_ *Fiber, x Value) Value {
		return Fn(func(_ *Fiber, y Value) Value {
			return x.(int32) + y.(int32)
		})
	})

	cb2 := Callback2(fb, add)
	if got := cb2(int32(3), int32(4)); got != int32(7) {
		t.Fatalf("expected 7, got %v", got)
	}

	wrapped := WrapCallback(fb, add, 2).(func(Value, Value) Value)
	if got := wrapped(int32(10), int32(20)); got != int32(30) {
		t.Fatalf("expected 30, got %v", got)
	}
}

type testCounter struct {
	val int32
}

func (c *testCounter) Add(fb *Fiber, delta int32) int32 {
	c.val += delta
	return c.val
}

func (c *testCounter) SumAll(args []Value) int32 {
	var total int32
	for _, a := range args {
		total += a.(int32)
	}
	return total
}

func (c *testCounter) FailMethod(fb *Fiber, _ Value) (Value, error) {
	return nil, errors.New("method error")
}

func TestCallMethod(t *testing.T) {
	fb := rootFiber()
	counter := &testCounter{val: 5}

	// Call Add
	res := CallMethod(fb, counter, "Add", []Value{int32(3)})
	if res != int32(8) || counter.val != int32(8) {
		t.Fatalf("expected 8, got res=%v val=%v", res, counter.val)
	}

	// Call SumAll
	resSum := CallMethod(fb, counter, "SumAll", []Value{int32(1), int32(2), int32(3)})
	if resSum != int32(6) {
		t.Fatalf("expected 6, got %v", resSum)
	}
}

func TestCallMethodCatchForeign(t *testing.T) {
	fb := rootFiber()
	counter := &testCounter{val: 5}
	mapErr := Fn(func(_ *Fiber, err Value) Value {
		return "mapped: " + err.(string)
	})

	// CallMethod returning (nil, err) wrapped in CatchForeign produces Fail(Apply(fb, catchFn, err.Error()))
	_, cause := catch(func() Value {
		return CatchForeign(fb, func() (Value, error) {
			return CallMethod(fb, counter, "FailMethod", []Value{Unit}), nil
		}, mapErr)
	})

	if cause == nil || cause.kind != causeFail {
		t.Fatalf("expected fail cause, got %v", cause)
	}
	if cause.err != "mapped: method error" {
		t.Fatalf("expected 'mapped: method error', got %v", cause.err)
	}

	// Unwrapped CallMethod error panics with error, which unwinds to fiber defect (causeDie)
	_, unwrappedCause := catch(func() Value {
		return CallMethod(fb, counter, "FailMethod", []Value{Unit})
	})
	if unwrappedCause == nil || unwrappedCause.kind != causeDie {
		t.Fatalf("expected die cause for unwrapped CallMethod error, got %v", unwrappedCause)
	}
}

func TestApplyGoFunctions(t *testing.T) {
	fb := rootFiber()

	// 1-arg Go function
	f1 := func(x Value) Value { return x.(int32) * 2 }
	if got := Apply(fb, f1, int32(21)); got != int32(42) {
		t.Fatalf("expected 42, got %v", got)
	}

	// 2-arg Go function
	f2 := func(x, y Value) Value { return x.(int32) + y.(int32) }
	step := Apply(fb, f2, int32(10))
	if got := Apply(fb, step, int32(5)); got != int32(15) {
		t.Fatalf("expected 15, got %v", got)
	}
}
