package main

import "flick.app/flickrt"

// count is the default of `quota`: it counts its calls in the cell it is given.
func count(_ *flickrt.Fiber, cell flickrt.Value) flickrt.Value {
	n := flickrt.RefGet(cell).(float64) + 1
	flickrt.RefSet(cell, n)
	return 100 + n
}
