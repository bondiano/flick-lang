package flickrt

import (
	"reflect"
	"testing"
)

// The strings a place in UTF-16 units is tested on: of ASCII, of the BMP on
// both sides of the surrogates, past the BMP, and with a lone surrogate.
var unitStrings = []string{
	"", "a", "abc", "0123456789", "a\x00b", "é", "aéb", "\uE000", "\uFFFD", "ｚ", "😀", "a😀b", "😀😀",
	"z😀ｚ", "\xED\xA0\x80", "a\xED\xB0\x80", "\xED\xA0\x80x", "é😀\uE000z",
	// the high surrogate of 😀 alone, then nothing, a unit below and a unit above its low one
	"\xED\xA0\xBD", "\xED\xA0\xBDa", "\xED\xA0\xBD\uE000", "\xED\xA0\xBD\xED\xA0\xBD",
}

// unitsOf is the reference: the whole string in UTF-16.
func unitsOf(s string) []uint16 { return utf16Units(s) }

func TestPlacesInAStringAreItsUtf16Units(t *testing.T) {
	for _, s := range unitStrings {
		units := unitsOf(s)
		for k := -2; k <= len(units)+1; k++ {
			var code Value = None
			if k >= 0 && k < len(units) {
				code = Some(int32(units[k]))
			}
			if got := CharCodeAt(int32(k), s); !reflect.DeepEqual(got, code) {
				t.Errorf("CharCodeAt %d of %q: %v, want %v", k, s, got, code)
			}
			var char Value = None
			if k >= 0 && k < len(units) {
				char = Some(fromUnits(units[k : k+1]))
			}
			if got := CharAt(int32(k), s); !reflect.DeepEqual(got, char) {
				t.Errorf("CharAt %d of %q: %v, want %v", k, s, got, char)
			}
			for to := -2; to <= len(units)+1; to++ {
				from, end := position(int32(k), len(units), true), position(int32(to), len(units), true)
				want := ""
				if from < end {
					want = fromUnits(units[from:end])
				}
				if got := Slice(int32(k), int32(to), s); got != want {
					t.Errorf("Slice %d %d of %q: %q, want %q", k, to, s, got, want)
				}
			}
		}
	}
}

func TestStringsCompareByTheirUtf16Units(t *testing.T) {
	sign := func(n int) int {
		switch {
		case n < 0:
			return -1
		case n > 0:
			return 1
		}
		return 0
	}
	byUnits := func(a, b string) int {
		x, y := unitsOf(a), unitsOf(b)
		for k := 0; k < len(x) && k < len(y); k++ {
			if x[k] != y[k] {
				return int(x[k]) - int(y[k])
			}
		}
		return len(x) - len(y)
	}
	for _, a := range unitStrings {
		for _, b := range unitStrings {
			if got, want := sign(CompareStrings(a, b)), sign(byUnits(a, b)); got != want {
				t.Errorf("%q against %q: %d, want %d", a, b, got, want)
			}
		}
	}
}

func TestTheHashOfAStringIsOfItsUtf16Units(t *testing.T) {
	for _, s := range unitStrings {
		units := unitsOf(s)
		h := int32(5381)
		for i := len(units) - 1; i >= 0; i-- {
			h = h*33 ^ int32(units[i])
		}
		if got := hashString(s); got != optimize(h) {
			t.Errorf("hash of %q: %d, want %d", s, got, optimize(h))
		}
	}
}
