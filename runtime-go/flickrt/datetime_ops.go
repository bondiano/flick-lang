package flickrt

import (
	"math"
	"regexp"
	"strings"
)

// The `DateTime` module, as the JS runtime's adapters over Effect's:
// options records with `Option` fields, unit constructors and
// `DateTimeError` instead of `None` or a throw.

var dateTimeErrorTag = Tag("DateTime.DateTimeError.DateTimeError")

func dateTimeError(message string) Value { return Err(NewData(dateTimeErrorTag, message)) }

// checked is the date-time, or the error `what` for an invalid one.
func checkedDate(what string, t *DateTime) Value {
	if t == nil || !finite(t.millis) {
		return dateTimeError(what)
	}
	return Ok(t)
}

// fromInput is Effect's makeUnsafe of a string: one without a zone is UTC.
func fromInput(s string) *DateTime {
	if !hasZone.MatchString(s) {
		s += "Z"
	}
	return utcAt(parseJSDate(s))
}

var hasZone = regexp.MustCompile(`Z|GMT|[+-]\d{2}$|[+-]\d{2}:?\d{2}$|\]$`)

const minEpochMillis, maxEpochMillis = -8640000000000000 + 12*3600000, 8640000000000000 - 14*3600000

// makeZoned is Effect's makeZonedUnsafe of a valid date-time; nil when it
// throws.
func makeZoned(t *DateTime, zone *TimeZone, adjust bool, disambiguation string) *DateTime {
	if !finite(t.millis) || t.millis < minEpochMillis || t.millis > maxEpochMillis || zone == nil {
		return nil
	}
	if !adjust {
		return &DateTime{millis: t.millis, zone: zone}
	}
	out, _ := fromAdjusted(t.millis, zone, disambiguation)
	return out
}

func DateTimeMake(s Value) Value {
	return checkedDate("not a date-time: "+QuoteJSON(s.(string)), fromInput(s.(string)))
}

func optionalPart(parts *Record, key string) (float64, bool) {
	o := parts.Get(key)
	if !isSome(o) {
		return 0, false
	}
	return float64(value(o).(int32)), true
}

// setParts is Effect's setPartsDate.
func setParts(d *date, parts Value) {
	p := record(parts)
	year, hasYear := optionalPart(p, "year")
	month, hasMonth := optionalPart(p, "month")
	day, hasDay := optionalPart(p, "day")
	if hasYear || hasMonth || hasDay {
		if !hasYear {
			year = d.year()
		}
		if hasMonth {
			month--
		} else {
			month = d.month()
		}
		if !hasDay {
			day = d.day()
		}
		d.setFullYear(year, month, day)
	}
	if w, ok := optionalPart(p, "weekDay"); ok {
		d.setDate(d.day() + w - d.weekDay())
	}
	if h, ok := optionalPart(p, "hour"); ok {
		d.setHours(h, d.minutes(), d.seconds(), d.milliseconds())
	}
	if m, ok := optionalPart(p, "minute"); ok {
		d.setMinutes(m, d.seconds(), d.milliseconds())
	}
	if s, ok := optionalPart(p, "second"); ok {
		d.setSeconds(s, d.milliseconds())
	}
	if ms, ok := optionalPart(p, "millisecond"); ok {
		d.setMillis(ms)
	}
}

func DateTimeFromParts(p Value) Value {
	d := &date{}
	setParts(d, p)
	return checkedDate("invalid date parts", utcAt(d.t))
}

func DateTimeFromEpochSeconds(n Value) Value {
	x := n.(float64)
	return checkedDate("epoch seconds out of range: "+ShowNumber(x), utcAt(timeClip(x*1000)))
}

func zoneOptions(o Value) (bool, string) {
	r := record(o)
	adjust := r.Get("adjustForTimeZone")
	disambiguation := "compatible"
	if d := r.Get("disambiguation"); isSome(d) {
		disambiguation = strings.ToLower(ConstructorName(value(d).(*Data).Tag))
	}
	return isSome(adjust) && value(adjust).(bool), disambiguation
}

