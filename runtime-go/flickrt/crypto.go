package flickrt

import (
	"crypto/hmac"
	"crypto/rand"
	"crypto/sha1"
	"crypto/sha256"
	"crypto/sha512"
	"fmt"
	gohash "hash"
	"math"
)

// `Crypto` made over the platform's randomness and digests, as the JS
// adapter's over Web Crypto; a `PlatformError` becomes a `CryptoError`.

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

// cryptoHash is the hash function of a `DigestAlgorithm`.
func cryptoHash(algorithm Value) func() gohash.Hash {
	switch ConstructorName(algorithm.(*Data).Tag) {
	case "Sha1":
		return sha1.New
	case "Sha384":
		return sha512.New384
	case "Sha512":
		return sha512.New
	default:
		return sha256.New
	}
}

func CryptoDigest(algorithm, data Value) Value {
	h := cryptoHash(algorithm)()
	h.Write([]byte(byteString(data)))
	return byteList(h.Sum(nil))
}

// CryptoPbkdf2 is PBKDF2 (RFC 8018) over HMAC, as Web Crypto's `deriveBits`.
func CryptoPbkdf2(algorithm, iterations, length, salt, password Value) Value {
	rounds, size := int(iterations.(int32)), int(length.(int32))
	if rounds <= 0 || size <= 0 {
		return Fail(NewData(cryptoErrorTag, "Crypto.pbkdf2: iterations and length must be positive"))
	}
	prf := hmac.New(cryptoHash(algorithm), []byte(byteString(password)))
	s := []byte(byteString(salt))
	out := make([]byte, 0, size+prf.Size())
	u := make([]byte, 0, prf.Size())
	t := make([]byte, prf.Size())
	for block := uint32(1); len(out) < size; block++ {
		prf.Reset()
		prf.Write(s)
		prf.Write([]byte{byte(block >> 24), byte(block >> 16), byte(block >> 8), byte(block)})
		u = prf.Sum(u[:0])
		copy(t, u)
		for i := 1; i < rounds; i++ {
			prf.Reset()
			prf.Write(u)
			u = prf.Sum(u[:0])
			for j := range t {
				t[j] ^= u[j]
			}
		}
		out = append(out, t...)
	}
	return byteList(out[:size])
}

// cryptoDouble is 53 random bits over 2^53.
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

func CryptoRandomBetween(min, max Value) Value {
	return nextBetween(min.(float64), max.(float64), cryptoDouble())
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

// cryptoMillis is the fiber's `Clock` time of UUID v7 and ULID, clamped to
// 48 bits.
func cryptoMillis(fb *Fiber) uint64 {
	return uint64(min(max(0, math.Trunc(clockOf(fb).millis())), 1<<48-1))
}

func CryptoRandomUuidV7(fb *Fiber) Value {
	b := cryptoBytes(16)
	t := cryptoMillis(fb)
	for i := 0; i < 6; i++ {
		b[i] = byte(t >> (40 - 8*i))
	}
	b[6] = b[6]&0x0f | 0x70
	b[8] = b[8]&0x3f | 0x80
	return uuidString(b)
}

const crockford = "0123456789ABCDEFGHJKMNPQRSTVWXYZ"

func CryptoRandomUlid(fb *Fiber) Value {
	t := cryptoMillis(fb)
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
