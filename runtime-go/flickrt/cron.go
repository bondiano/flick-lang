package flickrt

import (
	"math"
	"slices"
	"strconv"
	"strings"
)

// Cron is Effect's: sets of seconds, minutes, hours, days, months and
// weekdays (each empty for every value), in a zone, with lookup tables of
// the next and previous value of each.
type Cron struct {
	fields      [6][]int // seconds, minutes, hours, days, months, weekdays: sorted, unique
	and         bool
	tz          *TimeZone
	first, last [6]int
	next, prev  [6][]int // by value; -1 where there is none
}

const (
	cronSecond = iota
	cronMinute
	cronHour
	cronDay
	cronMonth
	cronWeekday
)

type cronRange struct {
	name     string
	min, max int
	size     int // of the lookup table
	aliases  map[string]int
}

var cronRanges = [6]cronRange{
	{"seconds", 0, 59, 60, nil},
	{"minutes", 0, 59, 60, nil},
	{"hours", 0, 23, 24, nil},
	{"days", 1, 31, 32, nil},
	{"months", 1, 12, 13, map[string]int{"jan": 1, "feb": 2, "mar": 3, "apr": 4, "may": 5, "jun": 6, "jul": 7, "aug": 8, "sep": 9, "oct": 10, "nov": 11, "dec": 12}},
	{"weekdays", 0, 7, 7, map[string]int{"sun": 0, "mon": 1, "tue": 2, "wed": 3, "thu": 4, "fri": 5, "sat": 6}},
}

// The first and last values when a field is empty; months count from 0.
var cronDefaults = [2][6]int{{0, 0, 0, 1, 1, 0}, {59, 59, 23, 31, 12, 6}}

func normalizeCron(field, v int) int {
	if field == cronWeekday && v == 7 {
		return 0
	}
	return v
}

func cronOf(v Value) *Cron { return v.(*Cron) }

// newCron is Effect's `make`: an error message for a value out of range.
func newCron(values [6][]int, and bool, zone *TimeZone) (*Cron, string) {
	c := &Cron{and: and, tz: zone}
	for f, vs := range values {
		r := cronRanges[f]
		var out []int
		for _, v := range vs {
			if v < r.min || v > r.max {
				return nil, r.name + " must contain only integers between " + strconv.Itoa(r.min) + " and " + strconv.Itoa(r.max)
			}
			out = append(out, normalizeCron(f, v))
		}
		slices.Sort(out)
		c.fields[f] = slices.Compact(out)
		c.first[f], c.last[f] = cronDefaults[0][f], cronDefaults[1][f]
		if n := len(c.fields[f]); n > 0 {
			c.first[f], c.last[f] = c.fields[f][0], c.fields[f][n-1]
		}
		c.next[f], c.prev[f] = cronTable(c.fields[f], r.size, true), cronTable(c.fields[f], r.size, false)
	}
	c.first[cronMonth]--
	c.last[cronMonth]--
	return c, ""
}

// cronTable is, for each index, the first value at or after it (next) or
// the last at or before it (prev).
func cronTable(values []int, size int, next bool) []int {
	out := make([]int, size)
	for i := range out {
		out[i] = -1
		if len(values) == 0 {
			continue
		}
		if next {
			if j, _ := slices.BinarySearch(values, i); j < len(values) {
				out[i] = values[j]
			}
		} else if j, found := slices.BinarySearch(values, i); found {
			out[i] = values[j]
		} else if j > 0 {
			out[i] = values[j-1]
		}
	}
	return out
}

func (c *Cron) has(field, v int) bool { _, ok := slices.BinarySearch(c.fields[field], v); return ok }

var cronErrorTag = Tag("Cron.CronParseError.CronParseError")

func cronError(message string) Value { return Err(NewData(cronErrorTag, message)) }

func intList(v Value) []int {
	xs := List(v)
	out := make([]int, len(xs))
	for i, x := range xs {
		out[i] = int(x.(int32))
	}
	return out
}

