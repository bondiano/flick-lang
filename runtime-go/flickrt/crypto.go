package flickrt

import (
	"crypto/rand"
	"crypto/sha1"
	"crypto/sha256"
	"crypto/sha512"
	"encoding/binary"
	"encoding/hex"
	"math"
	"strings"
)

// Effect's `Crypto` made over the system's secure randomness: numbers are
// drawn from random bytes as Effect draws them, so they are spread alike.

var cryptoErrorTag = Tag("Crypto.CryptoError.CryptoError")

func cryptoBytes(n int) []byte {
	b := make([]byte, n)
	rand.Read(b)
	return b
}

// nextDouble is a number from 0 up to 1 of 53 random bits.
func nextDouble() float64 {
	b := cryptoBytes(7)
	v := float64(b[0]&0x1f)*(1<<48) + float64(b[1])*(1<<40) + float64(b[2])*(1<<32) +
		float64(b[3])*(1<<24) + float64(b[4])*(1<<16) + float64(b[5])*(1<<8) + float64(b[6])
	return v / (1 << 53)
}

func CryptoRandomBytes(_ *Fiber, size Value) Value {
	n := size.(int32)
	if n < 0 {
		return Fail(NewData(cryptoErrorTag, "Crypto.randomBytes: size must be a non-negative safe integer"))
	}
	return byteList(cryptoBytes(int(n)))
}

func CryptoDigest(_ *Fiber, algorithm, data Value) Value {
	b := []byte(byteString(data))
	var sum []byte
	switch ConstructorName(algorithm.(*Data).Tag) {
	case "Sha1":
		s := sha1.Sum(b)
		sum = s[:]
	case "Sha384":
		s := sha512.Sum384(b)
		sum = s[:]
	case "Sha512":
		s := sha512.Sum512(b)
		sum = s[:]
	default:
		s := sha256.Sum256(b)
		sum = s[:]
	}
	return byteList(sum)
}

func CryptoRandom(*Fiber) Value        { return nextDouble() }
func CryptoRandomBoolean(*Fiber) Value { return nextDouble() > 0.5 }

// CryptoRandomInt is over the 32 bits of `Int`.
func CryptoRandomInt(*Fiber) Value { return randomIntBetween(-0x80000000, 0x7fffffff) }

// CryptoRandomBetween is from `min` up to `max`; a draw rounded up to
// `max` is the number just below it.
func CryptoRandomBetween(_ *Fiber, min, max Value) Value {
	lo, hi := min.(float64), max.(float64)
	v := nextDouble()*(hi-lo) + lo
	if v != hi || lo >= hi || math.IsInf(hi, 0) || math.IsNaN(hi) {
		return v
	}
	return math.Nextafter(hi, math.Inf(-1))
}

func CryptoRandomIntBetween(_ *Fiber, min, max Value) Value {
	return randomIntBetween(float64(min.(int32)), float64(max.(int32)))
}

func randomIntBetween(min, max float64) Value {
	return int32(math.Floor(nextDouble()*(max-min+1)) + min)
}

func CryptoRandomShuffle(_ *Fiber, xs Value) Value {
	out := append([]Value{}, List(xs)...)
	for i := len(out) - 1; i >= 1; i-- {
		j := min(i, int(math.Floor(nextDouble()*float64(i+1))))
		out[i], out[j] = out[j], out[i]
	}
	return out
}

func uuidString(b []byte) string {
	h := hex.EncodeToString(b)
	return strings.Join([]string{h[:8], h[8:12], h[12:16], h[16:20], h[20:]}, "-")
}

func CryptoRandomUuidV4(*Fiber) Value {
	b := cryptoBytes(16)
	b[6] = b[6]&0x0f | 0x40
	b[8] = b[8]&0x3f | 0x80
	return uuidString(b)
}

// timestamp48 is the clock's time in whole milliseconds, within 48 bits.
func timestamp48(fb *Fiber) uint64 {
	return uint64(math.Min(math.Max(0, math.Trunc(clockOf(fb).millis())), 1<<48-1))
}

// CryptoRandomUuidV7 is a version 7 UUID of the fiber's clock.
func CryptoRandomUuidV7(fb *Fiber) Value {
	b := cryptoBytes(16)
	var t [8]byte
	binary.BigEndian.PutUint64(t[:], timestamp48(fb))
	copy(b[:6], t[2:])
	b[6] = b[6]&0x0f | 0x70
	b[8] = b[8]&0x3f | 0x80
	return uuidString(b)
}

// CryptoRandomUlid is a ULID of the fiber's clock, in Crockford's base 32.
func CryptoRandomUlid(fb *Fiber) Value {
	const chars = "0123456789ABCDEFGHJKMNPQRSTVWXYZ"
	t := timestamp48(fb)
	var out strings.Builder
	for shift := 45; shift >= 0; shift -= 5 {
		out.WriteByte(chars[(t>>shift)&0x1f])
	}
	acc, bits := 0, 0
	for _, b := range cryptoBytes(10) {
		acc = acc<<8 | int(b)
		bits += 8
		for bits >= 5 {
			bits -= 5
			out.WriteByte(chars[acc>>bits&0x1f])
		}
		acc &= 1<<bits - 1
	}
	return out.String()
}
