package flickrt

import (
	"bufio"
	"os"
	"strings"
	"testing"
)

func fromCodePoints(field string) string { return runesString(parseRunes(field)) }

// The corpus is what Node's `normalize`, `toUpperCase` and `toLowerCase`
// give on strings of marks, Hangul, Greek sigma and compatibility forms,
// and their locale forms in Greek, Lithuanian, Turkish and Azeri.
func TestUnicodeIsJSs(t *testing.T) {
	f, err := os.Open("testdata/unicode.txt")
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	forms := []string{"NFC", "NFD", "NFKC", "NFKD"}
	scanner := bufio.NewScanner(f)
	for scanner.Scan() {
		if strings.HasPrefix(scanner.Text(), "#") {
			continue
		}
		fields := strings.Split(scanner.Text(), " | ")
		if len(fields) == 4 { // locale | input | upper | lower
			in := fromCodePoints(fields[1])
			if got, want := ToLocaleUpperCase(fields[0], in).(string), fromCodePoints(fields[2]); got != want {
				t.Errorf("%s upper %q = %q, JS gives %q", fields[0], in, got, want)
			}
			if got, want := ToLocaleLowerCase(fields[0], in).(string), fromCodePoints(fields[3]); got != want {
				t.Errorf("%s lower %q = %q, JS gives %q", fields[0], in, got, want)
			}
			continue
		}
		in := fromCodePoints(fields[0])
		for i, form := range forms {
			got := Normalize(NewData(form), in).(string)
			if want := fromCodePoints(fields[i+1]); got != want {
				t.Errorf("%s of %q = %q, JS gives %q", form, in, got, want)
			}
		}
		if got, want := ToUpperCase(in).(string), fromCodePoints(fields[5]); got != want {
			t.Errorf("upper %q = %q, JS gives %q", in, got, want)
		}
		if got, want := ToLowerCase(in).(string), fromCodePoints(fields[6]); got != want {
			t.Errorf("lower %q = %q, JS gives %q", in, got, want)
		}
	}
}
