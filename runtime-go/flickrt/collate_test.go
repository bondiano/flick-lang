package flickrt

import (
	"bufio"
	"os"
	"strconv"
	"strings"
	"testing"
)

// The corpus is the sign of Node's Intl.Collator(locale).compare, on pairs
// tools/go-collate/gen.mjs picks for each collation.
func TestCollationIsIntls(t *testing.T) {
	f, err := os.Open("testdata/collation.txt")
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	scanner := bufio.NewScanner(f)
	bad := 0
	for scanner.Scan() {
		if strings.HasPrefix(scanner.Text(), "#") {
			continue
		}
		fields := strings.Split(scanner.Text(), " | ")
		a, b := fromCodePoints(fields[1]), fromCodePoints(fields[2])
		want, _ := strconv.Atoi(fields[3])
		if got := collatorOf(parseLocale(fields[0])).compare(a, b); got != want {
			if bad++; bad <= 30 {
				t.Errorf("%s: %q %U vs %q %U: %d, Intl gives %d", fields[0], a, []rune(a), b, []rune(b), got, want)
			}
		}
	}
	if bad > 0 {
		t.Errorf("%d pairs differ", bad)
	}
}
