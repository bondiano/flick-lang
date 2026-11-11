package flickrt

import (
	"math"
	"regexp"
	"strings"
	"time"
)

// DateTime is an instant as JS keeps it, epoch milliseconds that may be
// fractional or NaN, in UTC or zoned. Its arithmetic goes through JS `Date`
// semantics (jsDate).
type DateTime struct {
	ms   float64
	zone *TimeZone // nil: UTC
}

func dateTime(v Value) *DateTime { return v.(*DateTime) }

// time is the instant as a Go time, to the millisecond.
func (d *DateTime) time() time.Time { return time.UnixMilli(int64(d.ms)).UTC() }

// toDate is a `Date` whose UTC parts are the wall clock.
func (d *DateTime) toDate() *jsDate {
	switch {
	case d.zone == nil:
		return newJsDate(d.ms)
	case d.zone.loc == nil:
		return newJsDate(d.ms + d.zone.offset)
	}
	return newJsDate(timeClip(d.ms) + d.zone.namedOffset(d.ms))
}

// zonedOffset is the wall clock's distance from UTC.
func (d *DateTime) zonedOffset() float64 { return d.toDate().t - d.ms }

// with is a date-time of the same kind and zone at `ms`.
func (d *DateTime) with(ms float64) *DateTime { return &DateTime{ms, d.zone} }

// mutate changes the wall clock by `f`, which the zone then places, a
// skipped or repeated time as `compatible` does.
func (d *DateTime) mutate(f func(*jsDate)) *DateTime {
	date := d.toDate()
	f(date)
	if d.zone == nil {
		return &DateTime{ms: date.t}
	}
	return makeZonedFromAdjusted(date.t, d.zone, "compatible")
}

// mutateUtc changes the instant's UTC parts by `f`.
func (d *DateTime) mutateUtc(f func(*jsDate)) *DateTime {
	date := newJsDate(d.ms)
	f(date)
	return d.with(date.t)
}

var dateTimeErrorTag = Tag("DateTime.DateTimeError.DateTimeError")

func dateTimeError(message string) Value { return Err(NewData(dateTimeErrorTag, message)) }

// checkedDateTime is the adapter's: the date-time, or the error when there
// is none or it is invalid.
func checkedDateTime(what string, d *DateTime, ok bool) Value {
	if !ok || !finite(d.ms) {
		return dateTimeError(what)
	}
	return Ok(d)
}

// attempt is the adapter's: `f()`, or the error of the RangeError it throws.
func attempt(what string, f func() Value) Value {
	out, message, failed := catchRange(f)
	if failed {
		return dateTimeError(what + ": " + message)
	}
	return out
}

var hasZone = regexp.MustCompile(`Z|GMT|[+-]\d{2}$|[+-]\d{2}:?\d{2}$|\]$`)

// parseInstant is the instant of a string: one without a zone is read as
// UTC.
func parseInstant(s string) (*DateTime, bool) {
	if !hasZone.MatchString(s) {
		s += "Z"
	}
	ms := parseJsDate(s)
	return &DateTime{ms: ms}, !math.IsNaN(ms)
}

func DateTimeMake(s Value) Value {
	d, ok := parseInstant(s.(string))
	return checkedDateTime("not a date-time: "+QuoteJSON(s.(string)), d, ok)
}

func DateTimeFromParts(parts Value) Value {
	date := newJsDate(0)
	setParts(date, parts)
	return checkedDateTime("invalid date parts", &DateTime{ms: date.t}, !math.IsNaN(date.t))
}

func DateTimeFromEpochSeconds(n Value) Value {
	ms := timeClip(n.(float64) * 1000)
	return checkedDateTime("epoch seconds out of range: "+formatNumber(n.(float64)), &DateTime{ms: ms}, !math.IsNaN(ms))
}

const (
	minEpochMillis = -8640000000000000 + 12*60*60*1000
	maxEpochMillis = 8640000000000000 - 14*60*60*1000
)

// makeZoned is a date-time in a zone: the same instant, or with `adjust`
// the one whose wall clock in the zone is `d` read as UTC.
func makeZoned(d *DateTime, zone *TimeZone, adjust bool, disambiguation string) *DateTime {
	if d.ms < minEpochMillis || d.ms > maxEpochMillis {
		panic(rangeError("Epoch millis out of range: " + formatNumber(d.ms)))
	}
	if !adjust {
		return &DateTime{d.ms, zone}
	}
	return makeZonedFromAdjusted(d.ms, zone, disambiguation)
}

