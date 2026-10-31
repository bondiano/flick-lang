package flickrt

import (
	"fmt"
	"math"
	"regexp"
	"strconv"
	"strings"
	"time"
)

// Effect's time zones and the JSON codecs of date-times and zones, as
// Effect Schema's DateTimeUtcFromString, DateTimeZonedFromString,
// TimeZoneFromString and their kin read and write them.

// TimeZone is Effect's: a named IANA zone or a fixed offset.
//
// shortcut: named zones come from the system's time zone database (Go's time.LoadLocation) and match names exactly, where Effect asks Intl, which also resolves names case-insensitively; embed time/tzdata or canonicalize names when a program needs it
type TimeZone struct {
	id     string         // named: the IANA identifier
	loc    *time.Location // named; nil for an offset
	offset float64        // an offset: milliseconds east of UTC
}

func zoneNamed(id string) (*TimeZone, bool) {
	if id == "" || id == "Local" {
		return nil, false
	}
	loc, err := time.LoadLocation(id)
	if err != nil {
		return nil, false
	}
	return &TimeZone{id: id, loc: loc}, true
}

var offsetText = regexp.MustCompile(`([+-])(\d{2}):(\d{2})$`)

// parseOffset is Effect's: `+hh:mm` at the end of `s`, in milliseconds.
func parseOffset(s string) (float64, bool) {
	m := offsetText.FindStringSubmatch(s)
	if m == nil {
		return 0, false
	}
	h, _ := strconv.Atoi(m[2])
	mm, _ := strconv.Atoi(m[3])
	sign := 1.0
	if m[1] == "-" {
		sign = -1
	}
	return sign * float64((h*60+mm)*60*1000), true
}

// zoneFromString is Effect's `zoneFromString`: an offset, or an IANA name.
func zoneFromString(s string) (*TimeZone, bool) {
	if strings.HasPrefix(s, "GMT") || strings.HasPrefix(s, "+") || strings.HasPrefix(s, "-") {
		off, ok := parseOffset(s)
		return &TimeZone{offset: off}, ok
	}
	return zoneNamed(s)
}

// String is Effect's `zoneToString`.
func (z *TimeZone) String() string {
	if z.loc != nil {
		return z.id
	}
	return offsetToString(z.offset)
}

// offsetAt is the zone's offset at `t`, in milliseconds.
func (z *TimeZone) offsetAt(t time.Time) float64 {
	if z.loc == nil {
		return z.offset
	}
	_, seconds := t.In(z.loc).Zone()
	return float64(seconds) * 1000
}

func offsetToString(offset float64) string {
	abs := math.Abs(offset)
	hours := math.Floor(abs / 3600000)
	minutes := math.Round(math.Mod(abs, 3600000) / 60000)
	if minutes == 60 {
		hours, minutes = hours+1, 0
	}
	sign := "+"
	if offset < 0 {
		sign = "-"
	}
	return fmt.Sprintf("%s%02d:%02d", sign, int(hours), int(minutes))
}

// isoString is JS `toISOString`.
func isoString(t time.Time) string { return t.UTC().Format("2006-01-02T15:04:05.000Z") }

// formatIsoZoned is Effect's: the wall clock with its offset, and the
// zone's name unless it is an offset.
func (d *DateTime) formatIsoZoned() string {
	off := d.zone.offsetAt(d.t)
	s := isoString(d.t.Add(time.Duration(off) * time.Millisecond))
	s = s[:len(s)-1] + offsetToString(off)
	if d.zone.loc != nil {
		s += "[" + d.zone.id + "]"
	}
	return s
}

var zonedText = regexp.MustCompile(`^(.{17,35})\[(.+)\]$`)

// makeZonedFromString is Effect's: ISO text with an offset, then maybe the
// zone in brackets; the instant keeps its offset's meaning.
func makeZonedFromString(s string) (*DateTime, bool) {
	iso := s
	var zone *TimeZone
	if m := zonedText.FindStringSubmatch(s); m != nil {
		z, ok := zoneFromString(m[2])
		if !ok {
			return nil, false
		}
		iso, zone = m[1], z
	} else if off, ok := parseOffset(s); ok {
		zone = &TimeZone{offset: off}
	} else {
		return nil, false
	}
	t, ok := parseInstant(iso)
	if !ok {
		return nil, false
	}
	return &DateTime{t: t, zone: zone}, true
}

