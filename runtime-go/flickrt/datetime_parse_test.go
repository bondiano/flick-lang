package flickrt

import (
	"math"
	"os"
	"strconv"
	"strings"
	"testing"
	"time"
)

// testdata/dates.txt is Node's `Date.parse` of each input, in UTC and in
// America/New_York (tools/go-dates/gen.mjs).
func TestParseJsDateAsV8(t *testing.T) {
	data, err := os.ReadFile("testdata/dates.txt")
	if err != nil {
		t.Fatal(err)
	}
	zones := []string{"UTC", "America/New_York"}
	defer func(local *time.Location) { time.Local = local }(time.Local)
	for _, line := range strings.Split(strings.TrimSpace(string(data)), "\n") {
		fields := strings.Split(line, "\t")
		input := value(JsonParse(fields[0])).(string)
		for i, zone := range zones {
			loc, err := time.LoadLocation(zone)
			if err != nil {
				t.Fatal(err)
			}
			time.Local = loc
			got := "NaN"
			if ms := parseJsDate(input); !math.IsNaN(ms) {
				got = strconv.FormatFloat(ms+0, 'f', -1, 64)
			}
			if got != fields[i+1] {
				t.Errorf("%s in %s: got %s, want %s", fields[0], zone, got, fields[i+1])
			}
		}
	}
}
