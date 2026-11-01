package flickrt

import (
	"math"
	"os"
	"strconv"
	"strings"
	"unicode/utf8"
)

// Effect's DateTime formatting through `Intl.DateTimeFormat`, on the
// tables tools/go-intl/gen.mjs writes from Node's ICU (datetime_intl.go).
//
// shortcut: the tables have the languages of 52 common locales on the Gregorian calendar, a tag of another falls back to the default locale; Unicode extensions (`-u-hc-h23`, `-u-ca-…`) are ignored; regenerate with more languages when a program needs them
// shortcut: a named zone has its metazone's names at the offsets it has in 2024, by its offset at other times, where ICU follows the metazone's history (Europe/Berlin is `GMT+1` in 1900); generate the history when a program formats old dates

// intlLocale is a locale's table: its patterns, by date and time style,
// and the names they use.
type intlLocale struct {
	patterns                                   []string
	months, monthsWide, days, daysWide, period []string
	zones                                      []string
}

var intlLocalesValue onFirstUse[map[string]*intlLocale]

func intlLocales() map[string]*intlLocale {
	return intlLocalesValue.get(func() map[string]*intlLocale {
		out := map[string]*intlLocale{}
		for _, line := range strings.Split(intlData, "\n") {
			tag, data, _ := strings.Cut(line, "=")
			if other, ok := strings.CutPrefix(data, "@"); ok {
				out[tag] = out[other]
				continue
			}
			f := strings.Split(data, "|")
			items := func(s string) []string { return strings.Split(s, "~") }
			out[tag] = &intlLocale{items(f[0]), items(f[1]), items(f[2]), items(f[3]), items(f[4]), items(f[5]), items(f[6])}
		}
		return out
	})
}

// localeTable is the locale Intl negotiates for a well-formed tag, as far
// as the tables go: as V8 does, the first of the tag without its variants
// and extensions and its shorter prefixes that ICU has data for.
func localeTable(id localeID) (string, bool) {
	language, script, region := id.language, id.script, id.region
	tag := func(subtags ...string) string {
		out := language
		for _, s := range subtags {
			if s != "" {
				out += "-" + s
			}
		}
		return out
	}
	for _, c := range []string{tag(script, region), tag(script), language} {
		if _, ok := intlLocales()[c]; ok {
			return c, true
		}
	}
	return "", false
}

// defaultLocale is ICU's default locale, from the environment.
func defaultLocale() string {
	for _, name := range []string{"LC_ALL", "LC_MESSAGES", "LANG"} {
		if v := os.Getenv(name); v != "" {
			v, _, _ = strings.Cut(v, ".")
			v, _, _ = strings.Cut(v, "@")
			if id, problem := languageTag(strings.ReplaceAll(v, "_", "-")); problem == "" {
				if t, ok := localeTable(id); ok {
					return t
				}
			}
			break
		}
	}
	return "en-US"
}

var styles = []string{"full", "long", "medium", "short"}

// intlFormat is an `Intl.DateTimeFormat` of the adapter's FormatOptions.
type intlFormat struct {
	tag     string
	locale  *intlLocale
	pattern string
}

// newIntlFormat checks the options as the adapter and Intl do, throwing
// their RangeErrors.
func newIntlFormat(o Value) intlFormat {
	r := record(o)
	style := func(name string) int {
		s := r.Get(name)
		if !isSome(s) {
			return 0
		}
		for i, known := range styles {
			if value(s).(string) == known {
				return i + 1
			}
		}
		panic(rangeError("invalid style " + QuoteJSON(value(s).(string))))
	}
	date, time := style("dateStyle"), style("timeStyle")
	tag := defaultLocale()
	if l := r.Get("locale"); isSome(l) {
		if t, ok := localeTable(parseLocale(value(l).(string))); ok {
			tag = t
		}
	}
	locale := intlLocales()[tag]
	return intlFormat{tag, locale, locale.patterns[date*5+time]}
}

// format is the instant `ms` on the wall clock of `zone`, nil for UTC.
func (f intlFormat) format(ms float64, zone *TimeZone) string {
	t := timeClip(ms)
	if math.IsNaN(t) {
		panic(rangeError("Invalid time value"))
	}
	offset, utc := 0.0, zone == nil || zone.loc != nil && zone.id == "UTC"
	if zone != nil {
		offset = (&DateTime{t, zone}).zonedOffset()
	}
	wall := newJsDate(t + offset)
	l := f.locale
	var out strings.Builder
	pattern := f.pattern
	for pattern != "" {
		c := pattern[0]
		switch {
		case c == '\'':
			i := 1
			for ; i < len(pattern); i++ {
				if pattern[i] == '\'' {
					if i+1 == len(pattern) || pattern[i+1] != '\'' {
						break
					}
					i++
				}
				out.WriteByte(pattern[i])
			}
			pattern = pattern[min(i+1, len(pattern)):]
		case c >= 'A' && c <= 'Z' || c >= 'a' && c <= 'z':
			n := len(pattern) - len(strings.TrimLeft(pattern, pattern[:1]))
			if pattern[0] == 'z' {
				out.WriteString(f.zoneName(zone, utc, offset, n == 4))
			} else {
				out.WriteString(l.field(pattern[:n], wall))
			}
			pattern = pattern[n:]
		default:
			_, size := utf8.DecodeRuneInString(pattern)
			out.WriteString(pattern[:size])
			pattern = pattern[size:]
		}
	}
	return out.String()
}

