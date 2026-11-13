package flickrt

import (
	"math"
	"os"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"
	"sync"
	"time"
	// Node ships its own zone database with ICU; Go reads the system's,
	// which a container or Windows may lack, so the program carries one.
	_ "time/tzdata"
)

// DateTime is Effect's: an instant in epoch milliseconds, in UTC (zone
// nil) or in a time zone. As in Effect the instant may be fractional or
// even NaN after `map_epoch_millis`; reading it as a JS Date truncates.
type DateTime struct {
	millis float64
	zone   *TimeZone
}

// TimeZone is a named zone of the IANA database or a fixed offset in
// milliseconds.
type TimeZone struct {
	id     string
	loc    *time.Location // nil for an offset
	offset float64
}

func dt(v Value) *DateTime { return v.(*DateTime) }
func tz(v Value) *TimeZone { return v.(*TimeZone) }

func utcAt(millis float64) *DateTime { return &DateTime{millis: millis} }

// JS Date arithmetic, as ECMA-262 defines it: time values are float64
// milliseconds since the epoch, NaN for an invalid date.

const msPerDay = 86400000.0

func jsDay(t float64) float64 { return math.Floor(t / msPerDay) }

func jsTimeInDay(t float64) float64 { return posMod(t, msPerDay) }

func posMod(a, b float64) float64 {
	r := math.Mod(a, b)
	if r < 0 {
		r += b
	}
	return r
}

// timeClip is a time value as a Date holds it: whole, within ±8.64e15.
func timeClip(t float64) float64 {
	if math.IsNaN(t) || math.IsInf(t, 0) || math.Abs(t) > 8.64e15 {
		return math.NaN()
	}
	return math.Trunc(t) + 0
}

func daysFromCivil(y, m, d int64) int64 {
	if m <= 2 {
		y--
	}
	era := y
	if era < 0 {
		era -= 399
	}
	era /= 400
	yoe := y - era*400
	mp := m + 9
	if m > 2 {
		mp = m - 3
	}
	doy := (153*mp+2)/5 + d - 1
	doe := yoe*365 + yoe/4 - yoe/100 + doy
	return era*146097 + doe - 719468
}

func civilFromDays(z int64) (y, m, d int64) {
	z += 719468
	era := z
	if era < 0 {
		era -= 146096
	}
	era /= 146097
	doe := z - era*146097
	yoe := (doe - doe/1460 + doe/36524 - doe/146096) / 365
	y = yoe + era*400
	doy := doe - (365*yoe + yoe/4 - yoe/100)
	mp := (5*doy + 2) / 153
	d = doy - (153*mp+2)/5 + 1
	m = mp + 3
	if mp >= 10 {
		m = mp - 9
	}
	if m <= 2 {
		y++
	}
	return y, m, d
}

func finite(xs ...float64) bool {
	for _, x := range xs {
		if math.IsNaN(x) || math.IsInf(x, 0) {
			return false
		}
	}
	return true
}

// makeDay is the day of a year, month (from 0, overflowing into years)
// and date (overflowing into months).
func makeDay(year, month, date float64) float64 {
	if !finite(year, month, date) {
		return math.NaN()
	}
	y, m, d := math.Trunc(year), math.Trunc(month), math.Trunc(date)
	ym := y + math.Floor(m/12)
	if math.Abs(ym) > 1e6 { // far past what a Date holds: TimeClip gives NaN anyway
		return math.NaN()
	}
	return float64(daysFromCivil(int64(ym), int64(posMod(m, 12))+1, 1)) + d - 1
}

func makeTime(h, m, s, ms float64) float64 {
	if !finite(h, m, s, ms) {
		return math.NaN()
	}
	return math.Trunc(h)*3600000 + math.Trunc(m)*60000 + math.Trunc(s)*1000 + math.Trunc(ms)
}

func makeDate(day, t float64) float64 {
	if !finite(day, t) {
		return math.NaN()
	}
	return day*msPerDay + t
}

// date is a JS Date: its getters and setters, UTC ones only.
type date struct{ t float64 }

