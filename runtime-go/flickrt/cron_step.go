package flickrt

import (
	"math"
	"slices"
)

// cronNotFound is Effect's `Unable to find cron date`.
type cronNotFound struct{}

func daysInMonth(date *jsDate) float64 {
	return newJsDate(makeDate(makeDay(date.year(), date.month()+1, 0), 0)).date()
}

// step is Effect's `stepCron`: the first matching instant after `now`, or
// the last before it in reverse; false when it finds none or `now` is out
// of range.
func (c *Cron) step(now *DateTime, reverse bool) (out float64, found bool) {
	defer func() {
		if r := recover(); r != nil {
			_, isRange := r.(rangeError)
			if _, missing := r.(cronNotFound); !isRange && !missing {
				panic(r)
			}
			found = false
		}
	}()
	zoned := makeZoned(now, c.tz, false, "")
	tick, table, boundary := 1.0, c.next, c.first
	needsStep := func(next, current int) bool { return next > current }
	disambiguation := "compatible"
	if reverse {
		tick, table, boundary = -1, c.prev, c.last
		needsStep = func(next, current int) bool { return next < current }
		disambiguation = "later"
	}
	utc := c.tz.loc != nil && c.tz.id == "UTC"
	wall := func(current *jsDate, disambiguation string) *DateTime {
		if math.IsNaN(current.t) {
			panic(rangeError("Invalid date"))
		}
		return makeZoned(&DateTime{ms: current.t}, zoned.zone, true, disambiguation)
	}
	adjustDst := func(current *jsDate) {
		if utc {
			return
		}
		adjusted := wall(current, disambiguation).toDate().t
		drift := current.t - adjusted
		if reverse && drift != 0 {
			current.setTime(adjusted)
		} else if !reverse && drift > 0 {
			current.setTime(current.t + drift)
		}
	}
	// repeated wall times resolve to their first occurrence, so when the
	// search starts in the second one, matches inside that fold are past
	isBeforeStart := func(current *jsDate) bool {
		return !reverse && !utc && wall(current, "compatible").ms <= zoned.ms
	}
	b := func(f int) float64 { return float64(boundary[f]) }
	has := func(f int) bool { return len(c.fields[f]) != 0 }
	nextDay := func(current *jsDate, days float64) {
		current.setDate(current.date() + days)
		current.setHours(b(cronHour), b(cronMinute), b(cronSecond))
		adjustDst(current)
	}
	result := zoned.mutate(func(current *jsDate) {
		current.setSeconds(current.seconds()+tick, 0)
		for i := 0; i < 10_000; i++ {
			if math.IsNaN(current.t) {
				panic(cronNotFound{})
			}
			if has(cronSecond) {
				now := int(current.seconds())
				if next := table[cronSecond][now]; next == -1 {
					current.setMinutes(current.minutes()+tick, b(cronSecond))
					adjustDst(current)
					continue
				} else if needsStep(next, now) {
					current.setSeconds(float64(next))
					adjustDst(current)
					continue
				}
			}
			if has(cronMinute) {
				now := int(current.minutes())
				if next := table[cronMinute][now]; next == -1 {
					current.setHours(current.hours()+tick, b(cronMinute), b(cronSecond))
					adjustDst(current)
					continue
				} else if needsStep(next, now) {
					current.setMinutes(float64(next), b(cronSecond))
					adjustDst(current)
					continue
				}
			}
			if has(cronHour) {
				now := int(current.hours())
				if next := table[cronHour][now]; next == -1 {
					nextDay(current, tick)
					continue
				} else if needsStep(next, now) {
					current.setHours(float64(next), b(cronMinute), b(cronSecond))
					adjustDst(current)
					continue
				}
			}
			if has(cronWeekday) || has(cronDay) {
				if c.and {
					matchesDay := !has(cronDay) || slices.Contains(c.fields[cronDay], int(current.date()))
					matchesWeekday := !has(cronWeekday) || slices.Contains(c.fields[cronWeekday], int(current.weekDay()))
					if !matchesDay || !matchesWeekday {
						nextDay(current, tick)
						continue
					}
				} else if days := c.daysToMatch(current, reverse, table, boundary); days != 0 {
					nextDay(current, days)
					continue
				}
			}
			if has(cronMonth) {
				now := int(current.month()) + 1
				clamp := func(target float64) float64 {
					last := daysInMonth(newJsDate(makeDate(makeDay(current.year(), target+1, 0), 0)))
					switch {
					case has(cronDay) && !has(cronWeekday) && reverse:
						if day := table[cronDay][int(last)]; day != -1 {
							return float64(day)
						}
						return last
					case has(cronDay) && !has(cronWeekday):
						return b(cronDay)
					case reverse:
						return last
					}
					return 1
				}
				if next := table[cronMonth][now]; next == -1 {
					current.setFullYear(current.year() + tick)
					current.setMonth(b(cronMonth), clamp(b(cronMonth)))
					current.setHours(b(cronHour), b(cronMinute), b(cronSecond))
					adjustDst(current)
					continue
				} else if needsStep(next, now) {
					target := float64(next - 1)
					current.setMonth(target, clamp(target))
					current.setHours(b(cronHour), b(cronMinute), b(cronSecond))
					adjustDst(current)
					continue
				}
			}
			if isBeforeStart(current) {
				current.setSeconds(current.seconds() + 1)
				i = -1 // skipped fold candidates do not count against the search budget
				continue
			}
			return
		}
		panic(cronNotFound{})
	})
	return result.ms, true
}

