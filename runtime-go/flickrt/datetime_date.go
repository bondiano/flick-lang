package flickrt

import (
	"fmt"
	"math"
	"regexp"
	"strconv"
	"strings"
	"time"
)

// jsDate is a JS `Date`: a time value, epoch milliseconds or NaN, and the
// UTC getters and setters of ECMAScript that Effect's DateTime and Cron
// compute through, so that overflowing parts roll over as they do in JS.
type jsDate struct{ t float64 }

const (
	msPerDay     = 86400000.0
	msPerHour    = 3600000.0
	msPerMinute  = 60000.0
	maxTimeValue = 8.64e15
)

var nan = math.NaN()

// posMod is ECMAScript's `modulo`: the sign of the divisor.
func posMod(a, b float64) float64 {
	r := math.Mod(a, b)
	if r < 0 {
		r += b
	}
	return r + 0
}

func floorDiv(a, b int64) int64 {
	q := a / b
	if a%b != 0 && (a < 0) != (b < 0) {
		q--
	}
	return q
}

// civil is the proleptic Gregorian date of a day since 1970-01-01.
func civil(days int64) (year int64, month, day int) {
	z := days + 719468
	era := floorDiv(z, 146097)
	doe := z - era*146097
	yoe := (doe - doe/1460 + doe/36524 - doe/146096) / 365
	doy := doe - (365*yoe + yoe/4 - yoe/100)
	mp := (5*doy + 2) / 153
	day = int(doy - (153*mp+2)/5 + 1)
	month = int(mp + 3)
	if month > 12 {
		month -= 12
	}
	year = yoe + era*400
	if month <= 2 {
		year++
	}
	return
}

// daysFromCivil is the day since 1970-01-01 of a proleptic Gregorian date.
func daysFromCivil(year int64, month, day int) int64 {
	if month <= 2 {
		year--
	}
	era := floorDiv(year, 400)
	yoe := year - era*400
	doy := int64((153*((month+9)%12)+2)/5 + day - 1)
	return era*146097 + yoe*365 + yoe/4 - yoe/100 + doy - 719468
}

// makeDay, makeTime, makeDate and timeClip are ECMAScript's.
func makeDay(year, month, date float64) float64 {
	if !finite(year) || !finite(month) || !finite(date) {
		return nan
	}
	m := math.Trunc(month)
	ym := math.Trunc(year) + math.Floor(m/12)
	if math.Abs(ym) > 1e9 { // far past any time value
		return nan
	}
	return float64(daysFromCivil(int64(ym), int(posMod(m, 12))+1, 1)) + math.Trunc(date) - 1
}

func makeTime(hour, minute, second, ms float64) float64 {
	if !finite(hour) || !finite(minute) || !finite(second) || !finite(ms) {
		return nan
	}
	return math.Trunc(hour)*msPerHour + math.Trunc(minute)*msPerMinute + math.Trunc(second)*1000 + math.Trunc(ms)
}

func makeDate(day, time float64) float64 {
	if !finite(day) || !finite(time) {
		return nan
	}
	return day*msPerDay + time
}

func timeClip(t float64) float64 {
	if !finite(t) || math.Abs(t) > maxTimeValue {
		return nan
	}
	return math.Trunc(t) + 0
}

// newJsDate is `new Date(ms)`.
func newJsDate(ms float64) *jsDate { return &jsDate{timeClip(ms)} }

func (d *jsDate) civil() (int64, int, int) { return civil(int64(math.Floor(d.t / msPerDay))) }

// The UTC getters; each is NaN for an invalid date.
func (d *jsDate) year() float64 {
	if math.IsNaN(d.t) {
		return nan
	}
	y, _, _ := d.civil()
	return float64(y)
}

// month counts from 0, as JS's does.
func (d *jsDate) month() float64 {
	if math.IsNaN(d.t) {
		return nan
	}
	_, m, _ := d.civil()
	return float64(m - 1)
}

func (d *jsDate) date() float64 {
	if math.IsNaN(d.t) {
		return nan
	}
	_, _, day := d.civil()
	return float64(day)
}

func (d *jsDate) weekDay() float64 { return posMod(math.Floor(d.t/msPerDay)+4, 7) }
func (d *jsDate) hours() float64   { return posMod(math.Floor(d.t/msPerHour), 24) }
func (d *jsDate) minutes() float64 { return posMod(math.Floor(d.t/msPerMinute), 60) }
func (d *jsDate) seconds() float64 { return posMod(math.Floor(d.t/1000), 60) }
func (d *jsDate) millis() float64  { return posMod(d.t, 1000) }
func (d *jsDate) withinDay() float64 {
	return posMod(d.t, msPerDay)
}

// arg is an optional argument of a setter, or the part it defaults to.
func arg(rest []float64, i int, part func() float64) float64 {
	if i < len(rest) {
		return rest[i]
	}
	return part()
}

// The UTC setters, with JS's optional trailing arguments.
func (d *jsDate) setFullYear(year float64, rest ...float64) {
	if math.IsNaN(d.t) {
		d.t = 0
	}
	day := makeDay(year, arg(rest, 0, d.month), arg(rest, 1, d.date))
	d.t = timeClip(makeDate(day, d.withinDay()))
}

