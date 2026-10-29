package main

import (
	"time"

	"flick.app/flickrt"
)

// sleep ends after `millis`; like the JS promise it ignores interruption.
func sleep(_ flickrt.Context, millis flickrt.Value) flickrt.Value {
	time.Sleep(time.Duration(millis.(float64) * float64(time.Millisecond)))
	return flickrt.Unit
}

// hang ends only when its call is cancelled.
func hang(ctx flickrt.Context, _ flickrt.Value) flickrt.Value {
	<-ctx.Done()
	flickrt.StdoutLine("hang cancelled")
	return flickrt.Unit
}
