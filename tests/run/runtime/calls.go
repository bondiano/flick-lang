package main

import (
	"sync"
	"time"

	"flick.app/flickrt"
)

var calls sync.Map

// call is the latch of the call named `name`, opened when it is under way.
type call struct {
	once   sync.Once
	opened chan struct{}
}

func latch(name flickrt.Value) *call {
	c, _ := calls.LoadOrStore(name, &call{opened: make(chan struct{})})
	return c.(*call)
}

// settleLater settles some time after its call is cancelled, and says so.
func settleLater(ctx flickrt.Context, name flickrt.Value) flickrt.Value {
	c := latch(name)
	c.once.Do(func() { close(c.opened) })
	<-ctx.Done()
	time.Sleep(30 * time.Millisecond)
	return flickrt.StdoutLine(name.(string) + " settled")
}

// underWay ends when the call named `name` is under way.
func underWay(_ flickrt.Context, name flickrt.Value) flickrt.Value {
	<-latch(name).opened
	return flickrt.Unit
}

// heldWrite ignores interruption: under way at once, it ends only when
// releaseWrite lets it, and says so.
func heldWrite(_ flickrt.Context, name flickrt.Value) flickrt.Value {
	c := latch(name)
	c.once.Do(func() { close(c.opened) })
	<-latch(name.(string) + " released").opened
	return flickrt.StdoutLine(name.(string) + " written")
}

// releaseWrite lets the held write named `name` end.
func releaseWrite(_ *flickrt.Fiber, name flickrt.Value) flickrt.Value {
	c := latch(name.(string) + " released")
	c.once.Do(func() { close(c.opened) })
	return flickrt.Unit
}
