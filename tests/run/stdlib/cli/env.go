package main

import (
	"os"

	"flick.app/flickrt"
)

// set sets a variable of the process's environment for the trace run test.
func set(_ *flickrt.Fiber, name, value flickrt.Value) flickrt.Value {
	os.Setenv(name.(string), value.(string))
	return flickrt.Unit
}