func twoDigits(n int) string {
	if n < 10 {
		return "0" + strconv.Itoa(n)
	}
	return strconv.Itoa(n)
}

func (l *intlLocale) field(token string, wall *jsDate) string {
	year, month, day, hour := int(wall.year()), int(wall.month()), int(wall.date()), int(wall.hours())
	h12 := hour % 12
	switch token {
	case "y":
		return strconv.Itoa(year)
	case "yy":
		return twoDigits((year%100 + 100) % 100)
	case "M":
		return strconv.Itoa(month + 1)
	case "MM":
		return twoDigits(month + 1)
	case "MMM":
		return l.months[month]
	case "MMMM":
		return l.monthsWide[month]
	case "d":
		return strconv.Itoa(day)
	case "dd":
		return twoDigits(day)
	case "EEE":
		return l.days[int(wall.weekDay())]
	case "EEEE":
		return l.daysWide[int(wall.weekDay())]
	case "a":
		return l.period[hour]
	case "h", "hh", "K", "KK", "H", "HH", "k", "kk":
		n := map[byte]int{'h': h12, 'K': h12, 'H': hour, 'k': hour}[token[0]]
		if n == 0 && (token[0] == 'h' || token[0] == 'k') {
			n = map[byte]int{'h': 12, 'k': 24}[token[0]]
		}
		if len(token) == 2 {
			return twoDigits(n)
		}
		return strconv.Itoa(n)
	case "m":
		return strconv.Itoa(int(wall.minutes()))
	case "mm":
		return twoDigits(int(wall.minutes()))
	case "s":
		return strconv.Itoa(int(wall.seconds()))
	case "ss":
		return twoDigits(int(wall.seconds()))
	}
	panic(Defect{"no Intl field " + token})
}

// zoneName is the zone's short or long name: UTC's, a named zone's own,
// offset 0's, or a template of the offset's.
func (f intlFormat) zoneName(zone *TimeZone, utc bool, offset float64, long bool) string {
	if !utc && zone != nil && zone.loc != nil {
		if name := metazoneName(f.tag, zone, offset, long); name != "" {
			return name
		}
	}
	l := f.locale
	b := map[bool]int{false: 0, true: 1}
	minutes := math.Round(offset / 60000)
	switch {
	case utc:
		return l.zones[b[long]]
	case minutes == 0:
		return l.zones[2+b[long]]
	}
	abs := int(math.Abs(minutes))
	h, m := abs/60, abs%60
	template := l.zones[4+2*b[minutes < 0]+b[m != 0]]
	if long {
		template = l.zones[8+b[minutes < 0]]
	}
	return strings.NewReplacer("{HH}", twoDigits(h), "{H}", strconv.Itoa(h), "{MM}", twoDigits(m)).Replace(template)
}

// formatting is a format binding: the adapter's `attempt` of `f`.
func formatting(what string, o, self Value, f func(intlFormat, *DateTime) string) Value {
	return attempt(what, func() Value { return Ok(f(newIntlFormat(o), dateTime(self))) })
}

// DateTimeFormat formats in the date-time's zone, or, when Intl takes no
// zone of its offset (24 hours or more), its wall clock in UTC.
func DateTimeFormat(o, self Value) Value {
	return formatting("format", o, self, func(f intlFormat, d *DateTime) string {
		if d.zone != nil && d.zone.loc == nil {
			if s := offsetToString(d.zone.offset); len(s) > 6 || s[1:3] >= "24" {
				return f.format(d.toDate().t, nil)
			}
		}
		return f.format(d.ms, d.zone)
	})
}

func DateTimeFormatLocal(o, self Value) Value {
	return formatting("format_local", o, self, func(f intlFormat, d *DateTime) string { return f.format(d.ms, localZone()) })
}

func DateTimeFormatUtc(o, self Value) Value {
	return formatting("format_utc", o, self, func(f intlFormat, d *DateTime) string { return f.format(d.ms, nil) })
}

// DateTimeFormatIntl is the adapter's: an `Intl.DateTimeFormat` of the
// options, in the machine's zone.
func DateTimeFormatIntl(o, self Value) Value {
	return formatting("format_intl", o, self, func(f intlFormat, d *DateTime) string { return f.format(d.ms, localZone()) })
}
