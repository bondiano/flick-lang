package flickrt

import (
	"crypto/subtle"
	"encoding/base64"
	"strings"
	"unicode/utf8"
)

// packedBytes holds raw bytes in immutable storage, separate from Flick String.
type packedBytes string

var BytesEmpty Value = packedBytes("")

func BytesLength(bytes Value) Value { return int32(len(bytes.(packedBytes))) }

func BytesGet(index, bytes Value) Value {
	data := bytes.(packedBytes)
	i := int(index.(int32))
	if i < 0 || i >= len(data) {
		return None
	}
	return Some(int32(data[i]))
}

func BytesSlice(start, end, bytes Value) Value {
	data := bytes.(packedBytes)
	lo, hi := int(start.(int32)), int(end.(int32))
	if lo < 0 || hi < lo || hi > len(data) {
		return None
	}
	if lo == hi {
		return Some(BytesEmpty)
	}
	return Some(data[lo:hi])
}

func BytesFromList(list Value) Value {
	xs := List(list)
	var buffer strings.Builder
	buffer.Grow(len(xs))
	for i, v := range xs {
		n := v.(int32)
		if n < 0 || n > 255 {
			return Err([]Value{int32(i), n})
		}
		buffer.WriteByte(byte(n))
	}
	return Ok(packedBytes(buffer.String()))
}

func BytesToList(bytes Value) Value { return byteList([]byte(bytes.(packedBytes))) }

func BytesEncodeUtf8(text Value) Value { return packedBytes(wellFormed(text.(string))) }

func BytesDecodeUtf8(bytes Value) Value {
	text := string(bytes.(packedBytes))
	if !utf8.ValidString(text) {
		return None
	}
	return Some(text)
}

func BytesEncodeBase64URL(bytes Value) Value {
	return base64.RawURLEncoding.EncodeToString([]byte(bytes.(packedBytes)))
}

func BytesDecodeBase64URL(text Value) Value {
	input := text.(string)
	// Go's decoders ignore CR and LF. Bytes rejects whitespace at the API.
	if strings.ContainsAny(input, "\r\n") {
		return None
	}
	encoding := base64.RawURLEncoding
	if strings.HasSuffix(input, "=") {
		encoding = base64.URLEncoding
	}
	data, err := encoding.DecodeString(input)
	if err != nil {
		return None
	}
	return Some(packedBytes(data))
}

func BytesEqualConstantTime(a, b Value) Value {
	return subtle.ConstantTimeCompare([]byte(a.(packedBytes)), []byte(b.(packedBytes))) == 1
}

// The list-based Bytes functions keep the conversions of the Crypto adapter.

// CryptoUtf8Encode writes a lone surrogate as U+FFFD, as `TextEncoder` does.
func CryptoUtf8Encode(text Value) Value {
	return byteList([]byte(wellFormed(text.(string))))
}

func CryptoUtf8Decode(bytes Value) Value {
	b := []byte(byteString(bytes))
	if !utf8.Valid(b) {
		return None
	}
	return Some(string(b))
}

func CryptoBase64UrlEncode(bytes Value) Value {
	return base64.RawURLEncoding.EncodeToString([]byte(byteString(bytes)))
}

func CryptoBase64UrlDecode(text Value) Value {
	b, ok := decodeBase64Url(text.(string))
	if !ok {
		return None
	}
	return Some(byteList(b))
}
