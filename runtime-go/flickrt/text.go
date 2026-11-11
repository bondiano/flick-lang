package flickrt

import (
	"math"
	"math/big"
	"regexp"
	"strconv"
	"strings"
)

// The foreign functions of `String` that count code units as JS does;
// each takes all its arguments at once, the string last.

// isJSSpace is the white space and line terminators JS `trim` removes.
func isJSSpace(r rune) bool {
	switch r {
	case '\t', '\n', '\v', '\f', '\r', ' ', 0xA0, 0x1680, 0x2028, 0x2029, 0x202F, 0x205F, 0x3000, 0xFEFF:
		return true
	}
	return r >= 0x2000 && r <= 0x200A
}

func Trim(s Value) Value      { return strings.TrimFunc(s.(string), isJSSpace) }
func TrimStart(s Value) Value { return strings.TrimLeftFunc(s.(string), isJSSpace) }
func TrimEnd(s Value) Value   { return strings.TrimRightFunc(s.(string), isJSSpace) }

func IsNonEmpty(s Value) Value { return s.(string) != "" }

// hasSurrogate tells whether a string holds a lone surrogate, which a
// byte-level operation could split or fail to join.
func hasSurrogate(s string) bool {
	for i := 0; i+1 < len(s); i++ {
		if s[i] == 0xED && s[i+1] >= 0xA0 {
			return true
		}
	}
	return false
}

// Split is JS `s.split(separator)`, never empty: the empty separator
// splits into code units.
func Split(separator, s Value) Value {
	sep, text := separator.(string), s.(string)
	if sep == "" {
		units := utf16Units(text)
		if len(units) == 0 {
			return []Value{""}
		}
		out := make([]Value, len(units))
		for i := range units {
			out[i] = fromUnits(units[i : i+1])
		}
		return out
	}
	parts := strings.Split(text, sep)
	out := make([]Value, len(parts))
	for i, p := range parts {
		out[i] = p
	}
	return out
}

// StringJoin joins the parts with a separator; halves of a pair at a joint
// become the pair.
func StringJoin(separator, parts Value) Value {
	sep, in := separator.(string), List(parts)
	strs := make([]string, len(in))
	split := hasSurrogate(sep)
	for i, p := range in {
		strs[i] = p.(string)
		split = split || hasSurrogate(strs[i])
	}
	if !split {
		return strings.Join(strs, sep)
	}
	var units []uint16
	for i, p := range strs {
		if i > 0 {
			units = append(units, utf16Units(sep)...)
		}
		units = append(units, utf16Units(p)...)
	}
	return fromUnits(units)
}

// shortcut: byte search on WTF-8 can find half of a pair inside a whole one; exact once strings are code units
func Includes(part, s Value) Value   { return strings.Contains(s.(string), part.(string)) }
func StartsWith(part, s Value) Value { return strings.HasPrefix(s.(string), part.(string)) }
func EndsWith(part, s Value) Value   { return strings.HasSuffix(s.(string), part.(string)) }

func count(n Value) int { return max(0, int(n.(int32))) }

// In a string of ASCII a code unit is a byte, so its ends are taken and it
// is padded as it is, without its copy in UTF-16.
func TakeLeft(n, s Value) Value {
	if text := s.(string); ascii(text) {
		return text[:min(count(n), len(text))]
	}
	units := utf16Units(s.(string))
	return fromUnits(units[:min(count(n), len(units))])
}

func TakeRight(n, s Value) Value {
	if text := s.(string); ascii(text) {
		return text[len(text)-min(count(n), len(text)):]
	}
	units := utf16Units(s.(string))
	return fromUnits(units[len(units)-min(count(n), len(units)):])
}

// Repeat repeats a string; a negative count is 0.
func Repeat(n, s Value) Value { return strings.Repeat(s.(string), count(n)) }

func pad(n, fill, s Value, start bool) Value {
	if text, with := s.(string), fill.(string); ascii(text) && ascii(with) {
		missing := int(n.(int32)) - len(text)
		if missing <= 0 || with == "" {
			return s
		}
		padding := strings.Repeat(with, (missing+len(with)-1)/len(with))[:missing]
		if start {
			return padding + text
		}
		return text + padding
	}
	units, filler := utf16Units(s.(string)), utf16Units(fill.(string))
	missing := int(n.(int32)) - len(units)
	if missing <= 0 || len(filler) == 0 {
		return s
	}
	padding := make([]uint16, 0, missing)
	for len(padding) < missing {
		padding = append(padding, filler[:min(len(filler), missing-len(padding))]...)
	}
	if start {
		return fromUnits(append(padding, units...))
	}
	return fromUnits(append(units, padding...))
}

// PadStart and PadEnd are JS `padStart` and `padEnd` to `n` code units.
func PadStart(n, fill, s Value) Value { return pad(n, fill, s, true) }
func PadEnd(n, fill, s Value) Value   { return pad(n, fill, s, false) }

// StringReplaceAll replaces every `pattern` with `replacement` literally;
// the empty pattern goes between code units.
func StringReplaceAll(pattern, replacement, s Value) Value {
	p, r, text := pattern.(string), replacement.(string), s.(string)
	if p != "" && !hasSurrogate(text) {
		return strings.ReplaceAll(text, p, r)
	}
	if p == "" {
		units := utf16Units(text)
		var out []uint16
		with := utf16Units(r)
		for _, u := range units {
			out = append(append(out, with...), u)
		}
		return fromUnits(append(out, with...))
	}
	return fromUnits(utf16Units(strings.ReplaceAll(text, p, r)))
}

var (
	decimal = regexp.MustCompile(`^[+-]?(Infinity|([0-9]+\.?[0-9]*|\.[0-9]+)([eE][+-]?[0-9]+)?)$`)
	radix   = regexp.MustCompile(`^0([xX][0-9a-fA-F]+|[oO][0-7]+|[bB][01]+)$`)
)

// StringToNumber is the number `s` spells in JS syntax, as `Number(s)`
// reads it; blank and malformed text are `None`.
func StringToNumber(s Value) Value {
	text := strings.TrimFunc(s.(string), isJSSpace)
	switch {
	case text == "":
		return None
	case text == "NaN":
		return Some(math.NaN())
	case radix.MatchString(text):
		n, _ := new(big.Int).SetString(text, 0)
		f, _ := new(big.Float).SetInt(n).Float64()
		return Some(f)
	case decimal.MatchString(text):
		if strings.HasSuffix(text, "Infinity") {
			return Some(math.Inf(1 - 2*boolInt(text[0] == '-')))
		}
		// ParseFloat reads "5." and ".5" and rounds as JS does; out of range is ±Infinity
		f, err := strconv.ParseFloat(text, 64)
		if err != nil && !math.IsInf(f, 0) {
			return None
		}
		return Some(f)
	}
	return None
}