func CronMake(fields Value) Value {
	r := record(fields)
	values := [6][]int{{0}, intList(r.Get("minutes")), intList(r.Get("hours")), intList(r.Get("days")),
		intList(r.Get("months")), intList(r.Get("weekdays"))}
	if s := r.Get("seconds"); isSome(s) {
		values[cronSecond] = intList(value(s))
	}
	and := r.Get("and")
	c, bad := newCron(values, isSome(and) && value(and).(bool), tz(r.Get("tz")))
	if c == nil {
		return cronError(bad)
	}
	return Ok(c)
}

func CronParse(zone, s Value) Value {
	text := s.(string)
	c, bad := parseCron(text, tz(zone))
	if c == nil {
		return cronError(bad + ": " + QuoteJSON(text))
	}
	return Ok(c)
}

func parseCron(text string, zone *TimeZone) (*Cron, string) {
	segments := strings.Fields(text)
	if len(segments) != 5 && len(segments) != 6 {
		return nil, "Invalid number of segments in cron expression"
	}
	if len(segments) == 5 {
		segments = append([]string{"0"}, segments...)
	}
	var values [6][]int
	var wildcard [6]bool
	for f, segment := range segments {
		var bad string
		values[f], wildcard[f], bad = parseCronSegment(segment, f)
		if bad != "" {
			return nil, bad
		}
	}
	and := (wildcard[cronDay] || wildcard[cronWeekday]) && len(values[cronDay]) > 0 && len(values[cronWeekday]) > 0
	return newCron(values, and, zone)
}

// parseCronSegment is Effect's parseSegment: the values in the order
// given, whether it starts with `*`, or the error.
func parseCronSegment(input string, field int) ([]int, bool, string) {
	r := cronRanges[field]
	parts := strings.Split(input, ",")
	firstRaw, firstStep, firstHasStep := splitCronStep(parts[0])
	wildcard := firstRaw == "*"
	every := wildcard && (!firstHasStep || firstStep == 1)
	var values []int
	seen := map[int]bool{}
	add := func(v int) {
		if v = normalizeCron(field, v); !every && !seen[v] {
			seen[v] = true
			values = append(values, v)
		}
	}
	for i, part := range parts {
		raw, step, hasStep := firstRaw, firstStep, firstHasStep
		if i > 0 {
			raw, step, hasStep = splitCronStep(part)
		}
		if hasStep {
			switch {
			case math.IsNaN(step) || math.IsInf(step, 0):
				return nil, false, "Expected step value to be a positive integer"
			case step < 1:
				return nil, false, "Expected step value to be greater than 0"
			case step > float64(r.max):
				return nil, false, "Expected step value to be less than or equal to " + strconv.Itoa(r.max)
			}
		} else {
			step = 1
		}
		by := int(step)
		if raw == "*" {
			if i == 0 && by == 1 {
				continue
			}
			for v := r.min; v <= r.max; v += by {
				add(v)
			}
			continue
		}
		left, right, hasRight := raw, "", false
		if j := strings.IndexByte(raw, '-'); j >= 0 {
			left, right, hasRight = raw[:j], raw[j+1:], true
		}
		lo, bad := cronValue(left, r)
		if bad != "" {
			return nil, false, bad
		}
		hi := lo
		switch {
		case hasRight:
			if hi, bad = cronValue(right, r); bad != "" {
				return nil, false, bad
			}
			if lo > hi {
				return nil, false, "Invalid value range"
			}
		case hasStep:
			hi = float64(r.max)
		}
		for v := lo; v <= hi; v += float64(by) {
			add(int(v))
		}
	}
	return values, wildcard, ""
}

func splitCronStep(s string) (string, float64, bool) {
	raw, step, ok := strings.Cut(s, "/")
	if !ok {
		return s, 0, false
	}
	if !isDigits(step) {
		return raw, math.NaN(), true
	}
	n, _ := strconv.ParseFloat(step, 64)
	return raw, n, true
}

// cronValue is an alias's value or a decimal one, in range.
func cronValue(s string, r cronRange) (float64, string) {
	v, ok := r.aliases[strings.ToLower(s)]
	n := float64(v)
	if !ok {
		if !isDigits(s) {
			return 0, "Expected a positive integer"
		}
		n, _ = strconv.ParseFloat(s, 64)
	}
	if n < float64(r.min) || n > float64(r.max) {
		return 0, "Expected a value between " + strconv.Itoa(r.min) + " and " + strconv.Itoa(r.max)
	}
	return n, ""
}

