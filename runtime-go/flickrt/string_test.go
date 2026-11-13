package flickrt

import (
	"strconv"
	"strings"
	"sync"
	"testing"
)

func TestStringsCountUTF16CodeUnits(t *testing.T) {
	s := "a\U0001F30Db"
	if Length(s) != int32(4) {
		t.Errorf("length %v", Length(s))
	}
	high := Slice(int32(1), int32(2), s).(string)
	low := Slice(int32(2), int32(3), s).(string)
	if Length(high) != int32(1) || Length(low) != int32(1) {
		t.Error("a half of a pair is one code unit")
	}
	if Concat(high, low) != "\U0001F30D" {
		t.Errorf("the halves join into the pair: %q", Concat(high, low))
	}
	if Concat(Concat("a", high), Concat(low, "b")) != s {
		t.Error("joining back gives the string")
	}
	if QuoteJSON(high) != `"\ud83c"` {
		t.Errorf("a lone surrogate is escaped: %s", QuoteJSON(high))
	}
	if wellFormed(high+"x") != "�x" {
		t.Error("output replaces a lone surrogate")
	}
	if CodePointAt(int32(1), s).(*Data).Fields[0] != int32(0x1F30D) {
		t.Error("code point at a pair")
	}
	if CharCodeAt(int32(2), s).(*Data).Fields[0] != int32(0xDF0D) {
		t.Error("code unit inside a pair")
	}
	if IndexOf("b", s).(*Data).Fields[0] != int32(3) {
		t.Error("index counts code units")
	}
	if Substring(int32(3), int32(-1), s) != "a\U0001F30D" {
		t.Error("substring swaps its bounds and reads negative as 0")
	}
	if len(Chars(s).([]Value)) != 3 {
		t.Error("chars are code points")
	}
}

func TestStringsOrderByCodeUnits(t *testing.T) {
	// U+FF61 is above the high surrogate of U+1F600 in UTF-16, below it in UTF-8
	if CompareStrings("｡", "\U0001F600") <= 0 {
		t.Error("UTF-16 order")
	}
	if CompareStrings("ab", "abc") >= 0 || CompareStrings("b", "abc") <= 0 || CompareStrings("x", "x") != 0 {
		t.Error("prefixes and plain order")
	}
}

// Strings made from one string, in turn or at once, keep their own bytes
// while the buffer they share grows.
func TestConcatGrowsWithoutAliasing(t *testing.T) {
	// The second `++` makes a buffer with room to grow.
	base := Concat(Concat(strings.Repeat("x", growFrom), "!"), "!").(string)
	a := Concat(base, "a").(string)
	b := Concat(base, "b").(string)
	aa := Concat(a, "a").(string)
	if base != strings.Repeat("x", growFrom)+"!!" || a != base+"a" || b != base+"b" || aa != a+"a" {
		t.Fatalf("branches share bytes: %q %q %q", a[len(base):], b[len(base):], aa[len(base):])
	}
	high := Slice(int32(0), int32(1), "\U0001F30D").(string)
	low := Slice(int32(1), int32(2), "\U0001F30D").(string)
	if got := Concat(Concat(base, high), low+"!"); got != base+"\U0001F30D!" || Length(got) != int32(len(base)+3) {
		t.Errorf("a pair split across a grown string: %q", got)
	}
	var wg sync.WaitGroup
	results := make([]string, 16)
	for i := range results {
		wg.Add(1)
		go func() {
			defer wg.Done()
			s := base
			for j := 0; j < 200; j++ {
				s = Concat(s, strconv.Itoa(i)+";").(string)
			}
			results[i] = s
		}()
	}
	wg.Wait()
	for i, s := range results {
		if s != base+strings.Repeat(strconv.Itoa(i)+";", 200) {
			t.Errorf("goroutine %d built %q", i, s[len(base):])
		}
	}
}

// `acc ++ piece` costs the piece: compare ns/op at 2 000 and 20 000 pieces.
func BenchmarkConcatAccumulates(b *testing.B) {
	for _, n := range []int{2000, 20000} {
		b.Run(strconv.Itoa(n), func(b *testing.B) {
			for range b.N {
				var acc Value = ""
				for i := range n {
					acc = Concat(acc, Concat(strconv.Itoa(i), ";"))
				}
			}
		})
	}
}
