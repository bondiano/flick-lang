package flickrt

import (
	"math"
	"math/big"
	"strconv"
)

// Http.Etag's generators as Effect's: a file's tag is its size and its
// modification time in milliseconds, both in hex.

func etagLayer(key Value, ctor string) Value {
	tag := Tag("Http.Etag.Etag." + ctor)
	return LayerSucceed(key, Rec("from_file_info", Fn(func(fb *Fiber, info Value) Value {
		return Eff(func(*Fiber) Value { return NewData(tag, etagOf(record(info))) })
	})))
}

func EtagLayer(key Value) Value     { return etagLayer(key, "Strong") }
func EtagLayerWeak(key Value) Value { return etagLayer(key, "Weak") }

func etagOf(info *Record) string {
	mtime := "0"
	if m := info.Get("mtime"); isSome(m) {
		// a Date keeps whole milliseconds
		ms := math.Trunc(value(m).(float64))
		if math.IsNaN(ms) || math.IsInf(ms, 0) || math.Abs(ms) > 8.64e15 {
			mtime = "NaN"
		} else {
			mtime = strconv.FormatInt(int64(ms), 16)
		}
	}
	return info.Get("size").(*big.Int).Text(16) + "-" + mtime
}
