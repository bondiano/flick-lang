package main

import (
	"time"

	"flick.app/flickrt"
)

// open is a resource of the innermost scope, closed when the scope closes.
func open(fb *flickrt.Fiber, name flickrt.Value) flickrt.Value {
	acquire := flickrt.Eff(func(*flickrt.Fiber) flickrt.Value {
		flickrt.StdoutLine("open " + name.(string))
		return name
	})
	release := flickrt.Fn(func(fb *flickrt.Fiber, _ flickrt.Value) flickrt.Value {
		return flickrt.Eff(func(*flickrt.Fiber) flickrt.Value { return flickrt.StdoutLine("close " + name.(string)) })
	})
	return flickrt.AcquireRelease(fb, acquire, release)
}

// slowWrite ignores interruption and finishes after `millis`.
func slowWrite(_ flickrt.Context, name, millis flickrt.Value) flickrt.Value {
	time.Sleep(time.Duration(millis.(float64) * float64(time.Millisecond)))
	return flickrt.StdoutLine(name.(string) + " written")
}