func CronFormat(options, c Value) Value {
	x := cronOf(c)
	segments := make([]string, 6)
	for f, vs := range x.fields {
		segments[f] = formatCronSegment(vs)
	}
	include := record(options).Get("includeSeconds")
	if !(isSome(include) && value(include).(bool)) && len(x.fields[cronSecond]) == 1 && x.fields[cronSecond][0] == 0 {
		segments = segments[1:]
	}
	return strings.Join(segments, " ")
}

// formatCronSegment writes runs of three or more values with one step
// as a range.
func formatCronSegment(values []int) string {
	if len(values) == 0 {
		return "*"
	}
	var out []string
	for i := 0; i < len(values); {
		start := values[i]
		if i+2 < len(values) && values[i+2]-values[i+1] == values[i+1]-start {
			step, end := values[i+1]-start, i+2
			for end+1 < len(values) && values[end+1]-values[end] == step {
				end++
			}
			text := strconv.Itoa(start) + "-" + strconv.Itoa(values[end])
			if step != 1 {
				text += "/" + strconv.Itoa(step)
			}
			out = append(out, text)
			i = end + 1
		} else {
			out = append(out, strconv.Itoa(start))
			i++
		}
	}
	return strings.Join(out, ",")
}

// zoned is the instant in the cron's zone, as Effect's makeZonedUnsafe
// gives it; a RangeError for one out of range.
func (c *Cron) zoned(t *DateTime) *DateTime {
	z := makeZoned(t, c.tz, false, "")
	if z == nil {
		panic(Defect{"RangeError: Epoch millis out of range: " + ShowNumber(t.millis)})
	}
	return z
}

func CronMatch(now, c Value) Value {
	x := cronOf(c)
	d := x.zoned(dt(now)).wall()
	_, month, day := d.ymd()
	values := [6]int{int(d.seconds()), int(d.minutes()), int(d.hours()), int(day), int(month) + 1, int(d.weekDay())}
	for _, f := range []int{cronSecond, cronMinute, cronHour, cronMonth} {
		if len(x.fields[f]) > 0 && !x.has(f, values[f]) {
			return false
		}
	}
	days, weekdays := len(x.fields[cronDay]) > 0, len(x.fields[cronWeekday]) > 0
	inDays, inWeekdays := x.has(cronDay, values[cronDay]), x.has(cronWeekday, values[cronWeekday])
	switch {
	case !days && !weekdays:
		return true
	case x.and:
		return (!days || inDays) && (!weekdays || inWeekdays)
	case !weekdays:
		return inDays
	case !days:
		return inWeekdays
	}
	return inDays || inWeekdays
}

func CronEquals(a, b Value) Value {
	x, y := cronOf(a), cronOf(b)
	if !x.tz.equal(y.tz) || x.and != y.and {
		return false
	}
	for f := range x.fields {
		if !slices.Equal(x.fields[f], y.fields[f]) {
			return false
		}
	}
	return true
}

// found is the date of `step`, or None when it finds none.
func found(step func() *DateTime) Value {
	t, ok := tryCron(step)
	if !ok {
		return None
	}
	return Some(t)
}

func CronNext(now, c Value) Value {
	return found(func() *DateTime { return cronOf(c).step(dt(now), false) })
}
func CronPrev(now, c Value) Value {
	return found(func() *DateTime { return cronOf(c).step(dt(now), true) })
}

func CronSequence(n, now, c Value) Value {
	out := []Value{}
	t := dt(now)
	for len(out) < int(n.(int32)) {
		next, ok := tryCron(func() *DateTime { return cronOf(c).step(t, false) })
		if !ok {
			break
		}
		out, t = append(out, next), next
	}
	return out
}

// cronNextMillis is the epoch millis of the first match after `now`, for
// Schedule.cron; finding none panics with Effect's error, a defect there
// as in Effect.
func cronNextMillis(c Value, now float64) float64 {
	return cronOf(c).step(utcAt(timeClip(now)), false).millis
}
