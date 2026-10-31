package flickrt

import (
	"crypto/rand"
	"crypto/sha1"
	"crypto/sha256"
	"crypto/sha512"
	"fmt"
	"math"
	"time"
)

// Effect's `Crypto` made over the platform's randomness and digests, as the
// JS adapter's over Web Crypto; a `PlatformError` becomes a `CryptoError`.

var cryptoErrorTag = Tag("Crypto.CryptoError.CryptoError")

func cryptoBytes(size int) []byte {
	out := make([]byte, size)
	if _, err := rand.Read(out); err != nil {
		panic(Defect{"crypto: " + err.Error()})
	}
	return out
}

func CryptoRandomBytes(size Value) Value {
	n := size.(int32)
	if n < 0 {
		return Fail(NewData(cryptoErrorTag, "Crypto.randomBytes: size must be a non-negative safe integer"))
	}
	return byteList(cryptoBytes(int(n)))
}

func CryptoDigest(algorithm, data Value) Value {
	in := []byte(byteString(data))
	var sum []byte
	switch ConstructorName(algorithm.(*Data).Tag) {
	case "Sha1":
		s := sha1.Sum(in)
		sum = s[:]
	case "Sha384":
		s := sha512.Sum384(in)
		sum = s[:]
	case "Sha512":
		s := sha512.Sum512(in)
		sum = s[:]
	default:
		s := sha256.Sum256(in)
		sum = s[:]
	}
	return byteList(sum)
}

// cryptoDouble is Effect's `nextDoubleUnsafe`: 53 random bits over 2^53.
func cryptoDouble() float64 {
	b := cryptoBytes(7)
	n := uint64(b[0]&0x1f)<<48 | uint64(b[1])<<40 | uint64(b[2])<<32 |
		uint64(b[3])<<24 | uint64(b[4])<<16 | uint64(b[5])<<8 | uint64(b[6])
	return float64(n) / (1 << 53)
}

func cryptoIntBetween(min, max float64) Value {
	lo, hi := math.Ceil(min), math.Floor(max)
	return int32(math.Floor(cryptoDouble()*(hi-lo+1)) + lo)
}

func CryptoRandom() Value        { return cryptoDouble() }
func CryptoRandomBoolean() Value { return cryptoDouble() > 0.5 }

// CryptoRandomInt draws over Flick's 32-bit `Int`, as the JS adapter's.
func CryptoRandomInt() Value { return cryptoIntBetween(math.MinInt32, math.MaxInt32) }

func CryptoRandomIntBetween(min, max Value) Value {
	return cryptoIntBetween(float64(min.(int32)), float64(max.(int32)))
}

// CryptoRandomBetween is Effect's `nextBetween`: a draw that rounds up to
// the excluded `max` gives its predecessor.
func CryptoRandomBetween(min, max Value) Value {
	lo, hi := min.(float64), max.(float64)
	v := cryptoDouble()*(hi-lo) + lo
	if v != hi || lo >= hi || math.IsInf(hi, 0) || math.IsNaN(hi) {
		return v
	}
	return math.Nextafter(hi, math.Inf(-1))
}

func CryptoRandomShuffle(xs Value) Value {
	buffer := append([]Value{}, List(xs)...)
	for i := len(buffer) - 1; i >= 1; i-- {
		index := min(i, int(math.Floor(cryptoDouble()*float64(i+1))))
		buffer[i], buffer[index] = buffer[index], buffer[i]
	}
	return buffer
}

func uuidString(b []byte) string {
	return fmt.Sprintf("%x-%x-%x-%x-%x", b[0:4], b[4:6], b[6:8], b[8:10], b[10:16])
}

func CryptoRandomUuidV4() Value {
	b := cryptoBytes(16)
	b[6] = b[6]&0x0f | 0x40
	b[8] = b[8]&0x3f | 0x80
	return uuidString(b)
}

// cryptoMillis is the time of UUID v7 and ULID, clamped to 48 bits.
// shortcut: the wall clock, not the fiber's `Clock` (a test clock does not
// move it); read the fiber's clock once the Go `Clock` is ported.
func cryptoMillis() uint64 {
	return uint64(min(max(0, time.Now().UnixMilli()), 1<<48-1))
}

func CryptoRandomUuidV7() Value {
	b := cryptoBytes(16)
	t := cryptoMillis()
	for i := 0; i < 6; i++ {
		b[i] = byte(t >> (40 - 8*i))
	}
	b[6] = b[6]&0x0f | 0x70
	b[8] = b[8]&0x3f | 0x80
	return uuidString(b)
}

const crockford = "0123456789ABCDEFGHJKMNPQRSTVWXYZ"

func CryptoRandomUlid() Value {
	t := cryptoMillis()
	out := make([]byte, 0, 26)
	for shift := 45; shift >= 0; shift -= 5 {
		out = append(out, crockford[t>>shift&0x1f])
	}
	acc, bits := 0, 0
	for _, b := range cryptoBytes(10) {
		acc = acc<<8 | int(b)
		bits += 8
		for bits >= 5 {
			bits -= 5
			out = append(out, crockford[acc>>bits&0x1f])
		}
		acc &= 1<<bits - 1
	}
	return string(out)
}
