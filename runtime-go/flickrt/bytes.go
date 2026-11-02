package flickrt

import (
	"encoding/base64"
	"unicode/utf8"
)

// `Bytes`: text as UTF-8, bytes as URL-safe base64, as the JS adapter's
// `TextEncoder`, fatal `TextDecoder` and Effect's `Base64Url`.

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
