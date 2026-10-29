package flickrt

import (
	"strconv"
	"strings"
	"unicode/utf8"
)

// A String is WTF-8: UTF-8 that may also hold a lone surrogate as a
// three-byte sequence, so it holds any JS string. Positions and lengths
// count UTF-16 code units, as JS does; a surrogate pair is never stored as
// two halves.

// decode is the code point at the start of `s` and its length in bytes,
// surrogates included; a malformed byte is U+FFFD.
func decode(s string) (rune, int) {
	if len(s) >= 3 && s[0] == 0xED && s[1] >= 0xA0 && s[1] <= 0xBF && s[2]&0xC0 == 0x80 {
		return 0xD000 | rune(s[1]&0x3F)<<6 | rune(s[2]&0x3F), 3
	}
	return utf8.DecodeRuneInString(s)
}

// encodeRune appends `r` in WTF-8.
func encodeRune(buf []byte, r rune) []byte {
	if r >= 0xD800 && r <= 0xDFFF {
		return append(buf, 0xED, byte(0x80|(r>>6)&0x3F), byte(0x80|r&0x3F))
	}
	return utf8.AppendRune(buf, r)
}

// utf16Units is the UTF-16 code units of `s`.
func utf16Units(s string) []uint16 {
	out := make([]uint16, 0, len(s))
	for len(s) > 0 {
		r, n := decode(s)
		s = s[n:]
		if r >= 0x10000 {
			r -= 0x10000
			out = append(out, uint16(0xD800+(r>>10)), uint16(0xDC00+r&0x3FF))
		} else {
			out = append(out, uint16(r))
		}
	}
	return out
}

// fromUnits is the string of UTF-16 code units, pairs joined.
func fromUnits(units []uint16) string {
	buf := make([]byte, 0, len(units))
	for i := 0; i < len(units); i++ {
		u := rune(units[i])
		if u >= 0xD800 && u <= 0xDBFF && i+1 < len(units) && units[i+1] >= 0xDC00 && units[i+1] <= 0xDFFF {
			u = 0x10000 + (u-0xD800)<<10 + rune(units[i+1]) - 0xDC00
			i++
		}
		buf = encodeRune(buf, u)
	}
	return string(buf)
}

// Length is the number of UTF-16 code units of a string.
func Length(v Value) Value {
	s := v.(string)
	n := int32(0)
	for i := 0; i < len(s); i++ {
		switch b := s[i]; {
		case b < 0x80 || b >= 0xC0 && b < 0xF0:
			n++
		case b >= 0xF0:
			n += 2
		}
	}
	return n
}

// Concat is `++` on strings: a high surrogate at the end of `a` and a low
// one at the start of `b` become one pair.
func Concat(a, b Value) Value {
	x, y := a.(string), b.(string)
	if len(x) >= 3 && len(y) >= 3 {
		hi, n := decode(x[len(x)-3:])
		lo, m := decode(y)
		if n == 3 && m == 3 && hi >= 0xD800 && hi <= 0xDBFF && lo >= 0xDC00 && lo <= 0xDFFF {
			pair := 0x10000 + (hi-0xD800)<<10 + lo - 0xDC00
			return x[:len(x)-3] + string(pair) + y[3:]
		}
	}
	return x + y
}

// CompareStrings orders strings by UTF-16 code units, as JS does.
func CompareStrings(a, b string) int {
	i := 0
	for i < len(a) && i < len(b) && a[i] == b[i] {
		i++
	}
	if i == len(a) || i == len(b) {
		return len(a) - len(b)
	}
	// both differ inside the code point starting at the last lead byte
	for i > 0 && a[i]&0xC0 == 0x80 {
		i--
	}
	x, y := utf16Units(a[i:]), utf16Units(b[i:])
	for k := 0; k < len(x) && k < len(y); k++ {
		if x[k] != y[k] {
			return int(x[k]) - int(y[k])
		}
	}
	return len(x) - len(y)
}

// QuoteJSON is JS `JSON.stringify` of a string; a lone surrogate is
// escaped.
func QuoteJSON(s string) string {
	var b strings.Builder
	b.WriteByte('"')
	for len(s) > 0 {
		r, n := decode(s)
		switch {
		case r == '"':
			b.WriteString(`\"`)
		case r == '\\':
			b.WriteString(`\\`)
		case r == '\b':
			b.WriteString(`\b`)
		case r == '\f':
			b.WriteString(`\f`)
		case r == '\n':
			b.WriteString(`\n`)
		case r == '\r':
			b.WriteString(`\r`)
		case r == '\t':
			b.WriteString(`\t`)
		case r < 0x20 || r >= 0xD800 && r <= 0xDFFF:
			hex := strconv.FormatInt(int64(r), 16)
			b.WriteString(`\u` + strings.Repeat("0", 4-len(hex)) + hex)
		default:
			b.WriteString(s[:n])
		}
		s = s[n:]
	}
	b.WriteByte('"')
	return b.String()
}

