package flickrt

import (
	"bufio"
	"math"
	"math/big"
	"os"
	"strconv"
	"strings"
	"testing"
)

// The corpus is JS `String(x)` of each float, as Node wrote it; the
// compiler's constant folding reads the same file.
func TestShowNumberWritesNumbersAsJS(t *testing.T) {
	f, err := os.Open("testdata/numbers.txt")
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	scanner := bufio.NewScanner(f)
	n := 0
	for scanner.Scan() {
		line := scanner.Text()
		if strings.HasPrefix(line, "#") {
			continue
		}
		bits, want, _ := strings.Cut(line, " ")
		u, err := strconv.ParseUint(bits, 16, 64)
		if err != nil {
			t.Fatal(err)
		}
		x := math.Float64frombits(u)
		if got := ShowNumber(x); got != want {
			t.Errorf("ShowNumber(%v [%s]) = %q, JS writes %q", x, bits, got, want)
		}
		n++
	}
	if n < 300 {
		t.Fatalf("only %d cases", n)
	}
}

func TestIntArithmeticWrapsAndDividesByZeroToZero(t *testing.T) {
	cases := []struct {
		got, want Value
	}{
		{AddI(int32(math.MaxInt32), int32(1)), int32(math.MinInt32)},
		{MulI(int32(65536), int32(65536)), int32(0)},
		{DivI(int32(7), int32(0)), int32(0)},
		{RemI(int32(7), int32(0)), int32(0)},
		{DivI(int32(math.MinInt32), int32(-1)), int32(math.MinInt32)},
		{RemI(int32(-7), int32(2)), int32(-1)},
		{PowI(int32(2), int32(31)), int32(math.MinInt32)},
		{PowI(int32(2), int32(-1)), int32(0)},
		{PowI(int32(-1), int32(-3)), int32(-1)},
		{IntTruncate(3e9), int32(-1294967296)},
		{IntTruncate(math.NaN()), int32(0)},
		{ShiftRightUnsignedInt(int32(-1), int32(28)), int32(15)},
	}
	for i, c := range cases {
		if c.got != c.want {
			t.Errorf("case %d: %v, want %v", i, c.got, c.want)
		}
	}
}

func TestFloatsFollowJS(t *testing.T) {
	if !Equals(math.NaN(), math.NaN()) || !Equals(0.0, math.Copysign(0, -1)) {
		t.Error("== is SameValueZero")
	}
	if Lt(math.NaN(), 1.0) || Ge(math.NaN(), math.NaN()) {
		t.Error("a comparison with NaN is false")
	}
	if Compare(math.NaN(), math.Inf(1)) != GT || Compare(math.Copysign(0, -1), 0.0) != EQ {
		t.Error("Ord puts NaN last and signed zeros together")
	}
	if !math.IsNaN(PowF(1.0, math.NaN()).(float64)) || !math.IsNaN(PowF(-1.0, math.Inf(1)).(float64)) {
		t.Error("Math.pow of ±1 to NaN or ±Infinity is NaN")
	}
	if RemF(-5.5, 2.0) != -1.5 {
		t.Error("% keeps the sign of the dividend")
	}
	for x, want := range map[float64]float64{2.5: 3, -2.5: -2, 0.49999999999999994: 0, -0.4: math.Copysign(0, -1)} {
		if got := FloatRound(x).(float64); got != want || math.Signbit(got) != math.Signbit(want) {
			t.Errorf("Math.round(%v) = %v, want %v", x, got, want)
		}
	}
}

func TestBigIntsDivideTowardsZero(t *testing.T) {
	b := func(s string) Value { return BigLit(s) }
	if Show(DivB(b("-7"), b("2"))) != "-3n" || Show(RemB(b("-7"), b("2"))) != "-1n" {
		t.Error("BigInt / and % truncate")
	}
	if DivB(b("5"), b("0")).(*big.Int).Sign() != 0 {
		t.Error("BigInt division by zero is 0")
	}
	if Show(MulB(b("0xffffffffffffffff"), b("0x10"))) != "295147905179352825840n" {
		t.Error("BigInt is unbounded")
	}
}
