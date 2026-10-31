package flickrt

import (
	"fmt"
	"math"
	"os"
	"regexp"
	"strconv"
	"strings"
	"sync"
	"time"
)

// TimeZone is Effect's: a named IANA zone or a fixed offset. Named zones
// come from Go's time zone database, which a program that reaches them
// embeds (`time/tzdata`), under the name V8 resolves each to.
//
// shortcut: Intl also takes an offset such as `+01:00` as a zone's name; take one when a program names a zone so
type TimeZone struct {
	id     string         // named: the IANA identifier
	loc    *time.Location // named; nil for an offset
	offset float64        // an offset: milliseconds east of UTC
}

// UseZones is called first by a program that reaches named zones, which
// also embeds the zone database: names then resolve as V8 resolves them.
// Other programs do not carry the table of names.
func UseZones() { resolveZone = zoneTable }

// resolveZone is the name V8 resolves a zone's name to, if it takes it.
var resolveZone = func(id string) (string, bool) { return id, true }

// onFirstUse is a value made on first use: unlike sync.OnceValue's, by no code
// at init, so a table only links into a program that uses it.
type onFirstUse[T any] struct {
	once  sync.Once
	value T
}

func (l *onFirstUse[T]) get(make func() T) T {
	l.once.Do(func() { l.value = make() })
	return l.value
}

var zoneNames onFirstUse[map[string]string]

func zoneTable(id string) (string, bool) {
	resolved, ok := zoneNames.get(func() map[string]string {
		out := map[string]string{}
		for _, entry := range strings.Split(zoneData, ";") {
			name, resolved, aliased := strings.Cut(entry, ">")
			if !aliased {
				resolved = name
			}
			out[strings.ToLower(name)] = resolved
		}
		return out
	})[strings.ToLower(id)]
	return resolved, ok
}

var zoneCache sync.Map // resolved id → *TimeZone

// zoneNamed is Effect's `zoneMakeNamed`.
func zoneNamed(id string) (*TimeZone, bool) {
	resolved, ok := resolveZone(id)
	if !ok || resolved == "" || resolved == "Local" {
		return nil, false
	}
	if z, ok := zoneCache.Load(resolved); ok {
		return z.(*TimeZone), true
	}
	loc, err := time.LoadLocation(resolved)
	if err != nil {
		return nil, false
	}
	z, _ := zoneCache.LoadOrStore(resolved, &TimeZone{id: resolved, loc: loc})
	return z.(*TimeZone), true
}

var localZoneValue onFirstUse[*TimeZone]

// localZone is Effect's `zoneMakeLocal`: the machine's zone, named as ICU
// names it from `TZ` or `/etc/localtime` (`Etc/UTC` as `UTC`), with the
// machine's rules, so that it needs no zone database of its own.
func localZone() *TimeZone {
	return localZoneValue.get(func() *TimeZone {
		name := strings.TrimPrefix(os.Getenv("TZ"), ":")
		if name == "" {
			if link, err := os.Readlink("/etc/localtime"); err == nil {
				if _, after, ok := strings.Cut(link, "zoneinfo/"); ok {
					name = after
				}
			}
		}
		if resolved, ok := zoneTable(name); ok {
			name = resolved
		} else {
			name = "UTC"
		}
		return &TimeZone{id: name, loc: time.Local}
	})
}

var offsetText = regexp.MustCompile(`([+-])(\d{2}):(\d{2})$`)

// parseOffset is Effect's: `+hh:mm` at the end of `s`, in milliseconds.
func parseOffset(s string) (float64, bool) {
	m := offsetText.FindStringSubmatch(s)
	if m == nil {
		return 0, false
	}
	h, _ := strconv.Atoi(m[2])
	mm, _ := strconv.Atoi(m[3])
	sign := 1.0
	if m[1] == "-" {
		sign = -1
	}
	return sign * float64((h*60+mm)*60*1000), true
}

// zoneFromString is Effect's `zoneFromString`: an offset, or an IANA name.
func zoneFromString(s string) (*TimeZone, bool) {
	if strings.HasPrefix(s, "GMT") || strings.HasPrefix(s, "+") || strings.HasPrefix(s, "-") {
		off, ok := parseOffset(s)
		return &TimeZone{offset: off}, ok
	}
	return zoneNamed(s)
}

