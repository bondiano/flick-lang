package flickrt

import "math"

// noCronDate is Effect's throw when a search finds no date.
var noCronDate = Defect{"Error: Unable to find cron date"}

// tryCron is the date `step` finds, false when it finds none or a date it
// reads is out of range, as the JS adapters catch Effect's throws.
func tryCron(step func() *DateTime) (t *DateTime, ok bool) {
	defer func() {
		if r := recover(); r != nil {
			if _, thrown := r.(Defect); !thrown {
				panic(r)
			}
			t, ok = nil, false
		}
	}()
	t = step()
	return t, t != nil && !math.IsNaN(t.millis)
}

func daysInMonth(d *date) float64 {
	end := &date{makeDate(makeDay(d.year(), d.month()+1, 0), 0)}
	return end.day()
}

// step is Effect's stepCron: the first match after `now` (or the last
// before it, `reverse`), searched field by field on the wall clock of the
// cron's zone, a skipped or repeated hour handled as Effect does.
func (c *Cron) step(now *DateTime, reverse bool) *DateTime {
	zoned := c.zoned(now)
	zone := zoned.zone
	tick := 1.0
	table, boundary := c.next, c.first
	needsStep := func(next, current int) bool { return next > current }
	disambiguation := "compatible"
	if reverse {
		tick, table, boundary = -1, c.prev, c.last
		needsStep = func(next, current int) bool { return next < current }
		disambiguation = "later"
	}
	utc := zone.loc != nil && zone.id == "UTC"
	placed := func(d *date, how string) *DateTime {
		t := makeZoned(utcAt(d.t), zone, true, how)
		if t == nil {
			panic(Defect{"RangeError: Epoch millis out of range: " + ShowNumber(d.t)})
		}
		return t
	}
	adjustDst := func(d *date) {
		if utc {
			return
		}
		adjusted := placed(d, disambiguation).wall().t
		drift := d.t - adjusted
		switch {
		case reverse && drift != 0:
			d.setTime(adjusted)
		case !reverse && drift > 0:
			d.setTime(d.t + drift)
		}
	}
	isBeforeStart := func(d *date) bool {
		return !reverse && !utc && placed(d, "compatible").millis <= zoned.millis
	}
	b := func(f int) float64 { return float64(boundary[f]) }
	result := zoned.mutate(func(d *date) {
		d.setSeconds(d.seconds()+tick, 0)
		for i := 0; i < 10_000; i++ {
			if len(c.fields[cronSecond]) > 0 {
				current := int(d.seconds())
				next := table[cronSecond][current]
				if next < 0 {
					d.setMinutes(d.minutes()+tick, b(cronSecond), d.milliseconds())
					adjustDst(d)
					continue
				}
				if needsStep(next, current) {
					d.setSeconds(float64(next), d.milliseconds())
					adjustDst(d)
					continue
				}
			}
			if len(c.fields[cronMinute]) > 0 {
				current := int(d.minutes())
				next := table[cronMinute][current]
				if next < 0 {
					d.setHours(d.hours()+tick, b(cronMinute), b(cronSecond), d.milliseconds())
					adjustDst(d)
					continue
				}
				if needsStep(next, current) {
					d.setMinutes(float64(next), b(cronSecond), d.milliseconds())
					adjustDst(d)
					continue
				}
			}
			toDay := func(days float64) {
				d.setDate(d.day() + days)
				d.setHours(b(cronHour), b(cronMinute), b(cronSecond), d.milliseconds())
				adjustDst(d)
			}
			if len(c.fields[cronHour]) > 0 {
				current := int(d.hours())
				next := table[cronHour][current]
				if next < 0 {
					toDay(tick)
					continue
				}
				if needsStep(next, current) {
					d.setHours(float64(next), b(cronMinute), b(cronSecond), d.milliseconds())
					adjustDst(d)
					continue
				}
			}
			days, weekdays := len(c.fields[cronDay]) > 0, len(c.fields[cronWeekday]) > 0
			if days || weekdays {
				if c.and {
					if days && !c.has(cronDay, int(d.day())) || weekdays && !c.has(cronWeekday, int(d.weekDay())) {
						toDay(tick)
						continue
					}
				} else if add := c.dayStep(d, reverse, table, boundary); add != 0 {
					toDay(add)
					continue
				}
			}
			if len(c.fields[cronMonth]) > 0 {
				current := int(d.month()) + 1
				next := table[cronMonth][current]
				clampDay := func(target float64) float64 {
					maxDay := daysInMonth(&date{makeDate(makeDay(d.year(), target+1, 0), 0)})
					if days && !weekdays {
						if reverse {
							if v := table[cronDay][int(maxDay)]; v >= 0 {
								return float64(v)
							}
							return maxDay
						}
						return b(cronDay)
					}
					if reverse {
						return maxDay
					}
					return 1
				}
				if next < 0 {
					d.setFullYear(d.year()+tick, d.month(), d.day())
					d.setMonth(b(cronMonth), clampDay(b(cronMonth)))
					d.setHours(b(cronHour), b(cronMinute), b(cronSecond), d.milliseconds())
					adjustDst(d)
					continue
				}
				if needsStep(next, current) {
					d.setMonth(float64(next-1), clampDay(float64(next-1)))
					d.setHours(b(cronHour), b(cronMinute), b(cronSecond), d.milliseconds())
					adjustDst(d)
					continue
				}
			}
			if isBeforeStart(d) {
				d.setSeconds(d.seconds()+1, d.milliseconds())
				i = -1 // skipped fold candidates do not count against the search budget
				continue
			}
			return
		}
		panic(noCronDate)
	})
	return utcAt(timeClip(result.millis))
}

// dayStep is how many days to move to the next (or previous) day that
// matches the days or the weekdays, 0 when this one does.
func (c *Cron) dayStep(d *date, reverse bool, table [6][]int, boundary [6]int) float64 {
	a, b := math.Inf(1), math.Inf(1)
	if reverse {
		a, b = math.Inf(-1), math.Inf(-1)
	}
	if len(c.fields[cronWeekday]) > 0 {
		current := int(d.weekDay())
		switch next := table[cronWeekday][current]; {
		case next >= 0:
			a = float64(next - current)
		case reverse:
			a = float64(boundary[cronWeekday] - 7 - current)
		default:
			a = float64(7 - current + boundary[cronWeekday])
		}
	}
	if len(c.fields[cronDay]) > 0 && a != 0 {
		current := d.day()
		next := table[cronDay][int(current)]
		switch {
		case next < 0 && reverse:
			previous := &date{d.t}
			previous.setDate(0)
			day := table[cronDay][int(previous.day())]
			if day < 0 {
				previous.setDate(0)
				day = table[cronDay][int(previous.day())]
			}
			if day < 0 {
				panic(noCronDate)
			}
			previous.setDate(float64(day))
			b = (previous.t - d.t) / msPerDay
		case next < 0 || !reverse && float64(next) > daysInMonth(d):
			b = daysInMonth(d) - current + float64(boundary[cronDay])
		default:
			b = float64(next) - current
		}
	}
	if reverse {
		return math.Max(a, b)
	}
	return math.Min(a, b)
}
