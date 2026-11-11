package flickrt

import "math"

// DateTime's parts, arithmetic, comparisons and ISO formatting.

func DateTimeToEpochMillis(d Value) Value  { return dateTime(d).ms }
func DateTimeToEpochSeconds(d Value) Value { return math.Floor(dateTime(d).ms / 1000) }

func DateTimeMapEpochMillis(fb *Fiber, f, self Value) Value {
	d := dateTime(self)
	return d.with(Apply(fb, f, d.ms).(float64))
}

func partsOf(date *jsDate) Value {
	return Rec("millisecond", int32(date.millis()), "second", int32(date.seconds()), "minute", int32(date.minutes()),
		"hour", int32(date.hours()), "day", int32(date.date()), "weekDay", int32(date.weekDay()),
		"month", int32(date.month()+1), "year", int32(date.year()))
}

func DateTimeToParts(d Value) Value    { return partsOf(dateTime(d).toDate()) }
func DateTimeToPartsUtc(d Value) Value { return partsOf(newJsDate(dateTime(d).ms)) }

// setParts sets the parts of the adapter's PartialParts on a date.
func setParts(date *jsDate, parts Value) {
	r := record(parts)
	part := func(name string) (float64, bool) {
		if o := r.Get(name); isSome(o) {
			return float64(value(o).(int32)), true
		}
		return 0, false
	}
	year, hasYear := part("year")
	month, hasMonth := part("month")
	day, hasDay := part("day")
	if hasYear || hasMonth || hasDay {
		if !hasYear {
			year = date.year()
		}
		if hasMonth {
			month--
		} else {
			month = date.month()
		}
		if !hasDay {
			day = date.date()
		}
		date.setFullYear(year, month, day)
	}
	if weekDay, ok := part("weekDay"); ok {
		date.setDate(date.date() + weekDay - date.weekDay())
	}
	if hour, ok := part("hour"); ok {
		date.setHours(hour)
	}
	if minute, ok := part("minute"); ok {
		date.setMinutes(minute)
	}
	if second, ok := part("second"); ok {
		date.setSeconds(second)
	}
	if ms, ok := part("millisecond"); ok {
		date.setMilliseconds(ms)
	}
}

func DateTimeSetParts(parts, self Value) Value {
	return dateTime(self).mutate(func(date *jsDate) { setParts(date, parts) })
}

func DateTimeSetPartsUtc(parts, self Value) Value {
	return dateTime(self).mutateUtc(func(date *jsDate) { setParts(date, parts) })
}

func DateTimeRemoveTime(d Value) Value {
	date := dateTime(d).toDate()
	date.setHours(0, 0, 0, 0)
	return &DateTime{ms: date.t}
}

func DateTimeZonedOffset(d Value) Value {
	if dateTime(d).zone == nil {
		return None
	}
	return Some(dateTime(d).zonedOffset())
}

func DateTimeZonedOffsetIso(d Value) Value {
	if dateTime(d).zone == nil {
		return None
	}
	return Some(offsetToString(dateTime(d).zonedOffset()))
}

// add moves a date-time: time units move the instant, calendar units the
// wall clock; a part of 0 or NaN does nothing, as JS's truthiness has it.
func add(d *DateTime, unit string, n float64) *DateTime {
	set := n != 0 && !math.IsNaN(n)
	scale, instant := map[string]float64{"Millisecond": 1, "Second": 1000, "Minute": msPerMinute, "Hour": msPerHour}[unit]
	shifted := d.mutateUtc(func(date *jsDate) {
		if set && instant {
			date.setTime(date.t + n*scale)
		}
	})
	if !set || instant {
		return shifted
	}
	return shifted.mutate(func(date *jsDate) {
		switch unit {
		case "Day":
			date.setDate(date.date() + n)
		case "Week":
			date.setDate(date.date() + n*7)
		case "Month":
			day := date.date()
			date.setMonth(date.month()+n+1, 0)
			if day < date.date() {
				date.setDate(day)
			}
		case "Year":
			day, month := date.date(), date.month()
			date.setFullYear(date.year()+n, month+1, 0)
			if day < date.date() {
				date.setDate(day)
			}
		}
	})
}

func DateTimeAdd(unit, n, self Value) Value {
	return add(dateTime(self), ConstructorName(unit.(*Data).Tag), n.(float64))
}

func DateTimeSubtract(unit, n, self Value) Value {
	return add(dateTime(self), ConstructorName(unit.(*Data).Tag), -1*n.(float64))
}

func DateTimeAddDuration(duration, self Value) Value {
	d := dateTime(self)
	return d.with(d.ms + millisOf(dur(duration)))
}

func DateTimeSubtractDuration(duration, self Value) Value {
	d := dateTime(self)
	return d.with(d.ms - millisOf(dur(duration)))
}

// weekStart is the adapter's: the first day of the week, modulo 7.
func weekStart(o Value) float64 {
	if n := record(o).Get("weekStartsOn"); isSome(n) {
		return float64((value(n).(int32)%7 + 7) % 7)
	}
	return 0
}

