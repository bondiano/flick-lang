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
