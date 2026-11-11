package flickrt

import (
	"strings"
	"testing"
)

// The same text with a letter past ASCII before it: the functions take it
// through UTF-16, and after that letter is dropped the answers must agree.
func TestAStringOfAsciiIsCasedTakenAndPaddedAsAnyString(t *testing.T) {
	texts := []string{"", "a", "Hello, World", "ALL CAPS 123", "mixed Case_and-more", "a\x00b\x7f", strings.Repeat("xY", 40)}
	const wide = "é"
	for _, text := range texts {
		if got, want := ToUpperCase(text), mapCase(text, unicodeData().upper, nil); got != want {
			t.Errorf("ToUpperCase of %q: %q, want %q", text, got, want)
		}
		if got, want := ToLowerCase(text), mapCase(text, unicodeData().lower, finalSigma); got != want {
			t.Errorf("ToLowerCase of %q: %q, want %q", text, got, want)
		}
		for n := int32(-1); n <= int32(len(text))+2; n++ {
			// `wide` is one unit at the far end, so one more unit is taken and it is dropped
			if got, want := TakeLeft(n, text), strings.TrimSuffix(TakeLeft(n, text+wide).(string), wide); got != want {
				t.Errorf("TakeLeft %d of %q: %q, want %q", n, text, got, want)
			}
			if got, want := TakeRight(n, text), strings.TrimPrefix(TakeRight(n, wide+text).(string), wide); got != want {
				t.Errorf("TakeRight %d of %q: %q, want %q", n, text, got, want)
			}
			for _, fill := range []string{"", " ", "ab", "abc"} {
				// padded to one unit more with `wide` in it, which is then dropped
				if got, want := PadStart(n, fill, text), strings.Replace(PadStart(n+1, fill, text+wide).(string), wide, "", 1); got != want {
					t.Errorf("PadStart %d %q of %q: %q, want %q", n, fill, text, got, want)
				}
				if got, want := PadEnd(n, fill, text), strings.Replace(PadEnd(n+1, fill, wide+text).(string), wide, "", 1); got != want {
					t.Errorf("PadEnd %d %q of %q: %q, want %q", n, fill, text, got, want)
				}
			}
		}
	}
}

var textSink Value

// A line of text, as it is and with one letter past ASCII, which sends it
// the way every string went.
func BenchmarkCaseTakeAndPad(b *testing.B) {
	line := strings.Repeat("The quick brown fox ", 5)
	for _, c := range []struct{ name, text string }{{"ascii", line}, {"other", line + "é"}} {
		b.Run("upper/"+c.name, func(b *testing.B) {
			b.ReportAllocs()
			for i := 0; i < b.N; i++ {
				textSink = ToUpperCase(c.text)
			}
		})
		b.Run("take/"+c.name, func(b *testing.B) {
			b.ReportAllocs()
			for i := 0; i < b.N; i++ {
				textSink = TakeLeft(int32(10), c.text)
			}
		})
		b.Run("pad/"+c.name, func(b *testing.B) {
			b.ReportAllocs()
			for i := 0; i < b.N; i++ {
				textSink = PadStart(int32(120), " ", c.text)
			}
		})
	}
}

// What JavaScript answers for strings of ASCII, written out.
func TestAStringOfAsciiIsCasedTakenAndPaddedAsInJavaScript(t *testing.T) {
	cases := []struct {
		name      string
		got, want Value
	}{
		{"upper", ToUpperCase("Hello, World 1!"), "HELLO, WORLD 1!"},
		{"upper of controls", ToUpperCase("a\x00b\x7f"), "A\x00B\x7f"},
		{"upper of nothing", ToUpperCase(""), ""},
		{"lower", ToLowerCase("Hello, World 1!"), "hello, world 1!"},
		{"lower of brackets", ToLowerCase("@AZ[`az{"), "@az[`az{"},
		{"take left below zero", TakeLeft(int32(-1), "abc"), ""},
		{"take left zero", TakeLeft(int32(0), "abc"), ""},
		{"take left", TakeLeft(int32(2), "abc"), "ab"},
		{"take left past the end", TakeLeft(int32(5), "abc"), "abc"},
		{"take right below zero", TakeRight(int32(-1), "abc"), ""},
		{"take right zero", TakeRight(int32(0), "abc"), ""},
		{"take right", TakeRight(int32(2), "abc"), "bc"},
		{"take right past the end", TakeRight(int32(5), "abc"), "abc"},
		{"pad start", PadStart(int32(7), "ab", "xyz"), "ababxyz"},
		{"pad start with part of the fill", PadStart(int32(6), "ab", "xyz"), "abaxyz"},
		{"pad start to less", PadStart(int32(2), "ab", "xyz"), "xyz"},
		{"pad start below zero", PadStart(int32(-1), "ab", "xyz"), "xyz"},
		{"pad start with nothing", PadStart(int32(5), "", "xyz"), "xyz"},
		{"pad start of nothing", PadStart(int32(3), "ab", ""), "aba"},
		{"pad end", PadEnd(int32(7), "ab", "xyz"), "xyzabab"},
		{"pad end with part of the fill", PadEnd(int32(6), "ab", "xyz"), "xyzaba"},
		{"pad end to its length", PadEnd(int32(3), "ab", "xyz"), "xyz"},
		{"pad end with nothing", PadEnd(int32(5), "", "xyz"), "xyz"},
	}
	for _, c := range cases {
		if c.got != c.want {
			t.Errorf("%s: %q, want %q", c.name, c.got, c.want)
		}
	}
}
