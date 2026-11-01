package flickrt

import (
	"math"
	"strings"
	"time"
)

// The names ICU gives named zones by their metazone (`CET`, `Central
// European Standard Time`, `MEZ`), from the tables of datetime_intl.go.

var zoneGroups onFirstUse[map[string]int]
var shortZoneNames, longZoneNames onFirstUse[map[string][]string]

// zoneNameLines reads shortZoneData or longZoneData: each locale's names
// of the zone groups.
func zoneNameLines(data string) map[string][]string {
	out := map[string][]string{}
	for _, line := range strings.Split(data, "\n") {
		tag, names, _ := strings.Cut(line, "=")
		if other, ok := strings.CutPrefix(names, "@"); ok {
			out[tag] = out[other]
			continue
		}
		out[tag] = strings.Split(names, "|")
	}
	return out
}

// metazoneName is the locale's short or long name of a named zone at an
// offset, if the zone has one of its own there.
func metazoneName(tag string, zone *TimeZone, offset float64, long bool) string {
	group, ok := zoneGroups.get(func() map[string]int {
		out := map[string]int{}
		for i, zones := range strings.Split(zoneGroupData, ";") {
			for _, z := range strings.Split(zones, ",") {
				out[z] = i
			}
		}
		return out
	})[zone.id]
	if !ok {
		return ""
	}
	names := shortZoneNames.get(func() map[string][]string { return zoneNameLines(shortZoneData) })
	if long {
		names = longZoneNames.get(func() map[string][]string { return zoneNameLines(longZoneData) })
	}
	winter, summer, two := strings.Cut(names[tag][group], "~")
	if !two {
		summer = winter
	}
	// the probes of the generator
	_, w := time.Date(2024, 1, 15, 12, 0, 0, 0, time.UTC).In(zone.loc).Zone()
	_, s := time.Date(2024, 7, 15, 12, 0, 0, 0, time.UTC).In(zone.loc).Zone()
	switch int(math.Round(offset / 60000)) {
	case w / 60:
		return winter
	case s / 60:
		return summer
	}
	return ""
}