func (d *date) ymd() (y, m, day float64) {
	if math.IsNaN(d.t) {
		return math.NaN(), math.NaN(), math.NaN()
	}
	cy, cm, cd := civilFromDays(int64(jsDay(d.t)))
	return float64(cy), float64(cm - 1), float64(cd)
}

func (d *date) year() float64         { y, _, _ := d.ymd(); return y }
func (d *date) month() float64        { _, m, _ := d.ymd(); return m }
func (d *date) day() float64          { _, _, x := d.ymd(); return x }
func (d *date) weekDay() float64      { return posMod(jsDay(d.t)+4, 7) }
func (d *date) hours() float64        { return math.Floor(jsTimeInDay(d.t) / 3600000) }
func (d *date) minutes() float64      { return posMod(math.Floor(d.t/60000), 60) }
func (d *date) seconds() float64      { return posMod(math.Floor(d.t/1000), 60) }
func (d *date) milliseconds() float64 { return posMod(d.t, 1000) }

func (d *date) setTime(t float64) { d.t = timeClip(t) }

// setFullYear is setUTCFullYear; the callers pass a part JS leaves out as
// the current one, and so for the other setters.
func (d *date) setFullYear(y, m, day float64) {
	t := d.t
	if math.IsNaN(t) {
		t = 0
	}
	d.t = timeClip(makeDate(makeDay(y, m, day), jsTimeInDay(t)))
}

func (d *date) setMonth(m, day float64) {
	if math.IsNaN(d.t) {
		return
	}
	d.t = timeClip(makeDate(makeDay(d.year(), m, day), jsTimeInDay(d.t)))
}

func (d *date) setDate(day float64) { d.setMonth(d.month(), day) }

func (d *date) setHours(h, m, s, ms float64) {
	if math.IsNaN(d.t) {
		return
	}
	d.t = timeClip(makeDate(jsDay(d.t), makeTime(h, m, s, ms)))
}

func (d *date) setMinutes(m, s, ms float64) { d.setHours(d.hours(), m, s, ms) }
func (d *date) setSeconds(s, ms float64)    { d.setHours(d.hours(), d.minutes(), s, ms) }
func (d *date) setMillis(ms float64)        { d.setHours(d.hours(), d.minutes(), d.seconds(), ms) }

// isoString is toISOString; an invalid date is a RangeError, as in JS.
func (d *date) isoString() string {
	if math.IsNaN(d.t) {
		panic(Defect{"RangeError: Invalid time value"})
	}
	y, m, day := d.ymd()
	var year string
	switch {
	case y >= 0 && y <= 9999:
		year = zeroPad(int(y), 4)
	case y < 0:
		year = "-" + zeroPad(int(-y), 6)
	default:
		year = "+" + zeroPad(int(y), 6)
	}
	return year + "-" + zeroPad(int(m)+1, 2) + "-" + zeroPad(int(day), 2) + "T" + zeroPad(int(d.hours()), 2) + ":" +
		zeroPad(int(d.minutes()), 2) + ":" + zeroPad(int(d.seconds()), 2) + "." + zeroPad(int(d.milliseconds()), 3) + "Z"
}

func zeroPad(n, width int) string {
	s := strconv.Itoa(n)
	for len(s) < width {
		s = "0" + s
	}
	return s
}

// Zones.

var zones sync.Map // id -> *TimeZone, as Effect caches the valid ones

// utcNames are the ids ICU resolves to "UTC".
var utcNames = map[string]bool{
	"utc": true, "etc/utc": true, "gmt": true, "etc/gmt": true, "uct": true, "etc/uct": true,
	"universal": true, "etc/universal": true, "zulu": true, "etc/zulu": true, "greenwich": true,
	"etc/greenwich": true, "gmt0": true, "etc/gmt0": true, "gmt+0": true, "gmt-0": true,
	"etc/gmt+0": true, "etc/gmt-0": true,
}

var offsetId = regexp.MustCompile(`^([+-])(\d{2})(?::?(\d{2}))?$`)

