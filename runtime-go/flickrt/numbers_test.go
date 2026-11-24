package flickrt

import (
	"math"
	"math/big"
	"testing"
)

// option is `Some n` as its digits, or "None".
func option(v Value) string {
	if IsNone(v) {
		return "None"
	}
	return UnwrapSome(v).(*big.Int).String()
}

// `BigInt.from_number`, `parse`, `to_string` and `to_number` as the JS
// target has them: exact integers of any size, decimal digits only.
func TestBigIntConversions(t *testing.T) {
	if got := option(BigIntOfFloat(math.Pow(2, 100))); got != "1267650600228229401496703205376" {
		t.Errorf("from_number 2**100 = %v", got)
	}
	for _, x := range []float64{1.5, math.NaN(), math.Inf(1)} {
		if got := option(BigIntOfFloat(x)); got != "None" {
			t.Errorf("from_number %v = %v, want None", x, got)
		}
	}
	for text, want := range map[string]string{"-17": "-17", " +4 ": "4", "0xff": "None", "1e3": "None", "": "None"} {
		if got := option(BigIntParse(text)); got != want {
			t.Errorf("parse %q = %v, want %v", text, got, want)
		}
	}
	if got := BigIntString(big.NewInt(-42)); got != "-42" {
		t.Errorf("to_string = %v", got)
	}
	plusOne := new(big.Int).Add(new(big.Int).Lsh(big.NewInt(1), 53), big.NewInt(1))
	if got := FloatOfBigInt(plusOne); got != 9007199254740992.0 {
		t.Errorf("to_number 2**53+1 = %v, want 2**53", got)
	}
}

func optionInt(t *testing.T, v Value) (int32, bool) {
	t.Helper()
	if IsNone(v) {
		return 0, false
	}
	if !IsSome(v) {
		t.Fatalf("expected Option, got %T (%v)", v, v)
	}
	n, ok := UnwrapSome(v).(int32)
	if !ok {
		t.Fatalf("Some payload had type %T (%v), want int32", UnwrapSome(v), UnwrapSome(v))
	}
	return n, true
}

func TestIntParse(t *testing.T) {
	accepted := []struct {
		input string
		want  int32
	}{
		{"36", 36},
		{"+36", 36},
		{"-36", -36},
		{"0", 0},
		{"+0", 0},
		{"-0", 0},
		{"000000000000000000000000036", 36},
		{" \t\n\v\f\r 36 \r\f\v\n\t ", 36},
		{"\uFEFF36", 36},
		{"36\uFEFF", 36},
		{"\uFEFF36\uFEFF", 36},
		{"\u00A036", 36},
		{"36\u00A0", 36},
		{"\uFEFF36\u00A0", 36},
		{"\u00A0+36\uFEFF", 36},
		{"\uFEFF-0\u00A0", 0},
		{"-2147483648", -2147483648},
		{"2147483647", 2147483647},
		{"+2147483647", 2147483647},
		{"\uFEFF-2147483648\u00A0", -2147483648},
		{"\u00A02147483647\uFEFF", 2147483647},
	}
	for _, tc := range accepted {
		got, ok := optionInt(t, IntParse(tc.input))
		if !ok || got != tc.want {
			t.Errorf("IntParse(%q) = (%d, %v), want Some(%d)", tc.input, got, ok, tc.want)
		}
	}

	// Every ECMAScript WhiteSpace and LineTerminator code point is accepted leading and trailing.
	ecmaSpaces := []rune{
		'\t', '\n', '\v', '\f', '\r', ' ',
		0x00A0, 0x1680,
		0x2000, 0x2001, 0x2002, 0x2003, 0x2004, 0x2005, 0x2006, 0x2007, 0x2008, 0x2009, 0x200A,
		0x2028, 0x2029, 0x202F, 0x205F, 0x3000, 0xFEFF,
	}
	for _, r := range ecmaSpaces {
		s := string(r) + "36" + string(r)
		got, ok := optionInt(t, IntParse(s))
		if !ok || got != 36 {
			t.Errorf("IntParse(U+%04X + 36 + U+%04X) = (%d, %v), want Some(36)", r, r, got, ok)
		}
	}

	rejected := []string{
		"",
		"   ",
		"\t\n\v\f\r",
		"\uFEFF\u00A0",
		"+",
		"-",
		" + ",
		"\uFEFF-\u00A0",
		"++36",
		"--36",
		"+-36",
		"-+36",
		"\u008536",
		"36\u0085",
		"\u008536\u0085",
		"\u180E36",
		"\u200B36",
		"3 6",
		"3\t6",
		"3\n6",
		"3\u00A06",
		"3\uFEFF6",
		"+ 36",
		"- 36",
		"-2147483649",
		"2147483648",
		"+2147483648",
		"999999999999999999999999999999",
		"4.5",
		"36.0",
		"0x24",
		"1e2",
		"1_000",
		"٣٦",
	}
	for _, input := range rejected {
		if got, ok := optionInt(t, IntParse(input)); ok {
			t.Errorf("IntParse(%q) = Some(%d), want None", input, got)
		}
	}
}
