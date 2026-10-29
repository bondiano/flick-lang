package flickrt

import (
	"bufio"
	"os"
	"strconv"
	"strings"
	"testing"
)

// The corpus is the sign of Node's Intl.Collator("en").compare.
func TestCollationIsIntls(t *testing.T) {
	f, err := os.Open("testdata/collation.txt")
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	scanner := bufio.NewScanner(f)
	for scanner.Scan() {
		if strings.HasPrefix(scanner.Text(), "#") {
			continue
		}
		fields := strings.Split(scanner.Text(), " | ")
		a, b := fromCodePoints(fields[0]), fromCodePoints(fields[1])
		want, _ := strconv.Atoi(fields[2])
		got := collate(a, b)
		if got > 0 {
			got = 1
		} else if got < 0 {
			got = -1
		}
		if got != want {
			t.Errorf("%q vs %q: %d, Intl gives %d", a, b, got, want)
		}
	}
}
