package main

import "flick.app/flickrt"

// twice is f (f x), with f as the host takes a callback of one argument.
func twice(_ *flickrt.Fiber, f, x flickrt.Value) flickrt.Value {
	g := f.(func(flickrt.Value) flickrt.Value)
	return g(g(x))
}

// combine is f a b, with f as the host takes a callback of two arguments.
func combine(_ *flickrt.Fiber, f, a, b flickrt.Value) flickrt.Value {
	return f.(func(flickrt.Value, flickrt.Value) flickrt.Value)(a, b)
}