func DateTimeMakeZoned(o, zone, self Value) Value {
	adjust, d := zoneOptions(o)
	return checkedDate("cannot place the date-time in the zone", makeZoned(dt(self), tz(zone), adjust, d))
}

func DateTimeMakeZonedFromString(s Value) Value {
	in := s.(string)
	what := "not a zoned date-time: " + QuoteJSON(in)
	if m := zonedString.FindStringSubmatch(in); m != nil && utf16Len(m[1]) >= 17 && utf16Len(m[1]) <= 35 {
		return checkedDate(what, makeZoned(fromInput(m[1]), zoneFromString(m[2]), false, ""))
	}
	if o, ok := parseOffset(in); ok {
		return checkedDate(what, makeZoned(fromInput(in), zoneOffset(o), false, ""))
	}
	return dateTimeError(what)
}

var zonedString = regexp.MustCompile(`^(.+)\[(.+)\]$`)

func utf16Len(s string) int { return len(utf16Units(s)) }

func DateTimeNow(fb *Fiber) Value { return utcAt(clockOf(fb).millis()) }

func DateTimeIsUtc(t Value) Value   { return dt(t).zone == nil }
func DateTimeIsZoned(t Value) Value { return dt(t).zone != nil }

func DateTimeMatch(options, t Value) Value {
	if dt(t).zone == nil {
		return Apply(record(options).Get("onUtc"), t)
	}
	return Apply(record(options).Get("onZoned"), t)
}

func DateTimeToUtc(t Value) Value { return utcAt(dt(t).millis) }

// setZone is Effect's: the instant kept, or with `adjust` the wall clock.
func setZone(o Value, self *DateTime, zone *TimeZone, what string) Value {
	adjust, d := zoneOptions(o)
	if !adjust {
		return Ok(&DateTime{millis: self.millis, zone: zone})
	}
	out, reason := fromAdjusted(self.millis, zone, d)
	if out == nil {
		return dateTimeError(what + ": " + reason)
	}
	return Ok(out)
}

func DateTimeSetZone(o, zone, self Value) Value { return setZone(o, dt(self), tz(zone), "set_zone") }

func DateTimeSetZoneOffset(o, offset, self Value) Value {
	return setZone(o, dt(self), zoneOffset(offset.(float64)), "set_zone_offset")
}

func DateTimeSetZoneNamed(o, id, self Value) Value {
	z := zoneNamed(id.(string))
	if z == nil {
		return dateTimeError("invalid time zone: " + QuoteJSON(id.(string)))
	}
	return setZone(o, dt(self), z, "set_zone_named")
}

func DateTimeZoneMakeOffset(offset Value) Value { return zoneOffset(offset.(float64)) }

func zoneResult(z *TimeZone, id string) Value {
	if z == nil {
		return dateTimeError("invalid time zone: " + QuoteJSON(id))
	}
	return Ok(z)
}

func DateTimeZoneMakeNamed(id Value) Value { return zoneResult(zoneNamed(id.(string)), id.(string)) }
func DateTimeZoneMakeLocal() Value         { return localZone() }
func DateTimeZoneFromString(s Value) Value { return zoneResult(zoneFromString(s.(string)), s.(string)) }
func DateTimeZoneToString(z Value) Value   { return tz(z).String() }
func DateTimeToEpochMillis(t Value) Value  { return dt(t).millis }
func DateTimeToEpochSeconds(t Value) Value { return math.Floor(dt(t).millis / 1000) }
func DateTimeToParts(t Value) Value        { return partsOf(dt(t).wall()) }
func DateTimeToPartsUtc(t Value) Value     { return partsOf(&date{timeClip(dt(t).millis)}) }
func DateTimeMapEpochMillis(f, t Value) Value {
	return &DateTime{millis: Apply(f, dt(t).millis).(float64), zone: dt(t).zone}
}

