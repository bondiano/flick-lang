package flickrt

import (
	"math"
	"regexp"
	"slices"
	"strconv"
	"strings"
)

// Cron is sets of seconds, minutes, hours, days, months and weekdays, each
// sorted (empty for every value), in a zone, with the tables its search
// for the next or previous match steps by.
type Cron struct {
	tz     *TimeZone
	fields [6][]int // seconds, minutes, hours, days, months, weekdays
	and    bool
	first  [6]int
	last   [6]int
	next   [6][]int // the nearest value at or after each one; -1 for none
	prev   [6][]int // at or before
}

const (
	cronSecond = iota
	cronMinute
	cronHour
	cronDay
	cronMonth
	cronWeekday
)

// cronFields are each field's name and range, and the size of its tables.
var cronFields = [6]struct {
	name     string
	min, max int
	size     int
}{{"seconds", 0, 59, 60}, {"minutes", 0, 59, 60}, {"hours", 0, 23, 24}, {"days", 1, 31, 32}, {"months", 1, 12, 13}, {"weekdays", 0, 7, 7}}

var cronParseErrorTag = Tag("Cron.CronParseError.CronParseError")

func cronParseError(message string) Value { return Err(NewData(cronParseErrorTag, message)) }

func normalizeWeekday(v int) int {
	if v == 7 {
		return 0
	}
	return v
}

// makeCron is the Cron of the values of its fields: it throws a RangeError
// for a value out of its field's range.
func makeCron(tz *TimeZone, values [6][]int, and bool) *Cron {
	c := &Cron{tz: tz, and: and}
	firsts, lasts := [6]int{0, 0, 0, 1, 1, 0}, [6]int{59, 59, 23, 31, 12, 6}
	for f, field := range cronFields {
		set := []int{}
		for _, v := range values[f] {
			if v < field.min || v > field.max {
				panic(rangeError(field.name + " must contain only integers between " + strconv.Itoa(field.min) + " and " + strconv.Itoa(field.max)))
			}
			if f == cronWeekday {
				v = normalizeWeekday(v)
			}
			set = append(set, v)
		}
		slices.Sort(set)
		set = slices.Compact(set)
		c.fields[f] = set
		c.first[f], c.last[f] = firsts[f], lasts[f]
		if len(set) > 0 {
			c.first[f], c.last[f] = set[0], set[len(set)-1]
		}
		c.next[f], c.prev[f] = cronLookup(set, field.size, true), cronLookup(set, field.size, false)
	}
	c.first[cronMonth]--
	c.last[cronMonth]--
	return c
}

// cronLookup is a lookup table: for each value of the field, the least set
// one at or after it, or with `next` false the greatest at or before it;
// -1 for none.
func cronLookup(values []int, size int, next bool) []int {
	out := make([]int, size)
	for i := range out {
		out[i] = -1
		for _, v := range values {
			if next && v >= i && out[i] == -1 || !next && v <= i {
				out[i] = v
			}
		}
	}
	return out
}

func cronInts(v Value) []int {
	out := []int{}
	for _, x := range List(v) {
		out = append(out, int(x.(int32)))
	}
	return out
}

func CronMake(fields Value) Value {
	r := record(fields)
	seconds := []int{0}
	if s := r.Get("seconds"); isSome(s) {
		seconds = cronInts(value(s))
	}
	and := false
	if a := r.Get("and"); isSome(a) {
		and = value(a).(bool)
	}
	values := [6][]int{seconds, cronInts(r.Get("minutes")), cronInts(r.Get("hours")), cronInts(r.Get("days")), cronInts(r.Get("months")), cronInts(r.Get("weekdays"))}
	c, message, failed := catchRange(func() *Cron { return makeCron(r.Get("tz").(*TimeZone), values, and) })
	if failed {
		return cronParseError(message)
	}
	return Ok(c)
}

var (
	monthAliases   = map[string]int{"jan": 1, "feb": 2, "mar": 3, "apr": 4, "may": 5, "jun": 6, "jul": 7, "aug": 8, "sep": 9, "oct": 10, "nov": 11, "dec": 12}
	weekdayAliases = map[string]int{"sun": 0, "mon": 1, "tue": 2, "wed": 3, "thu": 4, "fri": 5, "sat": 6}
	decimalText    = regexp.MustCompile(`^\d+$`)
)

// cronNumber is the number of a field's text: an alias's, else NaN unless
// decimal.
func cronNumber(s string, aliases map[string]int) float64 {
	if v, ok := aliases[strings.ToLower(s)]; ok {
		return float64(v)
	}
	if !decimalText.MatchString(s) {
		return nan
	}
	n, _ := strconv.ParseFloat(s, 64)
	return n
}

func isInteger(x float64) bool { return x == math.Trunc(x) && !math.IsInf(x, 0) }

