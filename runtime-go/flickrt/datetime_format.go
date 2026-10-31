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
// shortcut: a tag outside the tables falls back along its subtags to one in them, or to the default locale, where ICU has data for most languages and regions; Unicode extensions (`-u-hc-h23`, `-u-ca-…`) are ignored; regenerate with more locales when a program needs them
// shortcut: a named zone other than UTC is named by its offset (`GMT+1`, `GMT+01:00`), where ICU names most by their metazone (`CET`, `Central European Standard Time`); generate metazone names when a program formats them

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

func alpha(s string) bool {
	return strings.Trim(s, "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ") == ""
}
func allDigits(s string) bool { return strings.Trim(s, "0123456789") == "" }
func alnum(s string) bool {
	return strings.Trim(s, "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789") == ""
}

// languageTag reads a well-formed Unicode locale identifier, as V8 checks
// one: its language, script and region, canonically cased.
func languageTag(tag string) (language, script, region string, ok bool) {
	subtags := strings.Split(tag, "-")
	n, i := len(subtags), 1
	language = strings.ToLower(subtags[0])
	if l := len(language); !alpha(language) || l < 2 || l == 4 || l > 8 {
		return "", "", "", false
	}
	if i < n && len(subtags[i]) == 4 && alpha(subtags[i]) {
		script = strings.ToUpper(subtags[i][:1]) + strings.ToLower(subtags[i][1:])
		i++
	}
	if i < n && (len(subtags[i]) == 2 && alpha(subtags[i]) || len(subtags[i]) == 3 && allDigits(subtags[i])) {
		region = strings.ToUpper(subtags[i])
		i++
	}
	seen := map[string]bool{}
	for ; i < n; i++ {
		s := strings.ToLower(subtags[i])
		variant := len(s) >= 5 && len(s) <= 8 || len(s) == 4 && s[0] >= '0' && s[0] <= '9'
		if !variant || !alnum(s) || seen[s] {
			break
		}
		seen[s] = true
	}
	for i < n {
		singleton := strings.ToLower(subtags[i])
		if len(singleton) != 1 || !alnum(singleton) || seen[singleton] {
			return "", "", "", false
		}
		seen[singleton] = true
		least := 2
		if singleton == "x" {
			least = 1
		}
		j := i + 1
		for j < n && len(subtags[j]) >= least && len(subtags[j]) <= 8 && alnum(subtags[j]) && (singleton == "x" || len(subtags[j]) > 1) {
			j++
		}
		if j == i+1 {
			return "", "", "", false
		}
		if i = j; singleton == "x" && i < n {
			return "", "", "", false
		}
	}
	return language, script, region, true
}

// localeTable is the table of the locale Intl negotiates for a well-formed
// tag, as far as the tables go.
func localeTable(language, script, region string) (*intlLocale, bool) {
	tag := func(subtags ...string) string {
		out := language
		for _, s := range subtags {
			if s != "" {
				out += "-" + s
			}
		}
		return out
	}
	for _, c := range []string{tag(script, region), tag(region), tag(script), language} {
		if t, ok := intlLocales()[c]; ok {
			return t, true
		}
	}
	return nil, false
}

// defaultLocale is ICU's default locale, from the environment.
func defaultLocale() *intlLocale {
	for _, name := range []string{"LC_ALL", "LC_MESSAGES", "LANG"} {
		if v := os.Getenv(name); v != "" {
			v, _, _ = strings.Cut(v, ".")
			v, _, _ = strings.Cut(v, "@")
			if l, s, r, ok := languageTag(strings.ReplaceAll(v, "_", "-")); ok {
				if t, ok := localeTable(l, s, r); ok {
					return t
				}
			}
			break
		}
	}
	return intlLocales()["en-US"]
}

var styles = []string{"full", "long", "medium", "short"}

// intlFormat is an `Intl.DateTimeFormat` of the adapter's FormatOptions.
type intlFormat struct {
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
	locale := defaultLocale()
	if l := r.Get("locale"); isSome(l) {
		language, script, region, ok := languageTag(value(l).(string))
		if !ok {
			panic(rangeError("Incorrect locale information provided"))
		}
		if t, ok := localeTable(language, script, region); ok {
			locale = t
		}
	}
	return intlFormat{locale, locale.patterns[date*5+time]}
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
			out.WriteString(l.field(pattern[:n], wall, utc, offset))
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

func (l *intlLocale) field(token string, wall *jsDate, utc bool, offset float64) string {
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
	case "z", "zzzz":
		return l.zoneName(utc, offset, token == "zzzz")
	}
	panic(Defect{"no Intl field " + token})
}

// zoneName is the zone's short or long name: UTC's, offset 0's, or a
// template of the offset's.
func (l *intlLocale) zoneName(utc bool, offset float64, long bool) string {
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
