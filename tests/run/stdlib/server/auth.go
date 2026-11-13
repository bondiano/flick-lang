package main

import (
	"sync/atomic"

	"flick.app/flickrt"
)

// How many times the guarded function's body ran, for the auth run test.
var runCount atomic.Int64

func ran(_ *flickrt.Fiber) flickrt.Value {
	runCount.Add(1)
	return flickrt.Unit
}

func runs_(_ *flickrt.Fiber) flickrt.Value { return float64(runCount.Load()) }