// parseCronSegment parses a segment: the values, whether it starts with a
// wildcard, or the error's message.
func parseCronSegment(input string, f int) ([]int, bool, string) {
	field := cronFields[f]
	max := field.max
	var aliases map[string]int
	switch f {
	case cronMonth:
		aliases = monthAliases
	case cronWeekday:
		aliases = weekdayAliases
	}
	splitStep := func(s string) (string, float64, bool) {
		if raw, step, ok := strings.Cut(s, "/"); ok {
			return raw, cronNumber(step, nil), true
		}
		return s, 0, false
	}
	values := []int{}
	fields := strings.Split(input, ",")
	firstRaw, firstStep, firstHasStep := splitStep(fields[0])
	wildcard := firstRaw == "*"
	add := func(v int) {
		if f == cronWeekday {
			v = normalizeWeekday(v)
		}
		if !slices.Contains(values, v) {
			values = append(values, v)
		}
	}
	if wildcard && (!firstHasStep || firstStep == 1) {
		add = func(int) {}
	}
	between := "Expected a value between " + strconv.Itoa(field.min) + " and " + strconv.Itoa(max)
	for index, text := range fields {
		raw, step, hasStep := firstRaw, firstStep, firstHasStep
		if index > 0 {
			raw, step, hasStep = splitStep(text)
		}
		by := 1
		if hasStep {
			switch {
			case !isInteger(step):
				return nil, false, "Expected step value to be a positive integer"
			case step < 1:
				return nil, false, "Expected step value to be greater than 0"
			case step > float64(max):
				return nil, false, "Expected step value to be less than or equal to " + strconv.Itoa(max)
			}
			by = int(step)
		}
		if raw == "*" {
			if index == 0 && by == 1 {
				continue
			}
			for i := field.min; i <= max; i += by {
				add(i)
			}
			continue
		}
		leftText, rightText, isRange := strings.Cut(raw, "-")
		left := cronNumber(leftText, aliases)
		if !isInteger(left) {
			return nil, false, "Expected a positive integer"
		}
		if left < float64(field.min) || left > float64(max) {
			return nil, false, between
		}
		if !isRange {
			end := int(left)
			if hasStep {
				end = max
			}
			for i := int(left); i <= end; i += by {
				add(i)
			}
			continue
		}
		right := cronNumber(rightText, aliases)
		if !isInteger(right) {
			return nil, false, "Expected a positive integer"
		}
		if right < float64(field.min) || right > float64(max) {
			return nil, false, between
		}
		if left > right {
			return nil, false, "Invalid value range"
		}
		for i := int(left); i <= int(right); i += by {
			add(i)
		}
	}
	return values, wildcard, ""
}

// CronParse is the adapter's: the expression parsed in the zone, an
// error's message followed by the expression.
func CronParse(tz, s Value) Value {
	text := s.(string)
	segments := strings.Fields(text)
	if len(segments) != 5 && len(segments) != 6 {
		return cronParseError("Invalid number of segments in cron expression: " + QuoteJSON(text))
	}
	if len(segments) == 5 {
		segments = append([]string{"0"}, segments...)
	}
	var values [6][]int
	var wildcards [6]bool
	for f, segment := range segments {
		v, wildcard, message := parseCronSegment(segment, f)
		if message != "" {
			return cronParseError(message + ": " + QuoteJSON(text))
		}
		values[f], wildcards[f] = v, wildcard
	}
	and := (wildcards[cronDay] || wildcards[cronWeekday]) && len(values[cronDay]) != 0 && len(values[cronWeekday]) != 0
	return Ok(makeCron(tz.(*TimeZone), values, and))
}

// formatCronSegment is the text of a segment: runs of three or more values
// a step apart as ranges.
func formatCronSegment(values []int) string {
	if len(values) == 0 {
		return "*"
	}
	segments := []string{}
	for index := 0; index < len(values); {
		start := values[index]
		if index+2 < len(values) && values[index+2]-values[index+1] == values[index+1]-start {
			step, end := values[index+1]-start, index+2
			for end+1 < len(values) && values[end+1]-values[end] == step {
				end++
			}
			s := strconv.Itoa(start) + "-" + strconv.Itoa(values[end])
			if step != 1 {
				s += "/" + strconv.Itoa(step)
			}
			segments = append(segments, s)
			index = end + 1
		} else {
			segments = append(segments, strconv.Itoa(start))
			index++
		}
	}
	return strings.Join(segments, ",")
}

func CronFormat(options, cron Value) Value {
	c := cron.(*Cron)
	segments := []string{}
	for _, f := range c.fields {
		segments = append(segments, formatCronSegment(f))
	}
	include := record(options).Get("includeSeconds")
	if !(isSome(include) && value(include).(bool)) && slices.Equal(c.fields[cronSecond], []int{0}) {
		segments = segments[1:]
	}
	return strings.Join(segments, " ")
}

func CronMatch(now, cron Value) Value {
	c := cron.(*Cron)
	date := makeZoned(dateTime(now), c.tz, false, "").toDate()
	parts := [6]int{int(date.seconds()), int(date.minutes()), int(date.hours()), int(date.date()), int(date.month()) + 1, int(date.weekDay())}
	has := func(f int) bool { return slices.Contains(c.fields[f], parts[f]) }
	for _, f := range []int{cronSecond, cronMinute, cronHour, cronMonth} {
		if len(c.fields[f]) != 0 && !has(f) {
			return false
		}
	}
	days, weekdays := len(c.fields[cronDay]) != 0, len(c.fields[cronWeekday]) != 0
	switch {
	case !days && !weekdays:
		return true
	case c.and:
		return (!days || has(cronDay)) && (!weekdays || has(cronWeekday))
	case !weekdays:
		return has(cronDay)
	case !days:
		return has(cronWeekday)
	}
	return has(cronDay) || has(cronWeekday)
}

// CronEquivalence is the same zone, fields and `and`.
func CronEquivalence(a, b Value) Value {
	x, y := a.(*Cron), b.(*Cron)
	if !x.tz.equals(y.tz) || x.and != y.and {
		return false
	}
	for f := range x.fields {
		if !slices.Equal(x.fields[f], y.fields[f]) {
			return false
		}
	}
	return true
}
