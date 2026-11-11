package flickrt

import (
	"os"
	"strconv"
	"strings"
	"testing"
)

// TestGraphemesAndWidthsAsNode checks graphemes and cliDisplayWidth
// against Intl.Segmenter and the displayWidth of the Effect library on the
// corpus that tools/go-unicode/graphemes.mjs writes.
func TestGraphemesAndWidthsAsNode(t *testing.T) {
	data, err := os.ReadFile("testdata/graphemes.txt")
	if err != nil {
		t.Fatal(err)
	}
	bad := 0
	for _, line := range strings.Split(strings.TrimSpace(string(data)), "\n")[1:] {
		segmented, width, _ := strings.Cut(line, " | ")
		var want []string
		for _, s := range strings.Split(segmented, " / ") {
			want = append(want, string(parseRunes(s)))
		}
		text := strings.Join(want, "")
		got := graphemes(text)
		w, _ := strconv.Atoi(width)
		if strings.Join(got, "/") != strings.Join(want, "/") || cliDisplayWidth(text) != w {
			if bad++; bad <= 20 {
				t.Errorf("%U: %q, width %d; Node %q, width %d", []rune(text), got, cliDisplayWidth(text), want, w)
			}
		}
	}
}
