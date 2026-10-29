package flickrt

import (
	"strings"
	"unicode/utf8"
)

// Percent-encoding as the URL Standard and JS's URI functions do it. A Go
// string holds WTF-8: what goes on the wire is its UTF-8 with each lone
// surrogate replaced, as a USVString is.

const upperHex = "0123456789ABCDEF"

func hexValue(c byte) (byte, bool) {
	switch {
	case c >= '0' && c <= '9':
		return c - '0', true
	case c >= 'a' && c <= 'f':
		return c - 'a' + 10, true
	case c >= 'A' && c <= 'F':
		return c - 'A' + 10, true
	}
	return 0, false
}

// percentEncode encodes the bytes of `s` that `set` holds.
func percentEncode(s string, set func(byte) bool) string {
	s = wellFormed(s)
	var b strings.Builder
	for i := 0; i < len(s); i++ {
		if c := s[i]; set(c) {
			b.WriteByte('%')
			b.WriteByte(upperHex[c>>4])
			b.WriteByte(upperHex[c&15])
		} else {
			b.WriteByte(c)
		}
	}
	return b.String()
}

// percentDecode is the bytes of `s` with each `%XX` decoded; a `%` without
// two hex digits stays.
func percentDecode(s string) []byte {
	out := make([]byte, 0, len(s))
	for i := 0; i < len(s); i++ {
		if s[i] == '%' && i+2 < len(s) {
			hi, ok1 := hexValue(s[i+1])
			lo, ok2 := hexValue(s[i+2])
			if ok1 && ok2 {
				out = append(out, hi<<4|lo)
				i += 2
				continue
			}
		}
		out = append(out, s[i])
	}
	return out
}

// utf8Lossy decodes UTF-8 as the Encoding Standard does: each maximal
// invalid subpart becomes one U+FFFD.
func utf8Lossy(b []byte) string {
	if utf8.Valid(b) {
		return string(b)
	}
	var out strings.Builder
	for len(b) > 0 {
		r, n := utf8.DecodeRune(b)
		if r != utf8.RuneError || n > 1 {
			out.Write(b[:n])
			b = b[n:]
			continue
		}
		out.WriteString("�")
		b = b[invalidPrefix(b):]
	}
	return out.String()
}

// invalidPrefix is how many bytes of a sequence that does not decode make
// one maximal subpart: a lead byte and the continuation bytes it allows.
func invalidPrefix(b []byte) int {
	lo, hi, need := byte(0x80), byte(0xBF), 0
	switch c := b[0]; {
	case c >= 0xC2 && c <= 0xDF:
		need = 1
	case c == 0xE0:
		lo, need = 0xA0, 2
	case c == 0xED:
		hi, need = 0x9F, 2
	case c >= 0xE1 && c <= 0xEF:
		need = 2
	case c == 0xF0:
		lo, need = 0x90, 3
	case c >= 0xF1 && c <= 0xF3:
		need = 3
	case c == 0xF4:
		hi, need = 0x8F, 3
	}
	n := 1
	for n <= need && n < len(b) && b[n] >= lo && b[n] <= hi {
		lo, hi = 0x80, 0xBF
		n++
	}
	return n
}

// formByte is whether application/x-www-form-urlencoded escapes the byte.
func formByte(c byte) bool {
	return !(c >= 'a' && c <= 'z' || c >= 'A' && c <= 'Z' || c >= '0' && c <= '9' ||
		c == '*' || c == '-' || c == '.' || c == '_')
}

func formPart(b *strings.Builder, s string) {
	s = wellFormed(s)
	for i := 0; i < len(s); i++ {
		switch c := s[i]; {
		case c == ' ':
			b.WriteByte('+')
		case formByte(c):
			b.WriteByte('%')
			b.WriteByte(upperHex[c>>4])
			b.WriteByte(upperHex[c&15])
		default:
			b.WriteByte(c)
		}
	}
}

// formEncode is URLSearchParams' text of the pairs.
func formEncode(pairs []Value) string {
	var b strings.Builder
	for i, kv := range pairs {
		if i > 0 {
			b.WriteByte('&')
		}
		k, v := pairOf(kv)
		formPart(&b, k)
		b.WriteByte('=')
		formPart(&b, v)
	}
	return b.String()
}

// formDecode is the pairs of a query string as URLSearchParams reads it.
func formDecode(s string) []Value {
	out := []Value{}
	for _, part := range strings.Split(wellFormed(s), "&") {
		if part == "" {
			continue
		}
		k, v, _ := strings.Cut(part, "=")
		out = append(out, []Value{formText(k), formText(v)})
	}
	return out
}

func formText(s string) string {
	return utf8Lossy(percentDecode(strings.ReplaceAll(s, "+", " ")))
}

// decodeURIComponent is JS's: false where JS throws a URIError.
func decodeURIComponent(s string) (string, bool) {
	if !strings.Contains(s, "%") {
		return s, true
	}
	var b strings.Builder
	for i := 0; i < len(s); {
		if s[i] != '%' {
			b.WriteByte(s[i])
			i++
			continue
		}
		var run []byte
		for i < len(s) && s[i] == '%' {
			if i+2 >= len(s) {
				return "", false
			}
			hi, ok1 := hexValue(s[i+1])
			lo, ok2 := hexValue(s[i+2])
			if !ok1 || !ok2 {
				return "", false
			}
			run = append(run, hi<<4|lo)
			i += 3
		}
		if !utf8.Valid(run) {
			return "", false
		}
		b.Write(run)
	}
	return b.String(), true
}

// encodeURIComponent is JS's: false where a lone surrogate makes JS throw.
func encodeURIComponent(s string) (string, bool) {
	if wellFormed(s) != s {
		return "", false
	}
	return percentEncode(s, func(c byte) bool {
		return !(c >= 'a' && c <= 'z' || c >= 'A' && c <= 'Z' || c >= '0' && c <= '9' ||
			strings.IndexByte("-_.!~*'()", c) >= 0)
	}), true
}