// zoneNamed is the zone of an IANA id, nil for an unknown one.
//
// shortcut: ids are matched case-sensitively and links keep their own id, where ICU matches any case and resolves links (US/Eastern is America/New_York); resolve them if programs read ids back
func zoneNamed(id string) *TimeZone {
	if z, ok := zones.Load(id); ok {
		return z.(*TimeZone)
	}
	name := id
	if utcNames[strings.ToLower(id)] {
		name = "UTC"
	}
	if m := offsetId.FindStringSubmatch(name); m != nil {
		// Intl takes an offset as a zone id, written ±HH:MM
		h, mm := atoi(m[2]), atoi(m[3])
		if h > 23 || mm > 59 {
			return nil
		}
		if h == 0 && mm == 0 {
			m[1] = "+"
		}
		secs := (h*60 + mm) * 60
		if m[1] == "-" {
			secs = -secs
		}
		name = m[1] + zeroPad(h, 2) + ":" + zeroPad(mm, 2)
		z := &TimeZone{id: name, loc: time.FixedZone(name, secs)}
		zones.Store(id, z)
		return z
	}
	if name == "" || name == "Local" || strings.HasPrefix(name, "+") || strings.HasPrefix(name, "-") {
		return nil
	}
	loc, err := time.LoadLocation(name)
	if err != nil {
		return nil
	}
	z := &TimeZone{id: name, loc: loc}
	zones.Store(id, z)
	return z
}

func zoneOffset(offset float64) *TimeZone { return &TimeZone{offset: offset} }

// localZone is the machine's zone as Intl names it: TZ, else the target of
// /etc/localtime, else UTC.
func localZone() *TimeZone {
	name := strings.TrimPrefix(os.Getenv("TZ"), ":")
	if name == "" {
		if target, err := filepath.EvalSymlinks("/etc/localtime"); err == nil {
			if _, rest, ok := strings.Cut(target, "zoneinfo/"); ok {
				name = rest
			}
		}
	}
	if z := zoneNamed(name); z != nil {
		return z
	}
	return zoneNamed("UTC")
}

// offsetString is Effect's offsetToString: `+01:00`.
func offsetString(offset float64) string {
	abs := math.Abs(offset)
	hours := math.Floor(abs / 3600000)
	minutes := math.Round(posMod(abs, 3600000) / 60000)
	if minutes == 60 {
		hours++
		minutes = 0
	}
	sign := "+"
	if offset < 0 {
		sign = "-"
	}
	return sign + zeroPad(int(hours), 2) + ":" + zeroPad(int(minutes), 2)
}

// parseOffset is Effect's: the offset a string ends with, `±HH:MM`.
func parseOffset(s string) (float64, bool) {
	n := len(s)
	if n < 6 || s[n-3] != ':' || s[n-6] != '+' && s[n-6] != '-' || !isDigits(s[n-5:n-3]) || !isDigits(s[n-2:]) {
		return 0, false
	}
	v := (float64(atoi(s[n-5:n-3]))*60 + float64(atoi(s[n-2:]))) * 60000
	if s[n-6] == '-' {
		v = -v
	}
	return v, true
}

func isDigits(s string) bool {
	for i := 0; i < len(s); i++ {
		if s[i] < '0' || s[i] > '9' {
			return false
		}
	}
	return s != ""
}

func atoi(s string) int {
	n := 0
	for i := 0; i < len(s); i++ {
		n = n*10 + int(s[i]-'0')
	}
	return n
}

// zoneFromString is Effect's: an offset (`GMT`, `+…` or `-…`) or an id.
func zoneFromString(s string) *TimeZone {
	if strings.HasPrefix(s, "GMT") || strings.HasPrefix(s, "+") || strings.HasPrefix(s, "-") {
		if o, ok := parseOffset(s); ok {
			return zoneOffset(o)
		}
		return nil
	}
	return zoneNamed(s)
}

func (z *TimeZone) String() string {
	if z.loc == nil {
		return offsetString(z.offset)
	}
	return z.id
}

func (z *TimeZone) equal(o *TimeZone) bool {
	if z == nil || o == nil {
		return z == o
	}
	if (z.loc == nil) != (o.loc == nil) {
		return false
	}
	if z.loc == nil {
		return z.offset == o.offset
	}
	return z.id == o.id
}

