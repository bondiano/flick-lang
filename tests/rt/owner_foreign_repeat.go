package main

import "flick.app/flickrt"

func makeRepeat(_ *flickrt.Fiber, owner flickrt.Value) flickrt.Value {
	var work flickrt.Eff
	work = func(fb *flickrt.Fiber) flickrt.Value {
		if flickrt.ScopeCurrent(fb, flickrt.Unit) != owner {
			panic("wrong recursive task owner")
		}
		return work
	}
	return work
}
