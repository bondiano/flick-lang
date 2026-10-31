package flickrt

import (
	"time"
)

// DateTime is an instant of Effect's `DateTime`, as far as the prompts and
// the JSON codecs need it: made from an ISO string or the clock, read back
// as epoch milliseconds, and zoned when decoded with a zone.
// shortcut: no parts, arithmetic or formatting beyond ISO; port the rest of
// `DateTime` here when a Go program needs it.
type DateTime struct {
	t    time.Time
	zone *TimeZone // nil: UTC
}

var dateTimeLayouts = []string{
	time.RFC3339Nano,
	"2006-01-02T15:04:05.999999999",
	"2006-01-02T15:04",
	"2006-01-02 15:04:05.999999999Z07:00",
	"2006-01-02 15:04:05.999999999",
	"2006-01-02",
}

// parseInstant reads an ISO date-time; one without an offset is UTC.
func parseInstant(s string) (time.Time, bool) {
	for _, layout := range dateTimeLayouts {
		if t, err := time.Parse(layout, s); err == nil {
			return t.UTC(), true
		}
	}
	return time.Time{}, false
}

// DateTimeMake parses an ISO date-time; one without an offset is UTC.
func DateTimeMake(s Value) Value {
	if t, ok := parseInstant(s.(string)); ok {
		return Ok(&DateTime{t: t})
	}
	return Err(NewData(Tag("DateTime.DateTimeError.DateTimeError"), "not a date-time: "+QuoteJSON(s.(string))))
}

func DateTimeNow() Value { return &DateTime{t: time.Now().UTC()} }

func DateTimeToEpochMillis(d Value) Value { return float64(d.(*DateTime).t.UnixMilli()) }
