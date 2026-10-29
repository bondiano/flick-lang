package flickrt

import (
	"math"
	"math/big"
)

// ByteSize as Effect's: a non-negative whole number of bytes, a BigInt.

var ByteSizeZero Value = big.NewInt(0)

// ByteSizeBytes is `n` bytes, when that is a whole, non-negative, safe
// number.
func ByteSizeBytes(n Value) Value {
	x := n.(float64)
	if x < 0 || x != math.Trunc(x) || x > 1<<53-1 {
		return None
	}
	return Some(big.NewInt(int64(x)))
}
