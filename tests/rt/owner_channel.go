package main

import "flick.app/flickrt"

// Nonretaining instrumentation, matching the JavaScript ambient-owner observer.
func sameOwner(fb *flickrt.Fiber, owner flickrt.Value) flickrt.Value {
	return flickrt.ScopeCurrent(fb, flickrt.Unit) == owner
}