// zoneOptions reads the adapter's ZoneOptions.
func zoneOptions(o Value) (adjust bool, disambiguation string) {
	r := record(o)
	if a := r.Get("adjustForTimeZone"); isSome(a) {
		adjust = value(a).(bool)
	}
	disambiguation = "compatible"
	if d := r.Get("disambiguation"); isSome(d) {
		disambiguation = strings.ToLower(ConstructorName(value(d).(*Data).Tag))
	}
	return
}

func DateTimeMakeZoned(o, zone, self Value) Value {
	adjust, disambiguation := zoneOptions(o)
	d, _, failed := catchRange(func() *DateTime { return makeZoned(dateTime(self), zone.(*TimeZone), adjust, disambiguation) })
	return checkedDateTime("cannot place the date-time in the zone", d, !failed)
}

func DateTimeMakeZonedFromString(s Value) Value {
	d, ok := makeZonedFromString(s.(string))
	return checkedDateTime("not a zoned date-time: "+QuoteJSON(s.(string)), d, ok)
}

var zonedText = regexp.MustCompile(`^(.{17,35})\[(.+)\]$`)

// makeZonedFromString reads ISO text with an offset, then maybe the zone
// in brackets; the instant keeps its offset's meaning.
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
	d, ok := parseInstant(iso)
	if !ok {
		return nil, false
	}
	z, _, failed := catchRange(func() *DateTime { return makeZoned(d, zone, false, "") })
	return z, !failed
}

func DateTimeNow(fb *Fiber) Value { return &DateTime{ms: clockOf(fb).millis()} }

func DateTimeIsUtc(d Value) Value   { return dateTime(d).zone == nil }
func DateTimeIsZoned(d Value) Value { return dateTime(d).zone != nil }

func DateTimeMatch(fb *Fiber, options, self Value) Value {
	if dateTime(self).zone == nil {
		return Apply(fb, record(options).Get("onUtc"), self)
	}
	return Apply(fb, record(options).Get("onZoned"), self)
}

func DateTimeToUtc(d Value) Value { return &DateTime{ms: dateTime(d).ms} }

// setZone is `d` in `zone`: the same instant, or, when the options adjust,
// the date-time whose wall clock in the zone is `d` read as UTC.
func setZone(d *DateTime, zone *TimeZone, o Value) Value {
	adjust, disambiguation := zoneOptions(o)
	if adjust {
		return makeZonedFromAdjusted(d.ms, zone, disambiguation)
	}
	return &DateTime{d.ms, zone}
}

func DateTimeSetZone(o, zone, self Value) Value {
	return attempt("set_zone", func() Value { return Ok(setZone(dateTime(self), zone.(*TimeZone), o)) })
}

func DateTimeSetZoneOffset(o, offset, self Value) Value {
	return attempt("set_zone_offset", func() Value {
		return Ok(setZone(dateTime(self), &TimeZone{offset: offset.(float64)}, o))
	})
}

func DateTimeSetZoneNamed(o, id, self Value) Value {
	return attempt("set_zone_named", func() Value {
		zone, ok := zoneNamed(id.(string))
		if !ok {
			return dateTimeError("invalid time zone: " + QuoteJSON(id.(string)))
		}
		return Ok(setZone(dateTime(self), zone, o))
	})
}

func DateTimeZoneMakeOffset(offset Value) Value { return &TimeZone{offset: offset.(float64)} }

func DateTimeZoneMakeNamed(id Value) Value {
	if z, ok := zoneNamed(id.(string)); ok {
		return Ok(z)
	}
	return dateTimeError("invalid time zone: " + QuoteJSON(id.(string)))
}

func DateTimeZoneMakeLocal() Value { return localZone() }

func DateTimeZoneFromString(s Value) Value {
	if z, ok := zoneFromString(s.(string)); ok {
		return Ok(z)
	}
	return dateTimeError("invalid time zone: " + QuoteJSON(s.(string)))
}

func DateTimeZoneToString(z Value) Value { return z.(*TimeZone).String() }
