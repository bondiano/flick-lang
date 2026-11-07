package main

import (
	"sync"
	"time"

	"flick.app/flickrt"
)

var calls sync.Map

// latch is the channel of the call named `name`, closed when it is under way.
func latch(name flickrt.Value) chan struct{} {
	opened, _ := calls.LoadOrStore(name, make(chan struct{}))
	return opened.(chan struct{})
}

// settleLater settles some time after its call is cancelled, and says so.
func settleLater(ctx flickrt.Context, name flickrt.Value) flickrt.Value {
	close(latch(name))
	<-ctx.Done()
	time.Sleep(30 * time.Millisecond)
	return flickrt.StdoutLine(name.(string) + " settled")
}

// underWay ends when the call named `name` is under way.
func underWay(_ flickrt.Context, name flickrt.Value) flickrt.Value {
	<-latch(name)
	return flickrt.Unit
}
