package flickrt

import "testing"

// Expectations read from Node 24 with effect@4.0.5.

func TestParseJSDate(t *testing.T) {
	for in, want := range map[string]string{
		"2024-03-10T24:00:00Z":     "2024-03-11T00:00:00.000Z",
		"2024-02-30T00:00:00Z":     "2024-03-01T00:00:00.000Z",
		"-000001-01-01T00:00:00Z":  "-000001-01-01T00:00:00.000Z",
		"2024-03-10T08:30:00+0100": "2024-03-10T07:30:00.000Z",
		"2024-03-10T08:30:00.1Z":   "2024-03-10T08:30:00.100Z",
	} {
		if got := (&date{parseJSDate(in)}).isoString(); got != want {
			t.Errorf("%s: %s, want %s", in, got, want)
		}
	}
	for _, in := range []string{"2024-13-01Z", "2024-03-10T08Z", "-000000-01-01T00:00:00Z", "2024-03-10T24:00:01Z"} {
		if x := parseJSDate(in); x == x {
			t.Errorf("%s: %v, want NaN", in, x)
		}
	}
}

func TestCronAcrossDaylightSaving(t *testing.T) {
	paris := zoneNamed("Europe/Paris")
	c, bad := parseCron("0 * * * *", paris)
	if c == nil {
		t.Fatal(bad)
	}
	at := fromInput("2024-03-31T00:59:00Z")
	var got []string
	for range 4 {
		at = c.step(at, false)
		got = append(got, DateTimeFormatIso(at).(string))
	}
	want := []string{"2024-03-31T01:00:00.000Z", "2024-03-31T02:00:00.000Z", "2024-03-31T03:00:00.000Z", "2024-03-31T04:00:00.000Z"}
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("%v, want %v", got, want)
		}
	}
	if prev := c.step(fromInput("2024-10-27T01:30:00Z"), true); DateTimeFormatIso(prev) != "2024-10-27T00:00:00.000Z" {
		t.Errorf("prev: %v", DateTimeFormatIso(prev))
	}
	if r := DateTimeSetZone(Rec("adjustForTimeZone", Some(true), "disambiguation", Some(NewData(Tag("DateTime.Disambiguation.Reject")))), paris, fromInput("2024-03-31T02:30:00Z")); isOk(r) {
		t.Errorf("a skipped hour is placed: %v", DateTimeFormatIsoOffset(value(r)))
	}
}