// wellFormed is `s` with each lone surrogate replaced by U+FFFD, as Node
// writes a string to a stream.
func wellFormed(s string) string {
	if !strings.Contains(s, "\xED") {
		return s
	}
	buf := make([]byte, 0, len(s))
	for len(s) > 0 {
		r, n := decode(s)
		if r >= 0xD800 && r <= 0xDFFF {
			r = utf8.RuneError
		}
		buf = utf8.AppendRune(buf, r)
		s = s[n:]
	}
	return string(buf)
}

// position is `i` clamped into `0..n`, negative counting from the end when
// `fromEnd`, as JS `slice` takes it.
func position(i int32, n int, fromEnd bool) int {
	k := int(i)
	if k < 0 && fromEnd {
		k += n
	}
	return max(0, min(k, n))
}

// Slice is JS `s.slice(start, end)`.
func Slice(start, end, v Value) Value {
	units := utf16Units(v.(string))
	from := position(start.(int32), len(units), true)
	to := position(end.(int32), len(units), true)
	if from >= to {
		return ""
	}
	return fromUnits(units[from:to])
}

// Substring is JS `s.substring(start, end)`: negative is 0, and the bounds
// are swapped when out of order.
func Substring(start, end, v Value) Value {
	units := utf16Units(v.(string))
	from := position(start.(int32), len(units), false)
	to := position(end.(int32), len(units), false)
	if from > to {
		from, to = to, from
	}
	return fromUnits(units[from:to])
}

// CharCodeAt is the code unit at `i`, if any.
func CharCodeAt(i, v Value) Value {
	units := utf16Units(v.(string))
	k := int(i.(int32))
	if k < 0 || k >= len(units) {
		return None
	}
	return Some(int32(units[k]))
}

// CharAt is the one-unit string at `i`, if any.
func CharAt(i, v Value) Value {
	units := utf16Units(v.(string))
	k := int(i.(int32))
	if k < 0 || k >= len(units) {
		return None
	}
	return Some(fromUnits(units[k : k+1]))
}

// StringAt is the one-unit string at `i`, negative counting from the end.
func StringAt(i, v Value) Value {
	units := utf16Units(v.(string))
	k := int(i.(int32))
	if k < 0 {
		k += len(units)
	}
	if k < 0 || k >= len(units) {
		return None
	}
	return Some(fromUnits(units[k : k+1]))
}

// CodePointAt is the code point starting at unit `i`, if any.
func CodePointAt(i, v Value) Value {
	units := utf16Units(v.(string))
	k := int(i.(int32))
	if k < 0 || k >= len(units) {
		return None
	}
	u := rune(units[k])
	if u >= 0xD800 && u <= 0xDBFF && k+1 < len(units) && units[k+1] >= 0xDC00 && units[k+1] <= 0xDFFF {
		u = 0x10000 + (u-0xD800)<<10 + rune(units[k+1]) - 0xDC00
	}
	return Some(int32(u))
}

// indexUnits is the unit index of the byte offset `b` of `s`.
func indexUnits(s string, b int) int32 { return Length(s[:b]).(int32) }

// IndexOf is the unit position of the first `part` in `s`, if any.
func IndexOf(part, v Value) Value {
	s := v.(string)
	b := strings.Index(s, part.(string))
	if b < 0 {
		return None
	}
	return Some(indexUnits(s, b))
}

// LastIndexOf is the unit position of the last `part` in `s`, if any.
func LastIndexOf(part, v Value) Value {
	s := v.(string)
	b := strings.LastIndex(s, part.(string))
	if b < 0 {
		return None
	}
	return Some(indexUnits(s, b))
}

// Chars is the code points of a string, each a string.
func Chars(v Value) Value {
	s := v.(string)
	out := []Value{}
	for len(s) > 0 {
		_, n := decode(s)
		out = append(out, s[:n])
		s = s[n:]
	}
	return out
}

// FromCharCode is the string of one code unit, as JS `String.fromCharCode`.
func FromCharCode(code Value) Value {
	return fromUnits([]uint16{uint16(code.(int32))})
}