func DateTimeSetParts(p, t Value) Value { return dt(t).mutate(func(d *date) { setParts(d, p) }) }

func DateTimeSetPartsUtc(p, t Value) Value { return dt(t).mutateUtc(func(d *date) { setParts(d, p) }) }

func DateTimeRemoveTime(t Value) Value {
	d := dt(t).wall()
	d.setHours(0, 0, 0, 0)
	return utcAt(d.t)
}

func DateTimeZonedOffset(t Value) Value {
	if dt(t).zone == nil {
		return None
	}
	return Some(dt(t).zonedOffset())
}

func DateTimeZonedOffsetIso(t Value) Value {
	if dt(t).zone == nil {
		return None
	}
	return Some(offsetString(dt(t).zonedOffset()))
}

// add is Effect's: time units move the instant, calendar units the wall
// clock, a month or a year clamped to the end of a shorter month.
func addUnit(t *DateTime, unit string, n float64) *DateTime {
	if n == 0 || math.IsNaN(n) {
		return t.mutateUtc(func(*date) {})
	}
	switch unit {
	case "Millisecond", "Second", "Minute", "Hour":
		per := map[string]float64{"Millisecond": 1, "Second": 1000, "Minute": 60000, "Hour": 3600000}[unit]
		return t.mutateUtc(func(d *date) { d.setTime(d.t + n*per) })
	}
	return t.mutateUtc(func(*date) {}).mutate(func(d *date) {
		switch unit {
		case "Day":
			d.setDate(d.day() + n)
		case "Week":
			d.setDate(d.day() + n*7)
		case "Month":
			day := d.day()
			d.setMonth(d.month()+n+1, 0)
			if day < d.day() {
				d.setDate(day)
			}
		case "Year":
			day, month := d.day(), d.month()
			d.setFullYear(d.year()+n, month+1, 0)
			if day < d.day() {
				d.setDate(day)
			}
		}
	})
}

func unitOf(c Value) string { return ConstructorName(c.(*Data).Tag) }

func DateTimeAdd(c, n, t Value) Value      { return addUnit(dt(t), unitOf(c), n.(float64)) }
func DateTimeSubtract(c, n, t Value) Value { return addUnit(dt(t), unitOf(c), -1*n.(float64)) }

// toMillis is Effect's Duration.toMillis.
func toMillis(d *Duration) float64 {
	switch d.tag {
	case durationMillis:
		return d.millis
	case durationNanos:
		return bigToFloat(d.nanos) / 1e6
	case durationInfinite:
		return math.Inf(1)
	}
	return math.Inf(-1)
}

func DateTimeAddDuration(d, t Value) Value {
	return &DateTime{millis: dt(t).millis + toMillis(dur(d)), zone: dt(t).zone}
}

func DateTimeSubtractDuration(d, t Value) Value {
	return &DateTime{millis: dt(t).millis - toMillis(dur(d)), zone: dt(t).zone}
}

// weekStart is the first day of the week, modulo 7.
func weekStart(o Value) float64 {
	if w := record(o).Get("weekStartsOn"); isSome(w) {
		return posMod(float64(value(w).(int32)), 7)
	}
	return 0
}

func startOfDate(d *date, unit string, week float64) {
	switch unit {
	case "Second":
		d.setMillis(0)
	case "Minute":
		d.setSeconds(0, 0)
	case "Hour":
		d.setMinutes(0, 0, 0)
	case "Day":
		d.setHours(0, 0, 0, 0)
	case "Week":
		d.setDate(d.day() - posMod(d.weekDay()-week+7, 7))
		d.setHours(0, 0, 0, 0)
	case "Month":
		d.setDate(1)
		d.setHours(0, 0, 0, 0)
	case "Year":
		d.setMonth(0, 1)
		d.setHours(0, 0, 0, 0)
	}
}

