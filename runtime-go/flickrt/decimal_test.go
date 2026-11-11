package flickrt

import (
	"bufio"
	"os"
	"strconv"
	"strings"
	"testing"
)

var modeNames = []string{"Ceiling", "Floor", "TowardZero", "AwayFromZero", "HalfCeiling", "HalfFloor", "HalfTowardZero", "HalfAwayFromZero", "HalfEven", "HalfOdd"}

func optionText(o Value, f func(Value) Value) string {
	if d := o.(*Data); d.Tag == "Some" {
		return f(d.Fields()[0]).(string)
	}
	return "none"
}

// The corpus is what the BigDecimal and Number of the Effect library
// compute in Node.
func TestDecimalsAreEffects(t *testing.T) {
	f, err := os.Open("testdata/decimals.txt")
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	scanner := bufio.NewScanner(f)
	for scanner.Scan() {
		line := scanner.Text()
		if strings.HasPrefix(line, "#") {
			continue
		}
		fields := strings.Fields(line)
		if fields[0] == "N" {
			a, _ := strconv.ParseFloat(fields[1], 64)
			b, _ := strconv.ParseFloat(fields[2], 64)
			if got := ShowNumber(NumberRemainder(b, a).(float64)); got != fields[3] {
				t.Errorf("Number.remainder %s %s = %s, Effect gives %s", fields[1], fields[2], got, fields[3])
			}
			continue
		}
		a, _ := parseDecimal(fields[0])
		b, _ := parseDecimal(fields[1])
		got := []string{
			DecimalToString(a).(string),
			optionText(DecimalDivide(b, a), DecimalToString),
			optionText(DecimalRemainder(b, a), DecimalToString),
		}
		for _, m := range modeNames {
			got = append(got, DecimalToString(decRound(a, 1, m)).(string))
		}
		if strings.Join(got, " ") != strings.Join(fields[2:], " ") {
			t.Errorf("%s %s:\n  got    %v\n  Effect %v", fields[0], fields[1], got, fields[2:])
		}
	}
}
