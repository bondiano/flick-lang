package flickrt

import (
	"math"
	"math/big"
	"testing"
)

// option is `Some n` as its digits, or "None".
func option(v Value) string {
	d := v.(*Data)
	if d.Tag == "None" {
		return "None"
	}
	return d.Fields()[0].(*big.Int).String()
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