// namedOffset is the offset of a named zone at an instant, in ms.
func (z *TimeZone) namedOffset(millis float64) float64 {
	_, off := time.UnixMilli(int64(timeClip(millis))).In(z.loc).Zone()
	return float64(off) * 1000
}

// wall is Effect's toDate: the wall-clock time of the zone as a UTC date.
func (t *DateTime) wall() *date {
	switch {
	case t.zone == nil:
		return &date{timeClip(t.millis)}
	case t.zone.loc == nil:
		return &date{timeClip(t.millis + t.zone.offset)}
	}
	if math.IsNaN(t.millis) {
		panic(Defect{"RangeError: Invalid time value"})
	}
	return &date{timeClip(t.millis) + t.zone.namedOffset(t.millis)}
}

// zonedOffset is the zone's offset at the date-time, as Effect computes it.
func (t *DateTime) zonedOffset() float64 { return t.wall().t - t.millis }

// calculateNamedOffset is Effect's: the offset Intl names at `utc`; one of
// seconds, which Intl does not name as ±HH:MM, is the offset at `adjusted`.
func (z *TimeZone) calculateNamedOffset(utc, adjusted float64) float64 {
	if o := z.namedOffset(utc); math.Mod(o, 60000) == 0 {
		return o
	}
	return (&DateTime{millis: adjusted, zone: z}).zonedOffset()
}

// fromAdjusted is Effect's makeZonedFromAdjusted: the instant whose wall
// clock in the zone is `adjusted`, a skipped or repeated one chosen by
// `disambiguation`; nil for a rejected one, with the reason.
func fromAdjusted(adjusted float64, zone *TimeZone, disambiguation string) (*DateTime, string) {
	at := func(m float64) *DateTime { return &DateTime{millis: m, zone: zone} }
	if zone.loc == nil {
		return at(adjusted - zone.offset), ""
	}
	before := zone.calculateNamedOffset(adjusted-msPerDay, adjusted)
	after := zone.calculateNamedOffset(adjusted+msPerDay, adjusted)
	if before == after {
		return at(adjusted - before), ""
	}
	transition := before - after
	if before < after {
		if zone.calculateNamedOffset(adjusted-after, adjusted) == after {
			return at(adjusted - after), ""
		}
		b := at(adjusted - before)
		if adjusted != b.wall().t {
			switch disambiguation {
			case "reject":
				return nil, "Gap time: " + (&date{adjusted}).isoString() + " does not exist in time zone " + zone.id
			case "earlier":
				return at(adjusted - after), ""
			}
		}
		return b, ""
	}
	if zone.calculateNamedOffset(adjusted-before, adjusted) == before {
		if disambiguation == "earlier" || disambiguation == "compatible" {
			return at(adjusted - before), ""
		}
		if zone.calculateNamedOffset(adjusted-before+transition, adjusted+transition) == before {
			return at(adjusted - before), ""
		}
		if disambiguation == "reject" {
			return nil, "Ambiguous time: " + (&date{adjusted}).isoString() + " occurs twice in time zone " + zone.id
		}
	}
	return at(adjusted - after), ""
}

// mutate is Effect's: `f` changes the date-time's wall-clock date, which
// is placed back in the zone the compatible way.
func (t *DateTime) mutate(f func(*date)) *DateTime {
	if t.zone == nil {
		d := &date{timeClip(t.millis)}
		f(d)
		return utcAt(d.t)
	}
	d := t.wall()
	f(d)
	out, _ := fromAdjusted(d.t, t.zone, "compatible")
	return out
}

// mutateUtc changes the UTC date, keeping the zone.
func (t *DateTime) mutateUtc(f func(*date)) *DateTime {
	d := &date{timeClip(t.millis)}
	f(d)
	return &DateTime{millis: d.t, zone: t.zone}
}

func partsOf(d *date) Value {
	y, m, day := d.ymd()
	return Rec("millisecond", int32(d.milliseconds()), "second", int32(d.seconds()), "minute", int32(d.minutes()),
		"hour", int32(d.hours()), "day", int32(day), "weekDay", int32(d.weekDay()), "month", int32(m+1), "year", int32(y))
}