func (d *jsDate) setMonth(month float64, rest ...float64) {
	if math.IsNaN(d.t) {
		return
	}
	day := makeDay(d.year(), month, arg(rest, 0, d.date))
	d.t = timeClip(makeDate(day, d.withinDay()))
}

func (d *jsDate) setDate(date float64) {
	if math.IsNaN(d.t) {
		return
	}
	d.t = timeClip(makeDate(makeDay(d.year(), d.month(), date), d.withinDay()))
}

func (d *jsDate) setTimeOfDay(hour, minute, second, ms float64) {
	d.t = timeClip(makeDate(math.Floor(d.t/msPerDay), makeTime(hour, minute, second, ms)))
}

func (d *jsDate) setHours(hour float64, rest ...float64) {
	if math.IsNaN(d.t) {
		return
	}
	d.setTimeOfDay(hour, arg(rest, 0, d.minutes), arg(rest, 1, d.seconds), arg(rest, 2, d.millis))
}

func (d *jsDate) setMinutes(minute float64, rest ...float64) {
	if math.IsNaN(d.t) {
		return
	}
	d.setTimeOfDay(d.hours(), minute, arg(rest, 0, d.seconds), arg(rest, 1, d.millis))
}

func (d *jsDate) setSeconds(second float64, rest ...float64) {
	if math.IsNaN(d.t) {
		return
	}
	d.setTimeOfDay(d.hours(), d.minutes(), second, arg(rest, 0, d.millis))
}

func (d *jsDate) setMilliseconds(ms float64) {
	if math.IsNaN(d.t) {
		return
	}
	d.setTimeOfDay(d.hours(), d.minutes(), d.seconds(), ms)
}

func (d *jsDate) setTime(t float64) { d.t = timeClip(t) }

// isoString is JS `toISOString`, which throws for an invalid date.
func (d *jsDate) isoString() string {
	if math.IsNaN(d.t) {
		panic(rangeError("Invalid time value"))
	}
	y, m, day := d.civil()
	year := fmt.Sprintf("%04d", y)
	if y < 0 || y > 9999 {
		year = fmt.Sprintf("%+07d", y)
	}
	return fmt.Sprintf("%s-%02d-%02dT%02d:%02d:%02d.%03dZ", year, m, day,
		int(d.hours()), int(d.minutes()), int(d.seconds()), int(d.millis()))
}

// isoText is ECMAScript's date time string format, with V8's leniency
// about the separator and the case of `T` and `Z`.
var isoText = regexp.MustCompile(`^([+-]\d{6}|\d{4})(?:-(\d{2})(?:-(\d{2}))?)?` +
	`(?:[Tt ](\d{2}):(\d{2})(?::(\d{2})(?:\.(\d+))?)?)?([Zz]|[+-]\d{2}:?\d{2})?$`)

// legacyLayouts are some of the other forms V8's `Date.parse` reads.
//
// shortcut: V8's legacy date parser reads many more forms (`3/10/2024`, month names in any place, zone abbreviations); port it when a program parses them
var legacyLayouts = []string{"2006-1-2", "2006/1/2", "2006/1/2 15:04:05", "January 2, 2006", "Jan 2, 2006", time.RFC1123}

// parseJsDate is `new Date(s).getTime()` of a string: NaN when V8 reads no
// date in it; one without an offset is UTC, as Effect always gives one.
func parseJsDate(s string) float64 {
	m := isoText.FindStringSubmatch(s)
	if m == nil {
		return parseLegacy(s)
	}
	num := func(s string, def float64) float64 {
		if s == "" {
			return def
		}
		n, _ := strconv.ParseFloat(s, 64)
		return n
	}
	if m[1] == "-000000" {
		return nan
	}
	year, month, day := num(m[1], 0), num(m[2], 1), num(m[3], 1)
	hour, minute, second := num(m[4], 0), num(m[5], 0), num(m[6], 0)
	ms := 0.0
	if f := m[7]; f != "" {
		ms = num((f + "00")[:3], 0)
	}
	if month < 1 || month > 12 || day < 1 || day > 31 || minute > 59 || second > 59 ||
		hour > 24 || hour == 24 && (minute > 0 || second > 0 || ms > 0) {
		return nan
	}
	offset := 0.0
	if z := m[8]; len(z) > 1 {
		digits := strings.ReplaceAll(z[1:], ":", "")
		h, mm := num(digits[:2], 0), num(digits[2:], 0)
		if h > 23 || mm > 59 {
			return nan
		}
		offset = (h*60 + mm) * msPerMinute
		if z[0] == '-' {
			offset = -offset
		}
	}
	return timeClip(makeDate(makeDay(year, month-1, day), makeTime(hour, minute, second, ms)) - offset)
}

func parseLegacy(s string) float64 {
	trimmed := strings.TrimRight(s, "Zz ")
	for _, layout := range legacyLayouts {
		if t, err := time.Parse(layout, trimmed); err == nil {
			return timeClip(float64(t.UnixMilli()))
		}
	}
	return nan
}
