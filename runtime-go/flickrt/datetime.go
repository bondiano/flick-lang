package flickrt

import (
	"time"
)

// DateTime is an instant of Effect's `DateTime`, as far as the prompts
// need it: made from an ISO string or the clock, and read back as epoch
// milliseconds.
// shortcut: UTC instants only, without zones, parts or formatting; port the
// rest of `DateTime` here when a Go program needs it.
type DateTime struct{ t time.Time }

var dateTimeLayouts = []string{
	time.RFC3339Nano,
	"2006-01-02T15:04:05.999999999",
	"2006-01-02T15:04",
	"2006-01-02 15:04:05.999999999Z07:00",
	"2006-01-02 15:04:05.999999999",
	"2006-01-02",
}

// DateTimeMake parses an ISO date-time; one without an offset is UTC.
func DateTimeMake(s Value) Value {
	for _, layout := range dateTimeLayouts {
		if t, err := time.Parse(layout, s.(string)); err == nil {
			return Ok(&DateTime{t.UTC()})
		}
	}
	return Err(NewData(Tag("DateTime.DateTimeError.DateTimeError"), "not a date-time: "+QuoteJSON(s.(string))))
}

func DateTimeNow() Value { return &DateTime{time.Now().UTC()} }

func DateTimeToEpochMillis(d Value) Value { return float64(d.(*DateTime).t.UnixMilli()) }
