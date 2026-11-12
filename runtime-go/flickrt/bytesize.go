package flickrt

import (
	"math"
	"math/big"
)

// The JSON codecs of `ByteSize`, a non-negative whole number of bytes held as
// a BigInt; the rest of the module is written in Flick.

// ByteSizeBytes is `n` bytes, when that is a whole, non-negative, safe
// number.
func ByteSizeBytes(n Value) Value {
	x := n.(float64)
	if x < 0 || x != math.Trunc(x) || x > 1<<53-1 {
		return None
	}
	return Some(big.NewInt(int64(x)))
}

var (
	// CodecByteSize is `ByteSize.json`: the text of a size, written
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