func startOfDate(date *jsDate, unit string, weekStartsOn float64) {
	switch unit {
	case "Second":
		date.setMilliseconds(0)
	case "Minute":
		date.setSeconds(0, 0)
	case "Hour":
		date.setMinutes(0, 0, 0)
	case "Day":
		date.setHours(0, 0, 0, 0)
	case "Week":
		date.setDate(date.date() - posMod(date.weekDay()-weekStartsOn+7, 7))
		date.setHours(0, 0, 0, 0)
	case "Month":
		date.setDate(1)
		date.setHours(0, 0, 0, 0)
	case "Year":
		date.setMonth(0, 1)
		date.setHours(0, 0, 0, 0)
	}
}

func endOfDate(date *jsDate, unit string, weekStartsOn float64) {
	switch unit {
	case "Second":
		date.setMilliseconds(999)
	case "Minute":
		date.setSeconds(59, 999)
	case "Hour":
		date.setMinutes(59, 59, 999)
	case "Day":
		date.setHours(23, 59, 59, 999)
	case "Week":
		date.setDate(date.date() - posMod(date.weekDay()-weekStartsOn+7, 7) + 6)
		date.setHours(23, 59, 59, 999)
	case "Month":
		date.setMonth(date.month()+1, 0)
		date.setHours(23, 59, 59, 999)
	case "Year":
		date.setMonth(11, 31)
		date.setHours(23, 59, 59, 999)
	}
}

func DateTimeStartOf(o, unit, self Value) Value {
	name, week := ConstructorName(unit.(*Data).Tag), weekStart(o)
	return dateTime(self).mutate(func(date *jsDate) { startOfDate(date, name, week) })
}

func DateTimeEndOf(o, unit, self Value) Value {
	name, week := ConstructorName(unit.(*Data).Tag), weekStart(o)
	return dateTime(self).mutate(func(date *jsDate) { endOfDate(date, name, week) })
}

func DateTimeNearest(o, unit, self Value) Value {
	name, week := ConstructorName(unit.(*Data).Tag), weekStart(o)
	return dateTime(self).mutate(func(date *jsDate) {
		if name == "Millisecond" {
			return
		}
		start, end := newJsDate(date.t), newJsDate(date.t)
		startOfDate(start, name, week)
		endOfDate(end, name, week)
		if date.t-start.t < end.t+1-date.t {
			date.setTime(start.t)
		} else {
			date.setTime(end.t + 1)
		}
	})
}

// DateTimeDistance is the adapter's: never negative.
func DateTimeDistance(other, self Value) Value {
	return fromMillis(math.Abs(dateTime(other).ms - dateTime(self).ms))
}

// dateTimeOrder is the order of date-times: by instant.
func dateTimeOrder(a, b Value) int {
	x, y := dateTime(a).ms, dateTime(b).ms
	switch {
	case x < y:
		return -1
	case x > y:
		return 1
	}
	return 0
}

func DateTimeCompare(a, b Value) Value { return ordering(dateTimeOrder(a, b)) }

// The comparisons are data-last: they take `that` first.
func DateTimeMin(that, self Value) Value                 { return orderMin(dateTimeOrder, self, that) }
func DateTimeMax(that, self Value) Value                 { return orderMax(dateTimeOrder, self, that) }
func DateTimeClamp(bounds, self Value) Value             { return clamp(dateTimeOrder, bounds, self) }
func DateTimeBetween(bounds, self Value) Value           { return between(dateTimeOrder, bounds, self) }
func DateTimeIsLessThan(that, self Value) Value          { return less(dateTimeOrder, that, self) }
func DateTimeIsLessThanOrEqualTo(that, self Value) Value { return lessEq(dateTimeOrder, that, self) }
func DateTimeIsGreaterThan(that, self Value) Value       { return greater(dateTimeOrder, that, self) }
func DateTimeIsGreaterThanOrEqualTo(that, self Value) Value {
	return greaterEq(dateTimeOrder, that, self)
}

func DateTimeIsFuture(fb *Fiber, self Value) Value { return less(dateTimeOrder, self, DateTimeNow(fb)) }
func DateTimeIsPast(fb *Fiber, self Value) Value {
	return greater(dateTimeOrder, self, DateTimeNow(fb))
}

func DateTimeFormatIso(d Value) Value { return newJsDate(dateTime(d).ms).isoString() }

func DateTimeFormatIsoDate(d Value) Value { return dateTime(d).toDate().isoString()[:10] }

func DateTimeFormatIsoDateUtc(d Value) Value { return newJsDate(dateTime(d).ms).isoString()[:10] }

func (d *DateTime) formatIsoOffset() string {
	iso := d.toDate().isoString()
	if d.zone == nil {
		return iso
	}
	return iso[:len(iso)-1] + offsetToString(d.zonedOffset())
}

func DateTimeFormatIsoOffset(d Value) Value { return dateTime(d).formatIsoOffset() }

// formatIsoZoned is the ISO text with the zone's name unless it is an offset.
func (d *DateTime) formatIsoZoned() string {
	if d.zone.loc == nil {
		return d.formatIsoOffset()
	}
	return d.formatIsoOffset() + "[" + d.zone.id + "]"
}

func DateTimeFormatIsoZoned(d Value) Value {
	if dateTime(d).zone == nil {
		return None
	}
	return Some(dateTime(d).formatIsoZoned())
}
