package flickrt

import (
	"math"
	"testing"
)

// The order of numbers puts NaN below every number, so `Float.min` picks
// NaN and `Float.max` the other number, whichever side NaN is on.
func TestNumberOrderPutsNaNLeast(t *testing.T) {
	nan := math.NaN()
	for _, args := range [][2]float64{{nan, 1}, {1, nan}} {
		if got := NumberMin(args[0], args[1]).(float64); !math.IsNaN(got) {
			t.Errorf("min %v %v = %v, want NaN", args[0], args[1], got)
		}
		if got := NumberMax(args[0], args[1]).(float64); got != 1 {
			t.Errorf("max %v %v = %v, want 1", args[0], args[1], got)
		}
	}
	if NumberLess(1.0, nan) != true || NumberGreater(nan, 1.0) != true {
		t.Error("NaN is less than 1")
	}
	if NumberLess(nan, nan) != false || numberOrder(0.0, math.Copysign(0, -1)) != 0 {
		t.Error("NaN equals NaN and -0 equals 0")
	}
}