// maxEpochMillis bounds the instants of JS `Date`.
const maxEpochMillis = 8.64e15

// typed is an encoding that first checks the value is what Effect's declared
// schema `name` accepts.
func typed(name string, ok func(Value) bool, f func(Value) Value) func(*Fiber, Value) (Value, []issue) {
	return func(_ *Fiber, v Value) (Value, []issue) {
		if !ok(v) {
			return nil, expected(name, v)
		}
		return f(v), nil
	}
}

func isUtc(v Value) bool       { d, ok := v.(*DateTime); return ok && d.zone == nil }
func isZoned(v Value) bool     { d, ok := v.(*DateTime); return ok && d.zone != nil }
func isNamedZone(v Value) bool { z, ok := v.(*TimeZone); return ok && z.loc != nil }
func isOffsetZone(v Value) bool {
	z, ok := v.(*TimeZone)
	return ok && z.loc == nil
}

var (
	// CodecDateTimeUtc is `DateTime.json_utc`.
	CodecDateTimeUtc Value = &fallible{&stage{
		&stringAs{"a string that will be decoded as a DateTime.Utc"},
		func(_ *Fiber, v Value) (Value, []issue) {
			if t, ok := parseInstant(v.(string)); ok {
				return &DateTime{t: t}, nil
			}
			return nil, fail("an invalid value")
		},
		typed("DateTime.Utc", isUtc, func(v Value) Value { return isoString(v.(*DateTime).t) }),
	}}

	// CodecDateTimeUtcMillis is `DateTime.json_utc_millis`.
	CodecDateTimeUtcMillis Value = &fallible{&stage{
		withChecks(numberType, isInt()),
		func(_ *Fiber, v Value) (Value, []issue) {
			if x := numberOf(v); math.Abs(x) <= maxEpochMillis {
				return &DateTime{t: time.UnixMilli(int64(x)).UTC()}, nil
			}
			return nil, fail("Invalid DateTime input")
		},
		typed("DateTime.Utc", isUtc, DateTimeToEpochMillis),
	}}

	// CodecDateTimeZoned is `DateTime.json_zoned`.
	CodecDateTimeZoned Value = &fallible{&stage{
		&stringAs{"a zoned DateTime string (e.g. 2024-01-01T00:00:00.000+00:00[Europe/London])"},
		func(_ *Fiber, v Value) (Value, []issue) {
			if d, ok := makeZonedFromString(v.(string)); ok {
				return d, nil
			}
			return nil, fail("an invalid value")
		},
		typed("DateTime.Zoned", isZoned, func(v Value) Value { return v.(*DateTime).formatIsoZoned() }),
	}}

	// CodecTimeZone is `DateTime.json_zone`: an IANA name or an offset.
	CodecTimeZone Value = &fallible{&stage{
		&stringAs{"a time zone string (IANA identifier or offset like +03:00)"},
		func(_ *Fiber, v Value) (Value, []issue) {
			if z, ok := zoneFromString(v.(string)); ok {
				return z, nil
			}
			return nil, fail("an invalid value")
		},
		typed("DateTime.TimeZone", func(v Value) bool { _, ok := v.(*TimeZone); return ok },
			func(v Value) Value { return v.(*TimeZone).String() }),
	}}

	// CodecTimeZoneNamed is `DateTime.json_zone_named`.
	CodecTimeZoneNamed Value = &fallible{&stage{
		&stringAs{"an IANA time zone identifier"},
		func(_ *Fiber, v Value) (Value, []issue) {
			if z, ok := zoneNamed(v.(string)); ok {
				return z, nil
			}
			return nil, fail("an invalid value")
		},
		typed("DateTime.TimeZone.Named", isNamedZone, func(v Value) Value { return v.(*TimeZone).id }),
	}}

	// CodecTimeZoneOffset is `DateTime.json_zone_offset`: milliseconds.
	CodecTimeZoneOffset Value = &stage{
		CodecNumberFinite.(codec),
		pure(func(v Value) Value { return &TimeZone{offset: numberOf(v)} }),
		typed("DateTime.TimeZone.Offset", isOffsetZone, func(v Value) Value { return v.(*TimeZone).offset }),
	}
)
