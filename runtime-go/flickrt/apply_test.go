package flickrt

import "testing"

func TestAFunctionIsTheSameHoweverItIsGivenItsArguments(t *testing.T) {
	join := func(parts ...Value) Value {
		text := ""
		for _, part := range parts {
			text += part.(string)
		}
		return text
	}
	stage := func(f func(Value) Value) Value { return Fn(func(_ *Fiber, a Value) Value { return f(a) }) }
	two := map[string]Value{
		"a function per argument": stage(func(a Value) Value { return stage(func(b Value) Value { return join(a, b) }) }),
		"both at once":            Fn2(func(_ *Fiber, a, b Value) Value { return join(a, b) }),
		"a Go function":           Curry2(func(_ *Fiber, a, b Value) Value { return join(a, b) }),
	}
	for name, f := range two {
		for how, got := range map[string]Value{
			"Apply2":      Apply2(nil, f, "a", "b"),
			"Apply twice": Apply(nil, Apply(nil, f, "a"), "b"),
		} {
			if got != "ab" {
				t.Errorf("%s by %s: %v", name, how, got)
			}
		}
	}
	three := map[string]Value{
		"a function per argument": stage(func(a Value) Value {
			return stage(func(b Value) Value { return stage(func(c Value) Value { return join(a, b, c) }) })
		}),
		"all at once": Fn3(func(_ *Fiber, a, b, c Value) Value { return join(a, b, c) }),
		"one, then two at once": stage(func(a Value) Value {
			return Fn2(func(_ *Fiber, b, c Value) Value { return join(a, b, c) })
		}),
		"two at once, then one": Fn2(func(_ *Fiber, a, b Value) Value {
			return stage(func(c Value) Value { return join(a, b, c) })
		}),
		"a Go function": Curry3(func(_ *Fiber, a, b, c Value) Value { return join(a, b, c) }),
	}
	for name, f := range three {
		for how, got := range map[string]Value{
			"Apply3":             Apply3(nil, f, "a", "b", "c"),
			"Apply2, then Apply": Apply(nil, Apply2(nil, f, "a", "b"), "c"),
			"Apply, then Apply2": Apply2(nil, Apply(nil, f, "a"), "b", "c"),
			"Apply three times":  Apply(nil, Apply(nil, Apply(nil, f, "a"), "b"), "c"),
		} {
			if got != "abc" {
				t.Errorf("%s by %s: %v", name, how, got)
			}
		}
		// a function given its first argument is a function again, for any rest
		part := Apply(nil, f, "a")
		if x, y := Apply2(nil, part, "b", "c"), Apply2(nil, part, "x", "y"); x != "abc" || y != "axy" {
			t.Errorf("%s given its first argument: %v, %v", name, x, y)
		}
	}
}

func TestAValueThatIsNoFunctionFailsWhenApplied(t *testing.T) {
	for name, apply := range map[string]func(){
		"Apply":  func() { Apply(nil, "text", "a") },
		"Apply2": func() { Apply2(nil, int32(1), "a", "b") },
		"Apply3": func() { Apply3(nil, Unit, "a", "b", "c") },
	} {
		func() {
			defer func() {
				if recover() == nil {
					t.Errorf("%s of what is no function did not fail", name)
				}
			}()
			apply()
		}()
	}
}