// String is Effect's `zoneToString`.
func (z *TimeZone) String() string {
	if z.loc != nil {
		return z.id
	}
	return offsetToString(z.offset)
}

func (z *TimeZone) equals(o *TimeZone) bool {
	if z.loc != nil || o.loc != nil {
		return z.loc != nil && o.loc != nil && z.id == o.id
	}
	return z.offset == o.offset
}

// namedOffset is a named zone's offset at an instant, in milliseconds, as
// Intl formats the instant: it throws for an invalid one.
func (z *TimeZone) namedOffset(ms float64) float64 {
	t := timeClip(ms)
	if math.IsNaN(t) {
		panic(rangeError("Invalid time value"))
	}
	_, seconds := time.UnixMilli(int64(t)).In(z.loc).Zone()
	return float64(seconds) * 1000
}

func offsetToString(offset float64) string {
	abs := math.Abs(offset)
	hours := math.Floor(abs / 3600000)
	minutes := math.Round(math.Mod(abs, 3600000) / 60000)
	if minutes == 60 {
		hours, minutes = hours+1, 0
	}
	sign := "+"
	if offset < 0 {
		sign = "-"
	}
	return fmt.Sprintf("%s%02d:%02d", sign, int(hours), int(minutes))
}

// calculateNamedOffset is Effect's: the offset Intl names at `utc`, or,
// when it names one with seconds, the offset at `adjusted` as a wall clock.
func (z *TimeZone) calculateNamedOffset(utc, adjusted float64) float64 {
	offset := z.namedOffset(utc)
	if math.Mod(offset, 60000) != 0 {
		return z.namedOffset(adjusted) + timeClip(adjusted) - adjusted
	}
	return offset
}

// makeZonedFromAdjusted is Effect's: the date-time whose wall clock in the
// zone is `adjusted` read as UTC, with a time a transition skips or repeats
// placed by the disambiguation.
func makeZonedFromAdjusted(adjusted float64, zone *TimeZone, disambiguation string) *DateTime {
	if zone.loc == nil {
		return &DateTime{adjusted - zone.offset, zone}
	}
	before := zone.calculateNamedOffset(adjusted-msPerDay, adjusted)
	after := zone.calculateNamedOffset(adjusted+msPerDay, adjusted)
	if before == after {
		return &DateTime{adjusted - before, zone}
	}
	if before < after {
		if zone.calculateNamedOffset(adjusted-after, adjusted) == after {
			return &DateTime{adjusted - after, zone}
		}
		earlier := &DateTime{adjusted - before, zone}
		if adjusted != earlier.toDate().t {
			switch disambiguation {
			case "reject":
				panic(rangeError(fmt.Sprintf("Gap time: %s does not exist in time zone %s", newJsDate(adjusted).isoString(), zone.id)))
			case "earlier":
				return &DateTime{adjusted - after, zone}
			}
		}
		return earlier
	}
	if zone.calculateNamedOffset(adjusted-before, adjusted) == before {
		if disambiguation == "earlier" || disambiguation == "compatible" {
			return &DateTime{adjusted - before, zone}
		}
		transition := before - after
		if zone.calculateNamedOffset(adjusted-before+transition, adjusted+transition) == before {
			return &DateTime{adjusted - before, zone}
		}
		if disambiguation == "reject" {
			panic(rangeError(fmt.Sprintf("Ambiguous time: %s occurs twice in time zone %s", newJsDate(adjusted).isoString(), zone.id)))
		}
	}
	return &DateTime{adjusted - after, zone}
}

// rangeError is a JS RangeError that Effect throws: a binding that catches
// it, as the JS adapter's do, gives its message; uncaught it is a defect.
type rangeError string

func (e rangeError) Error() string { return "RangeError: " + string(e) }

// catchRange is `f()`, or the message of the RangeError it throws.
func catchRange[A any](f func() A) (out A, message string, failed bool) {
	defer func() {
		if r := recover(); r != nil {
			e, ok := r.(rangeError)
			if !ok {
				panic(r)
			}
			message, failed = string(e), true
		}
	}()
	return f(), "", false
}
