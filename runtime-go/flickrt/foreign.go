package flickrt

import (
	"context"
	"fmt"
	"reflect"
)

// Context is what a Go function behind a `foreign async` binding takes
// first: it is cancelled when the calling fiber is interrupted.
type Context = context.Context

// ForeignAsync runs the Go side of a `foreign async` binding in a goroutine
// of its own. An interruption cancels its context and fails at once; the
// call may still be running, so the innermost owner waits for it to end
// before releasing resources, as the JS runtime waits for a promise.
func ForeignAsync(fb *Fiber, call func(Context) Value) Value {
	ctx, cancel := context.WithCancel(context.Background())
	type result struct {
		value Value
		cause *Cause
	}
	ended := make(chan result, 1)
	done := make(chan struct{})
	go func() {
		defer close(done)
		v, c := catch(func() Value { return call(ctx) })
		ended <- result{v, c}
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
		return r.value
	case <-signal:
		cancel()
		fb.owner.track(done)
		panic(fb.state.stopped())
	}
}

// OptVal represents an optional foreign argument that may be set or omitted.
type OptVal struct {
	Has bool
	Val Value
}

func (o OptVal) Value() (Value, bool) {
	return o.Val, o.Has
}

func (o OptVal) Or(def Value) Value {
	if o.Has {
		return o.Val
	}
	return def
}

func (o OptVal) OrNil() Value {
	if o.Has {
		return o.Val
	}
	return nil
}

// ToOptVal converts a Flick Option (Some/None) or raw value to an OptVal.
func ToOptVal(v Value) OptVal {
	if v == nil || v == None {
		return OptVal{Has: false, Val: nil}
	}
	if opt, ok := v.(OptVal); ok {
		return opt
	}
	if d, ok := v.(*Data); ok {
		if d.Tag == "Some" {
			return OptVal{Has: true, Val: d.in[0]}
		}
		if d.Tag == "None" {
			return OptVal{Has: false, Val: nil}
		}
	}
	return OptVal{Has: true, Val: v}
}

// FromNullable converts nil or an empty OptVal/None to None, and other values to Some(v).
func FromNullable(v Value) Value {
	if v == nil || v == None {
		return None
	}
	if opt, ok := v.(OptVal); ok {
		if !opt.Has || opt.Val == nil {
			return None
		}
		return Some(opt.Val)
	}
	if d, ok := v.(*Data); ok && d.Tag == "None" {
		return None
	}
	return Some(v)
}

// ToUnit normalizes any foreign result to Flick Unit ().
func ToUnit(_ Value) Value {
	return Unit
}

// CatchForeign runs a foreign function returning (Value, error) or panicking with an error.
// If an error occurs, it maps the error through catchFn and raises a typed failure on the fiber.
func CatchForeign(fb *Fiber, call func() (Value, error), catchFn Value) (res Value) {
	defer func() {
		if r := recover(); r != nil {
			if f, ok := r.(*Failure); ok {
				panic(f)
			}
			var errVal Value
			if err, ok := r.(error); ok {
				errVal = err.Error()
			} else if s, ok := r.(string); ok {
				errVal = s
			} else {
				errVal = fmt.Sprint(r)
			}
			mapped := Apply(fb, catchFn, errVal)
			res = Fail(mapped)
		}
	}()
	val, err := call()
	if err != nil {
		var errVal Value = err.Error()
		mapped := Apply(fb, catchFn, errVal)
		return Fail(mapped)
	}
	return val
}

// Callback1 wraps a curried Flick function as a 1-argument Go function.
func Callback1(fb *Fiber, fn Value) func(Value) Value {
	return func(x Value) Value {
		return Apply(fb, fn, x)
	}
}

// Callback2 wraps a curried Flick function as a 2-argument Go function.
func Callback2(fb *Fiber, fn Value) func(Value, Value) Value {
	return func(x, y Value) Value {
		return Apply2(fb, fn, x, y)
	}
}

// Callback3 wraps a curried Flick function as a 3-argument Go function.
func Callback3(fb *Fiber, fn Value) func(Value, Value, Value) Value {
	return func(x, y, z Value) Value {
		return Apply3(fb, fn, x, y, z)
	}
}

// WrapCallback wraps a curried Flick function with arity n into a Go function.
func WrapCallback(fb *Fiber, fn Value, arity int) Value {
	switch arity {
	case 1:
		return Callback1(fb, fn)
	case 2:
		return Callback2(fb, fn)
	case 3:
		return Callback3(fb, fn)
	default:
		return func(args ...Value) Value {
			cur := fn
			for _, arg := range args {
				cur = Apply(fb, cur, arg)
			}
			return cur
		}
	}
}

func convertArg(arg Value, targetType reflect.Type) reflect.Value {
	if arg == nil {
		return reflect.Zero(targetType)
	}
	v := reflect.ValueOf(arg)
	if v.Type().AssignableTo(targetType) {
		return v
	}
	if v.Type().ConvertibleTo(targetType) {
		return v.Convert(targetType)
	}
	return v
}

// CallMethod invokes a method on a receiver struct or pointer by name,
// passing fb if the method accepts *Fiber as its first parameter, followed by args.
func CallMethod(fb *Fiber, receiver Value, method string, args []Value) Value {
	if receiver == nil {
		panic(fmt.Sprintf("cannot call method %s on nil receiver", method))
	}
	v := reflect.ValueOf(receiver)
	m := v.MethodByName(method)
	if !m.IsValid() && v.CanAddr() {
		m = v.Addr().MethodByName(method)
	}
	if !m.IsValid() && v.Kind() == reflect.Pointer && !v.IsNil() {
		m = v.Elem().MethodByName(method)
	}
	if !m.IsValid() {
		panic(fmt.Sprintf("method %s not found on %T", method, receiver))
	}
	mType := m.Type()
	numIn := mType.NumIn()
	callArgs := make([]reflect.Value, 0, numIn)
	argOffset := 0
	if numIn > 0 && mType.In(0) == reflect.TypeOf((*Fiber)(nil)) {
		callArgs = append(callArgs, reflect.ValueOf(fb))
		argOffset = 1
	}
	if mType.IsVariadic() {
		fixedIn := numIn - 1
		for i := argOffset; i < fixedIn && i-argOffset < len(args); i++ {
			callArgs = append(callArgs, convertArg(args[i-argOffset], mType.In(i)))
		}
		varType := mType.In(fixedIn).Elem()
		for i := fixedIn - argOffset; i < len(args); i++ {
			callArgs = append(callArgs, convertArg(args[i], varType))
		}
	} else if numIn == argOffset+1 && mType.In(argOffset) == reflect.TypeOf((*[]Value)(nil)).Elem() {
		callArgs = append(callArgs, reflect.ValueOf(args))
	} else {
		for i := 0; i < len(args) && len(callArgs) < numIn; i++ {
			targetType := mType.In(len(callArgs))
			callArgs = append(callArgs, convertArg(args[i], targetType))
		}
	}
	results := m.Call(callArgs)
	if len(results) == 0 {
		return Unit
	}
	if len(results) == 1 {
		return results[0].Interface()
	}
	if len(results) == 2 {
		var err error
		if !results[1].IsNil() {
			err = results[1].Interface().(error)
		}
		if err != nil {
			panic(err)
		}
		return results[0].Interface()
	}
	return results[0].Interface()
}
