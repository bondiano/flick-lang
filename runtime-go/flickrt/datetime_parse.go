package flickrt

import (
	"math"
	"regexp"
	"time"
)

// isoDate is the date-time string format of ECMA-262 as V8 reads it: a
// space or a lower-case `t` before the time, any number of fraction
// digits, `24:00` as the end of the day, and an offset with or without
// its colon. A time without an offset is local time.
var isoDate = regexp.MustCompile(`^([+-]\d{6}|\d{4})(?:-(\d{2})(?:-(\d{2}))?)?(?:[Tt ](\d{2}):(\d{2})(?::(\d{2})(?:\.(\d+))?)?)?([Zz]|[+-]\d{2}:?\d{2})?$`)

// parseJSDate is `new Date(s).getTime()`, NaN for a string it refuses.
//
// shortcut: V8's legacy fallback formats ("March 10, 2024", "2024/03/10", "2024-3-10") are refused; read them if programs need them
func parseJSDate(s string) float64 {
	m := isoDate.FindStringSubmatch(s)
	if m == nil || m[1] == "-000000" {
		return math.NaN()
	}
	num := func(s string, absent float64) float64 {
		if s == "" {
			return absent
		}
		if s[0] == '+' || s[0] == '-' {
			v := float64(atoi(s[1:]))
			if s[0] == '-' {
				v = -v
			}
			return v
		}
		return float64(atoi(s))
	}
	year, month, day := num(m[1], 0), num(m[2], 1), num(m[3], 1)
	hour, minute, second := num(m[4], 0), num(m[5], 0), num(m[6], 0)
	ms := 0.0
	if f := m[7]; f != "" {
		for len(f) < 3 {
			f += "0"
		}
		ms = float64(atoi(f[:3]))
	}
	if month < 1 || month > 12 || day < 1 || day > 31 || minute > 59 || second > 59 ||
		hour > 24 || hour == 24 && (minute != 0 || second != 0 || ms != 0) {
		return math.NaN()
	}
	t := makeDate(makeDay(year, month-1, day), makeTime(hour, minute, second, ms))
	switch zone := m[8]; {
	case zone == "Z" || zone == "z":
	case zone != "":
		h, mm := float64(atoi(zone[1:3])), float64(atoi(zone[len(zone)-2:]))
		if h > 23 || mm > 59 {
			return math.NaN()
		}
		off := (h*60 + mm) * 60000
		if zone[0] == '-' {
			off = -off
		}
		t -= off
	case m[4] != "":
		// a date-time without an offset is local time; a date alone is UTC
		local := time.UnixMilli(int64(timeClip(t))).UTC()
		wall := time.Date(local.Year(), local.Month(), local.Day(), local.Hour(), local.Minute(), local.Second(), local.Nanosecond(), time.Local)
		t = float64(wall.UnixMilli())
	}
	return timeClip(t)
}
