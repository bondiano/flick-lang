package flickrt

import (
	"math"
	"regexp"
	"strconv"
	"strings"
	"time"
)

// Formatting as Node's `Intl.DateTimeFormat` with a locale and the date
// and time styles: the English patterns of CLDR, US or British.
//
// shortcut: other locales format as en-US, and a named zone other than UTC is named by its offset (GMT+1, GMT+01:00) where CLDR has a name (EST, Central European Standard Time); add CLDR data if programs format for people in other locales

var localeTag = regexp.MustCompile(`(?i)^(?:[a-z]{2,3}|[a-z]{5,8})(?:-[a-z]{4})?(?:-(?:[a-z]{2}|\d{3}))?(?:-(?:[a-z\d]{5,8}|\d[a-z\d]{3}))*(?:-[\da-wyz](?:-[a-z\d]{2,8})+)*(?:-x(?:-[a-z\d]{1,8})+)?$`)

var britishLocale = regexp.MustCompile(`(?i)^en(?:-[a-z]{4})?-gb(?:-|$)`)

var (
	weekdayNames = []string{"Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"}
	monthNames   = []string{"January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"}
)

// intlStyle is a style option, or the error the adapter throws.
func intlStyle(o Value, key string) (string, string) {
	s := record(o).Get(key)
	if !isSome(s) {
		return "", ""
	}
	switch v := value(s).(string); v {
	case "full", "long", "medium", "short":
		return v, ""
	default:
		return "", "invalid style " + QuoteJSON(v)
	}
}

// intlFormat formats the instant in a zone (nil: the machine's), or gives
// why Intl throws, prefixed with `what`.
func intlFormat(what string, o Value, millis float64, zone *TimeZone) Value {
	dateStyle, bad := intlStyle(o, "dateStyle")
	if bad == "" {
		var timeStyle string
		timeStyle, bad = intlStyle(o, "timeStyle")
		if bad == "" {
			bad = formatted(o, dateStyle, timeStyle, millis, zone)
			if !strings.HasPrefix(bad, "\x00") {
				return Ok(bad)
			}
			bad = bad[1:]
		}
	}
	return dateTimeError(what + ": " + bad)
}

// formatted is the text, or "\x00" and Intl's error.
func formatted(o Value, dateStyle, timeStyle string, millis float64, zone *TimeZone) string {
	british := false
	if l := record(o).Get("locale"); isSome(l) {
		tag := value(l).(string)
		if !localeTag.MatchString(tag) {
			return "\x00Incorrect locale information provided"
		}
		british = britishLocale.MatchString(tag)
	}
	t := timeClip(millis)
	if math.IsNaN(t) {
		return "\x00Invalid time value"
	}
	wall, offset := wallIn(t, zone)
	d := &date{wall}
	y, m, day := d.ymd()
	year, month, dom := strconv.Itoa(int(y)), int(m), strconv.Itoa(int(day))
	var datePart string
	switch {
	case dateStyle == "full" && british:
		datePart = weekdayNames[int(d.weekDay())] + ", " + dom + " " + monthNames[month] + " " + year
	case dateStyle == "full":
		datePart = weekdayNames[int(d.weekDay())] + ", " + monthNames[month] + " " + dom + ", " + year
	case dateStyle == "long" && british:
		datePart = dom + " " + monthNames[month] + " " + year
	case dateStyle == "long":
		datePart = monthNames[month] + " " + dom + ", " + year
	case dateStyle == "medium" && british:
		datePart = dom + " " + monthNames[month][:3] + " " + year
	case dateStyle == "medium":
		datePart = monthNames[month][:3] + " " + dom + ", " + year
	case (dateStyle == "short" || timeStyle == "") && british:
		datePart = zeroPad(int(day), 2) + "/" + zeroPad(month+1, 2) + "/" + year
	case dateStyle == "short":
		datePart = strconv.Itoa(month+1) + "/" + dom + "/" + zeroPad(int(posMod(y, 100)), 2)
	case timeStyle == "":
		datePart = strconv.Itoa(month+1) + "/" + dom + "/" + year
	}
	if timeStyle == "" {
		return datePart
	}
	h, mm, ss := int(d.hours()), zeroPad(int(d.minutes()), 2), zeroPad(int(d.seconds()), 2)
	clock := zeroPad(h, 2) + ":" + mm
	if timeStyle != "short" {
		clock += ":" + ss
	}
	if !british {
		hour12, half := h%12, "AM"
		if hour12 == 0 {
			hour12 = 12
		}
		if h >= 12 {
			half = "PM"
		}
		clock = strconv.Itoa(hour12) + clock[2:] + " " + half
	}
	switch timeStyle {
	case "long":
		clock += " " + shortZoneName(offset, zone)
	case "full":
		clock += " " + longZoneName(offset, zone)
	}
	switch {
	case datePart == "":
		return clock
	case dateStyle == "full" || dateStyle == "long":
		return datePart + " at " + clock
	}
	return datePart + ", " + clock
}

// wallIn is the wall-clock time of an instant in a zone and its offset.
func wallIn(t float64, zone *TimeZone) (float64, float64) {
	var offset float64
	switch {
	case zone == nil:
		_, off := time.UnixMilli(int64(t)).In(time.Local).Zone()
		offset = float64(off) * 1000
	case zone.loc == nil:
		// Intl reads an offset zone from its text, to the minute
		parsed, _ := parseOffset(offsetString(zone.offset))
		offset = parsed
	default:
		offset = zone.namedOffset(t)
	}
	return t + offset, offset
}

func isUtcZone(zone *TimeZone) bool { return zone != nil && zone.loc != nil && zone.id == "UTC" }

// shortZoneName is CLDR's short name: UTC, GMT, or GMT+5:30.
func shortZoneName(offset float64, zone *TimeZone) string {
	if isUtcZone(zone) {
		return "UTC"
	}
	if offset == 0 {
		return "GMT"
	}
	text := offsetString(offset)
	hours, minutes := strings.TrimLeft(text[1:3], "0"), text[4:]
	if hours == "" {
		hours = "0"
	}
	if minutes == "00" {
		return "GMT" + text[:1] + hours
	}
	return "GMT" + text[:1] + hours + ":" + minutes
}

// longZoneName is CLDR's long name of UTC and GMT, else GMT+01:00.
func longZoneName(offset float64, zone *TimeZone) string {
	switch {
	case isUtcZone(zone):
		return "Coordinated Universal Time"
	case offset == 0:
		return "Greenwich Mean Time"
	}
	return "GMT" + offsetString(offset)
}

// intlZone is the zone Effect's `format` gives Intl: the date-time's.
func intlZone(t *DateTime) *TimeZone {
	if t.zone == nil {
		return zoneNamed("UTC")
	}
	return t.zone
}

func DateTimeFormat(o, t Value) Value {
	return intlFormat("format", o, dt(t).millis, intlZone(dt(t)))
}

func DateTimeFormatLocal(o, t Value) Value { return intlFormat("format_local", o, dt(t).millis, nil) }

func DateTimeFormatUtc(o, t Value) Value {
	return intlFormat("format_utc", o, dt(t).millis, zoneNamed("UTC"))
}

func DateTimeFormatIntl(o, t Value) Value { return intlFormat("format_intl", o, dt(t).millis, nil) }