// daysToMatch is Effect's step to the nearer of the next matching weekday
// and day of the month, when either may match.
func (c *Cron) daysToMatch(current *jsDate, reverse bool, table [6][]int, boundary [6]int) float64 {
	a, b := math.Inf(1), math.Inf(1)
	if reverse {
		a, b = math.Inf(-1), math.Inf(-1)
	}
	if len(c.fields[cronWeekday]) != 0 {
		now := int(current.weekDay())
		switch next := table[cronWeekday][now]; {
		case next != -1:
			a = float64(next - now)
		case reverse:
			a = float64(boundary[cronWeekday] - 7 - now)
		default:
			a = float64(7 - now + boundary[cronWeekday])
		}
	}
	if len(c.fields[cronDay]) != 0 && a != 0 {
		now := current.date()
		next := table[cronDay][int(now)]
		switch {
		case next == -1 && reverse:
			// day zero is the previous month's last day; two probes cover every valid day
			previous := newJsDate(current.t)
			previous.setDate(0)
			day := table[cronDay][int(previous.date())]
			if day == -1 {
				previous.setDate(0)
				day = table[cronDay][int(previous.date())]
			}
			if day == -1 {
				panic(cronNotFound{})
			}
			previous.setDate(float64(day))
			b = (previous.t - current.t) / msPerDay
		case next == -1, !reverse && float64(next) > daysInMonth(current):
			b = daysInMonth(current) - now + float64(boundary[cronDay])
		default:
			b = float64(next) - now
		}
	}
	if reverse {
		return math.Max(a, b)
	}
	return math.Min(a, b)
}

func cronFound(ms float64, found bool) Value {
	if !found {
		return None
	}
	return Some(&DateTime{ms: ms})
}

func CronNext(now, cron Value) Value { return cronFound(cron.(*Cron).step(dateTime(now), false)) }
func CronPrev(now, cron Value) Value { return cronFound(cron.(*Cron).step(dateTime(now), true)) }

// CronSequence is the adapter's: the first `n` matches, fewer when Effect
// finds no further one.
func CronSequence(n, now, cron Value) Value {
	out, at := []Value{}, dateTime(now)
	for len(out) < int(n.(int32)) {
		ms, found := cron.(*Cron).step(at, false)
		if !found {
			break
		}
		at = &DateTime{ms: ms}
		out = append(out, at)
	}
	return out
}

// ScheduleCron is Effect's `Schedule.cron`: recurs at each match, the
// delay until it its output.
func ScheduleCron(cron Value) Value {
	c := cron.(*Cron)
	return &Schedule{func(*Fiber) scheduleStep {
		return func(_ *Fiber, now float64, _ Value) (Value, *Duration, bool) {
			if math.IsInf(now, 1) {
				return noDelay, nil, true
			}
			next, found := c.step(&DateTime{ms: timeClip(now)}, false)
			if !found {
				panic(Defect{"Unable to find cron date"})
			}
			d := fromMillis(next - now)
			return d, d, false
		}
	}}
}