func endOfDate(d *date, unit string, week float64) {
	switch unit {
	case "Second":
		d.setMillis(999)
	case "Minute":
		d.setSeconds(59, 999)
	case "Hour":
		d.setMinutes(59, 59, 999)
	case "Day":
		d.setHours(23, 59, 59, 999)
	case "Week":
		d.setDate(d.day() - posMod(d.weekDay()-week+7, 7) + 6)
		d.setHours(23, 59, 59, 999)
	case "Month":
		d.setMonth(d.month()+1, 0)
		d.setHours(23, 59, 59, 999)
	case "Year":
		d.setMonth(11, 31)
		d.setHours(23, 59, 59, 999)
	}
}

func DateTimeStartOf(o, c, t Value) Value {
	return dt(t).mutate(func(d *date) { startOfDate(d, unitOf(c), weekStart(o)) })
}

func DateTimeEndOf(o, c, t Value) Value {
	return dt(t).mutate(func(d *date) { endOfDate(d, unitOf(c), weekStart(o)) })
}

func DateTimeNearest(o, c, t Value) Value {
	unit, week := unitOf(c), weekStart(o)
	return dt(t).mutate(func(d *date) {
		if unit == "Millisecond" {
			return
		}
		start, end := &date{d.t}, &date{d.t}
		startOfDate(start, unit, week)
		endOfDate(end, unit, week)
		if d.t-start.t < end.t+1-d.t {
			d.t = start.t
		} else {
			d.t = end.t + 1
		}
	})
}

func DateTimeDistance(other, self Value) Value {
	return fromMillis(math.Abs(dt(other).millis - dt(self).millis))
}

func dateTimeOrder(a, b Value) int {
	x, y := dt(a).millis, dt(b).millis
	switch {
	case x < y:
		return -1
	case x > y:
		return 1
	}
	return 0
}

func DateTimeCompare(a, b Value) Value       { return ordering(dateTimeOrder(a, b)) }
func DateTimeMin(a, self Value) Value        { return orderMin(dateTimeOrder, self, a) }
func DateTimeMax(a, self Value) Value        { return orderMax(dateTimeOrder, self, a) }
func DateTimeClamp(options, t Value) Value   { return clamp(dateTimeOrder, options, t) }
func DateTimeBetween(options, t Value) Value { return between(dateTimeOrder, options, t) }
func DateTimeLess(a, self Value) Value       { return less(dateTimeOrder, a, self) }
func DateTimeLessEq(a, self Value) Value     { return lessEq(dateTimeOrder, a, self) }
func DateTimeGreater(a, self Value) Value    { return greater(dateTimeOrder, a, self) }
func DateTimeGreaterEq(a, self Value) Value  { return greaterEq(dateTimeOrder, a, self) }

func DateTimeIsFuture(fb *Fiber, t Value) Value { return clockOf(fb).millis() < dt(t).millis }
func DateTimeIsPast(fb *Fiber, t Value) Value   { return clockOf(fb).millis() > dt(t).millis }

func DateTimeFormatIso(t Value) Value { return (&date{timeClip(dt(t).millis)}).isoString() }

func DateTimeFormatIsoDate(t Value) Value { return dt(t).wall().isoString()[:10] }

func DateTimeFormatIsoDateUtc(t Value) Value { return (&date{timeClip(dt(t).millis)}).isoString()[:10] }

func formatIsoOffset(t *DateTime) string {
	iso := t.wall().isoString()
	if t.zone == nil {
		return iso
	}
	return iso[:len(iso)-1] + offsetString(t.zonedOffset())
}

func DateTimeFormatIsoOffset(t Value) Value { return formatIsoOffset(dt(t)) }

func DateTimeFormatIsoZoned(t Value) Value {
	x := dt(t)
	switch {
	case x.zone == nil:
		return None
	case x.zone.loc == nil:
		return Some(formatIsoOffset(x))
	}
	return Some(formatIsoOffset(x) + "[" + x.zone.id + "]")
}
