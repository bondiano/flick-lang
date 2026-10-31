package flickrt

import (
	"math"
	"math/big"
)

// Effect's `ByteSize` constructors, conversions, comparisons and arithmetic.
// A constructor gives `None` where Effect's throws, as the JS adapter's.

// unitOf is the unit of a `ByteSize.ByteUnit`: its constructor name, `KB`
// as `kB`.
func unitOf(u Value) byteUnit {
	name := ConstructorName(u.(*Data).Tag)
	if name == "KB" {
		name = "kB"
	}
	for _, unit := range append(append([]byteUnit{}, decimalUnits...), binaryUnits[1:]...) {
		if unit.symbol == name {
			return unit
		}
	}
	return decimalUnits[0]
}

// byteSizeIn is Effect's constructor of a unit: `None` unless the bytes
// are a whole, non-negative, safe number.
func byteSizeIn(units []byteUnit, symbol string) func(Value) Value {
	for _, u := range units {
		if u.symbol == symbol {
			factor := bigToFloat(u.factor)
			return func(q Value) Value { return ByteSizeBytes(q.(float64) * factor) }
		}
	}
	panic("no byte unit " + symbol)
}

var (
	ByteSizeKilobytes   = byteSizeIn(decimalUnits, "kB")
	ByteSizeMegabytes   = byteSizeIn(decimalUnits, "MB")
	ByteSizeGigabytes   = byteSizeIn(decimalUnits, "GB")
	ByteSizeTerabytes   = byteSizeIn(decimalUnits, "TB")
	ByteSizePetabytes   = byteSizeIn(decimalUnits, "PB")
	ByteSizeExabytes    = byteSizeIn(decimalUnits, "EB")
	ByteSizeZettabytes  = byteSizeIn(decimalUnits, "ZB")
	ByteSizeYottabytes  = byteSizeIn(decimalUnits, "YB")
	ByteSizeRonnabytes  = byteSizeIn(decimalUnits, "RB")
	ByteSizeQuettabytes = byteSizeIn(decimalUnits, "QB")
	ByteSizeKibibytes   = byteSizeIn(binaryUnits, "KiB")
	ByteSizeMebibytes   = byteSizeIn(binaryUnits, "MiB")
	ByteSizeGibibytes   = byteSizeIn(binaryUnits, "GiB")
	ByteSizeTebibytes   = byteSizeIn(binaryUnits, "TiB")
	ByteSizePebibytes   = byteSizeIn(binaryUnits, "PiB")
	ByteSizeExbibytes   = byteSizeIn(binaryUnits, "EiB")
	ByteSizeZebibytes   = byteSizeIn(binaryUnits, "ZiB")
	ByteSizeYobibytes   = byteSizeIn(binaryUnits, "YiB")
)

func bsize(v Value) *big.Int { return v.(*big.Int) }

func ByteSizeFromString(text Value) Value {
	if n, reason := decodeByteSize(text.(string)); reason == "" {
		return Some(n)
	}
	return None
}

func ByteSizeIsZero(self Value) Value   { return bsize(self).Sign() == 0 }
func ByteSizeToBigInt(self Value) Value { return self }

func ByteSizeToNumber(self Value) Value {
	if n := bsize(self); n.IsInt64() && n.Int64() <= 1<<53-1 {
		return Some(float64(n.Int64()))
	}
	return None
}

func ByteSizeToUnit(unit, self Value) Value {
	return bigToFloat(bsize(self)) / bigToFloat(unitOf(unit).factor)
}

// The comparisons and arithmetic take `that` first and `self` last, as
// Flick passes them.

func ByteSizeEquals(that, self Value) Value                 { return bigOrder(self, that) == 0 }
func ByteSizeIsLessThan(that, self Value) Value             { return bigOrder(self, that) < 0 }
func ByteSizeIsLessThanOrEqualTo(that, self Value) Value    { return bigOrder(self, that) <= 0 }
func ByteSizeIsGreaterThan(that, self Value) Value          { return bigOrder(self, that) > 0 }
func ByteSizeIsGreaterThanOrEqualTo(that, self Value) Value { return bigOrder(self, that) >= 0 }

func ByteSizeMin(that, self Value) Value {
	if bigOrder(self, that) < 1 {
		return self
	}
	return that
}

func ByteSizeMax(that, self Value) Value {
	if bigOrder(self, that) > -1 {
		return self
	}
	return that
}

func ByteSizeBetween(options, self Value) Value {
	o := record(options)
	return bigOrder(self, o.Get("minimum")) != -1 && bigOrder(self, o.Get("maximum")) != 1
}

func ByteSizeClamp(options, self Value) Value {
	o := record(options)
	return ByteSizeMin(o.Get("maximum"), ByteSizeMax(o.Get("minimum"), self))
}

func ByteSizeSum(that, self Value) Value { return new(big.Int).Add(bsize(self), bsize(that)) }

func ByteSizeSubtract(that, self Value) Value {
	if bigOrder(self, that) < 0 {
		return None
	}
	return Some(new(big.Int).Sub(bsize(self), bsize(that)))
}

// byteScalar is Effect's `scalar`: a safe integer of at least `least`.
func byteScalar(x float64, least float64) (*big.Int, bool) {
	if x != math.Trunc(x) || math.Abs(x) > 1<<53-1 || x < least {
		return nil, false
	}
	return big.NewInt(int64(x)), true
}

func ByteSizeTimes(multiplier, self Value) Value {
	if m, ok := byteScalar(multiplier.(float64), 0); ok {
		return Some(m.Mul(bsize(self), m))
	}
	return None
}

func ByteSizeDivide(divisor, self Value) Value {
	if d, ok := byteScalar(divisor.(float64), 1); ok {
		return Some(d.Quo(bsize(self), d))
	}
	return None
}
