package flickrt

import (
	"math"
	"math/big"
	"strings"
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

type byteUnit struct {
	symbol string
	factor *big.Int
}

// Effect's units, the decimal then the binary ones, each from the byte up.
var decimalUnits, binaryUnits = func() ([]byteUnit, []byteUnit) {
	pow := func(base, n int64) *big.Int { return new(big.Int).Exp(big.NewInt(base), big.NewInt(n), nil) }
	decimal := []byteUnit{{"B", big.NewInt(1)}}
	for i, s := range []string{"kB", "MB", "GB", "TB", "PB", "EB", "ZB", "YB", "RB", "QB"} {
		decimal = append(decimal, byteUnit{s, pow(1000, int64(i+1))})
	}
	binary := []byteUnit{decimal[0]}
	for i, s := range []string{"KiB", "MiB", "GiB", "TiB", "PiB", "EiB", "ZiB", "YiB"} {
		binary = append(binary, byteUnit{s, pow(1024, int64(i+1))})
	}
	return decimal, binary
}()

// roundedIn is `n` in `unit`, scaled by `scale`, rounded half up.
func roundedIn(n *big.Int, unit byteUnit, scale *big.Int) *big.Int {
	num := new(big.Int).Mul(n, scale)
	num.Mul(num, big.NewInt(2)).Add(num, unit.factor)
	return num.Quo(num, new(big.Int).Mul(unit.factor, big.NewInt(2)))
}

func formatWithUnit(n *big.Int, unit byteUnit, precision int, trailingZeros bool) string {
	scale := new(big.Int).Exp(big.NewInt(10), big.NewInt(int64(precision)), nil)
	whole, rest := new(big.Int).QuoRem(roundedIn(n, unit, scale), scale, new(big.Int))
	if precision == 0 {
		return whole.String() + " " + unit.symbol
	}
	fraction := strings.Repeat("0", precision-len(rest.String())) + rest.String()
	if !trailingZeros {
		fraction = strings.TrimRight(fraction, "0")
	}
	if fraction != "" {
		fraction = "." + fraction
	}
	return whole.String() + fraction + " " + unit.symbol
}

// ByteSizeFormat is Effect's `ByteSize.format`, the precision cut to an
// integer from 0 to 20.
func ByteSizeFormat(options, self Value) Value {
	o, n := record(options), self.(*big.Int)
	precision := 2
	if p, ok := some(o.Get("precision")); ok {
		precision = min(20, max(0, int(p.(int32))))
	}
	trailingZeros := false
	if t, ok := some(o.Get("trailingZeros")); ok {
		trailingZeros = t.(bool)
	}
	if u, ok := some(o.Get("unit")); ok {
		return formatWithUnit(n, unitOf(u), precision, trailingZeros)
	}
	units, base := decimalUnits, int64(1000)
	if o.Get("binary").(bool) {
		units, base = binaryUnits, 1024
	}
	if n.Sign() == 0 {
		return "0 B"
	}
	i := len(units) - 1
	for i > 0 && n.Cmp(units[i].factor) < 0 {
		i--
	}
	if i < len(units)-1 {
		scale := new(big.Int).Exp(big.NewInt(10), big.NewInt(int64(precision)), nil)
		if roundedIn(n, units[i], scale).Cmp(new(big.Int).Mul(big.NewInt(base), scale)) >= 0 {
			i++
		}
	}
	return formatWithUnit(n, units[i], precision, trailingZeros)
}

var (
	// CodecByteSize is `ByteSize.json`: Effect's text of a size, written
	// back as a count of bytes.
	CodecByteSize Value = &fallible{&stage{
		&stringAs{"a string that will be decoded as a ByteSize"},
		func(_ *Fiber, v Value) (Value, []issue) {
			n, reason := decodeByteSize(v.(string))
			if reason != "" {
				return nil, fail("an invalid value")
			}
			return n, nil
		},
		pure(func(v Value) Value {
			if n := v.(*big.Int); n.Cmp(big.NewInt(1)) != 0 {
				return n.String() + " bytes"
			}
			return "1 byte"
		}),
	}}

	// CodecByteSizeNumber is `ByteSize.json_number`: a safe whole number.
	CodecByteSizeNumber Value = &fallible{&stage{
		CodecNumberFinite.(codec),
		func(_ *Fiber, v Value) (Value, []issue) {
			if n, ok := some(ByteSizeBytes(v)); ok {
				return n, nil
			}
			return nil, fail("an invalid value")
		},
		func(_ *Fiber, v Value) (Value, []issue) {
			if n := v.(*big.Int); n.IsInt64() && n.Int64() <= 1<<53-1 {
				return float64(n.Int64()), nil
			}
			return nil, fail("an invalid value")
		},
	}}

	// CodecByteSizeBigint is `ByteSize.json_bigint`: a string of digits.
	CodecByteSizeBigint Value = &fallible{&stage{
		withChecks(CodecString.(codec), isPattern(`^[0-9]+$`)),
		pure(func(v Value) Value { n, _ := new(big.Int).SetString(v.(string), 10); return n }),
		pure(func(v Value) Value { return v.(*big.Int).String() }),
	}}
)
