package main

import (
	"strings"
	"sync"
	"time"

	"flick.app/flickrt"
)

// Marks the handlers of the local run test leave once they finish, and
// the handlers that have started, which a client waits for before it gives up.
var (
	mu    sync.Mutex
	marks []string
	begun = map[string]chan struct{}{}
)

func mark(_ *flickrt.Fiber, name flickrt.Value) flickrt.Value {
	mu.Lock()
	defer mu.Unlock()
	marks = append(marks, name.(string))
	return flickrt.Unit
}

func marked(_ *flickrt.Fiber) flickrt.Value {
	mu.Lock()
	defer mu.Unlock()
	return strings.Join(marks, ", ")
}

// sleep ends after `millis`; like the JS promise it ignores interruption.
func sleep(_ flickrt.Context, millis flickrt.Value) flickrt.Value {
	time.Sleep(time.Duration(millis.(float64) * float64(time.Millisecond)))
	return flickrt.Unit
}

func signal(name string) chan struct{} {
	mu.Lock()
	defer mu.Unlock()
	if begun[name] == nil {
		begun[name] = make(chan struct{})
	}
	return begun[name]
}

func start(_ *flickrt.Fiber, name flickrt.Value) flickrt.Value {
	ch := signal(name.(string))
	select {
	case <-ch:
	default:
		close(ch)
	}
	return flickrt.Unit
}

func started(ctx flickrt.Context, name flickrt.Value) flickrt.Value {
	select {
	case <-signal(name.(string)):
	case <-ctx.Done():
	}
	return flickrt.Unit
}
