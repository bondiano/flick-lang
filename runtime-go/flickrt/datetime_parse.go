package flickrt

import (
	"math"
	"time"
	"unicode"
)

// parseJsDate is V8's `Date.parse` (src/date/dateparser*, date.cc): an ES
// date-time string, or one of the legacy forms V8 reads as Safari does;
// NaN for none. A date alone, or a time with a zone, is UTC; a date-time
// without one is a wall-clock time of the local zone, `time.Local`.
func parseJsDate(s string) float64 {
	sc := newDateScanner(s)
	var day dayComposer
	var clock timeComposer
	var tz zoneComposer
	day.namedMonth, clock.hourOffset = dateNone, dateNone
	tz.sign, tz.hour, tz.minute = dateNone, dateNone, dateNone

	token := parseES5Date(sc, &day, &clock, &tz)
	if token.tag == tokInvalid {
		return nan
	}
	hasNumber := day.index > 0
	for ; token.tag != tokEnd; token = sc.next() {
		switch {
		case token.tag == tokNumber:
			hasNumber = true
			n := token.value
			if sc.skip(':') {
				if sc.skip(':') {
					// n + "::"
					if clock.index > 0 {
						return nan
					}
					clock.add(n)
					clock.add(0)
				} else {
					// n + ":"
					if !clock.add(n) {
						return nan
					}
					if sc.peek.isSymbol('.') {
						sc.next()
					}
				}
			} else if sc.skip('.') && clock.isExpecting(n) {
				clock.add(n)
				if sc.peek.tag != tokNumber {
					return nan
				}
				clock.addFinal(readMilliseconds(sc.next()))
			} else if tz.isExpecting(n) {
				tz.minute = n
			} else if clock.isExpecting(n) {
				clock.addFinal(n)
				// the end, white space, "Z", "+" or "-" must follow a time
				if p := sc.peek; p.tag != tokEnd && p.tag != tokSpace && !p.isZ() && !p.isSign() {
					return nan
				}
			} else {
				if !day.add(n) {
					return nan
				}
				sc.skip('-')
			}
		case token.tag >= 0: // a word
			switch {
			case token.tag == kwAmPm && clock.index > 0:
				clock.hourOffset = token.value
			case token.tag == kwMonth:
				day.namedMonth = token.value
				sc.skip('-')
			case token.tag == kwZone && hasNumber:
				tz.set(token.value)
			default:
				// garbage words are refused after a number, and before the
				// first one must be separated from it
				if hasNumber || sc.peek.tag == tokNumber {
					return nan
				}
			}
		case token.isSign() && (tz.isUTC() || clock.index > 0):
			// an offset, only after UTC or a time; its number may be empty
			tz.sign = token.sign()
			n, length := 0, 0
			if sc.peek.tag == tokNumber {
				t := sc.next()
				n, length = t.value, t.length
			}
			hasNumber = true
			switch {
			case sc.peek.isSymbol(':'):
				tz.hour, tz.minute = n, dateNone
			case length == 1 || length == 2:
				tz.hour, tz.minute = n, 0
			case length == 3 || length == 4:
				tz.hour, tz.minute = n/100, n%100
			default:
				return nan
			}
		case (token.isSign() || token.isSymbol(')')) && hasNumber:
			return nan
		}
	}

	year, month, date, ok := day.write()
	if !ok {
		return nan
	}
	hour, minute, second, ms, ok := clock.write()
	if !ok {
		return nan
	}
	offset, ok := tz.write()
	if !ok {
		return nan
	}
	t := makeDate(makeDay(float64(year), float64(month-1), float64(date)),
		makeTime(float64(hour), float64(minute), float64(second), float64(ms)))
	if math.IsNaN(offset) {
		// V8's kMaxTimeBeforeUTCInMs: a time value and ten days
		if !(math.Abs(t) <= maxTimeValue+10*msPerDay) {
			return nan
		}
		t = utcOfLocal(t)
	} else {
		t -= offset * 1000
	}
	return timeClip(t)
}

// parseES5Date reads as much of an ES date-time string as there is:
// `[+-yy]yyyy[-MM[-DD]][THH:mm[:ss[.s]][Z|+-hh:mm]]`. It returns the end
// for a whole one, an invalid token for one that cannot be a date, and
// otherwise the first token the legacy parser goes on with.
func parseES5Date(sc *dateScanner, day *dayComposer, clock *timeComposer, tz *zoneComposer) dateToken {
	invalid := dateToken{tag: tokInvalid}
	switch {
	case sc.peek.isSign():
		// the sign goes back to the legacy parser if no year follows it
		sign := sc.next()
		if !sc.peek.isFixedNumber(6) {
			return sign
		}
		year := sc.next().value
		if sign.sign() < 0 && year == 0 {
			return sign
		}
		day.add(sign.sign() * year)
	case sc.peek.isFixedNumber(4):
		day.add(sc.next().value)
	default:
		return sc.next()
	}
	if sc.skip('-') {
		if !sc.peek.isFixedNumber(2) || !dateBetween(sc.peek.value, 1, 12) {
			return sc.next()
		}
		day.add(sc.next().value)
		if sc.skip('-') {
			if !sc.peek.isFixedNumber(2) || !dateBetween(sc.peek.value, 1, 31) {
				return sc.next()
			}
			day.add(sc.next().value)
		}
	}
	if sc.peek.tag != kwTimeSeparator {
		if sc.peek.tag != tokEnd {
			return sc.next()
		}
	} else {
		sc.next()
		if !sc.peek.isFixedNumber(2) || !dateBetween(sc.peek.value, 0, 24) {
			return invalid
		}
		// 24:00[:00[.000]] is the end of the day, no other time of hour 24
		hour24 := sc.peek.value == 24
		clock.add(sc.next().value)
		if !sc.skip(':') {
			return invalid
		}
		if !sc.peek.isFixedNumber(2) || !dateBetween(sc.peek.value, 0, 59) || hour24 && sc.peek.value > 0 {
			return invalid
		}
		clock.add(sc.next().value)
		if sc.skip(':') {
			if !sc.peek.isFixedNumber(2) || !dateBetween(sc.peek.value, 0, 59) || hour24 && sc.peek.value > 0 {
				return invalid
			}
			clock.add(sc.next().value)
			if sc.skip('.') {
				if sc.peek.tag != tokNumber || hour24 && sc.peek.value > 0 {
					return invalid
				}
				clock.add(readMilliseconds(sc.next()))
			}
		}
		if sc.peek.isZ() {
			sc.next()
			tz.set(0)
		} else if sc.peek.isSymbol('+') || sc.peek.isSymbol('-') {
			tz.sign = sc.next().sign()
			if sc.peek.isFixedNumber(4) {
				// the hhmm extension
				hm := sc.next().value
				if !dateBetween(hm/100, 0, 23) || !dateBetween(hm%100, 0, 59) {
					return invalid
				}
				tz.hour, tz.minute = hm/100, hm%100
			} else {
				if !sc.peek.isFixedNumber(2) || !dateBetween(sc.peek.value, 0, 23) {
					return invalid
				}
				tz.hour = sc.next().value
				if !sc.skip(':') {
					return invalid
				}
				if !sc.peek.isFixedNumber(2) || !dateBetween(sc.peek.value, 0, 59) {
					return invalid
				}
				tz.minute = sc.next().value
			}
		}
		if sc.peek.tag != tokEnd {
			return invalid
		}
	}
	// without an offset a date alone is UTC, a date-time local time
	if tz.hour == dateNone && clock.index == 0 {
		tz.set(0)
	}
	day.iso = true
	return dateToken{tag: tokEnd}
}

// utcOfLocal is V8's DateCache::ToUTC of a wall-clock time of the local
// zone, ICU's getOffsetFromLocal with UCAL_TZ_LOCAL_FORMER: a time a
// transition skips or repeats takes the offset in force before it.
func utcOfLocal(local float64) float64 {
	at := func(ms float64) time.Time { return time.UnixMilli(int64(ms)).In(time.Local) }
	offsetAt := func(t time.Time) float64 { _, o := t.Zone(); return float64(o) * 1000 }
	later := at(local + msPerDay)
	before, offset := offsetAt(at(local-msPerDay)), offsetAt(later)
	if before != offset {
		start, _ := later.ZoneBounds()
		if local < float64(start.UnixMilli())+math.Max(before, offset) {
			offset = before
		}
	}
	return local - offset
}

// dateNone is V8's kNone: a part not read.
const dateNone = math.MaxInt32

func dateBetween(x, lo, hi int) bool { return x >= lo && x <= hi }

// dayComposer is V8's DayComposer: up to three numbers and a named month.
type dayComposer struct {
	comp       [3]int
	index      int
	namedMonth int
	iso        bool // the numbers are in ES order: year, month, day
}

func (d *dayComposer) add(n int) bool {
	if d.index < len(d.comp) {
		d.comp[d.index] = n
		d.index++
		return true
	}
	return false
}

// write is the year, month (1 to 12) and day, as V8 orders them.
func (d *dayComposer) write() (year, month, day int, ok bool) {
	if d.index < 1 {
		return 0, 0, 0, false
	}
	// as V8, the defaults count as numbers read: a missing year is 1
	for d.index < len(d.comp) {
		d.comp[d.index] = 1
		d.index++
	}
	isDay := func(x int) bool { return dateBetween(x, 1, 31) }
	switch {
	case d.namedMonth == dateNone && (d.iso || !isDay(d.comp[0])):
		year, month, day = d.comp[0], d.comp[1], d.comp[2]
	case d.namedMonth == dateNone:
		month, day, year = d.comp[0], d.comp[1], d.comp[2]
	case !isDay(d.comp[0]):
		month, year, day = d.namedMonth, d.comp[0], d.comp[1]
	default:
		month, day, year = d.namedMonth, d.comp[0], d.comp[1]
	}
	if !d.iso {
		if dateBetween(year, 0, 49) {
			year += 2000
		} else if dateBetween(year, 50, 99) {
			year += 1900
		}
	}
	return year, month, day, dateBetween(month, 1, 12) && isDay(day)
}

// timeComposer is V8's TimeComposer: hours, minutes, seconds and
// milliseconds, and the hours AM or PM adds.
type timeComposer struct {
	comp       [4]int
	index      int
	hourOffset int
}

func (c *timeComposer) isExpecting(n int) bool {
	return c.index == 1 && dateBetween(n, 0, 59) || c.index == 2 && dateBetween(n, 0, 59) ||
		c.index == 3 && dateBetween(n, 0, 999)
}

func (c *timeComposer) add(n int) bool {
	if c.index < len(c.comp) {
		c.comp[c.index] = n
		c.index++
		return true
	}
	return false
}

func (c *timeComposer) addFinal(n int) bool {
	if !c.add(n) {
		return false
	}
	for c.index < len(c.comp) {
		c.comp[c.index] = 0
		c.index++
	}
	return true
}

func (c *timeComposer) write() (hour, minute, second, ms int, ok bool) {
	for c.index < len(c.comp) {
		c.comp[c.index] = 0
		c.index++
	}
	hour, minute, second, ms = c.comp[0], c.comp[1], c.comp[2], c.comp[3]
	if c.hourOffset != dateNone {
		if !dateBetween(hour, 0, 12) {
			return 0, 0, 0, 0, false
		}
		hour = hour%12 + c.hourOffset
	}
	if !dateBetween(hour, 0, 23) || !dateBetween(minute, 0, 59) || !dateBetween(second, 0, 59) || !dateBetween(ms, 0, 999) {
		ok = hour == 24 && minute == 0 && second == 0 && ms == 0
		return hour, minute, second, ms, ok
	}
	return hour, minute, second, ms, true
}

// zoneComposer is V8's TimeZoneComposer: the sign, hours and minutes of
// an offset.
type zoneComposer struct{ sign, hour, minute int }

func (z *zoneComposer) set(hours int) {
	z.sign = 1
	if hours < 0 {
		z.sign = -1
	}
	z.hour, z.minute = hours*z.sign, 0
}

func (z *zoneComposer) isExpecting(n int) bool {
	return z.hour != dateNone && z.minute == dateNone && dateBetween(n, 0, 59)
}

func (z *zoneComposer) isUTC() bool { return z.hour == 0 && z.minute == 0 }

// write is the offset in seconds, NaN for none.
func (z *zoneComposer) write() (float64, bool) {
	if z.sign == dateNone {
		return nan, true
	}
	if z.hour == dateNone {
		z.hour = 0
	}
	if z.minute == dateNone {
		z.minute = 0
	}
	// in unsigned 32 bits, as V8 avoids an overflow, up to its largest Smi
	total := uint32(z.hour)*3600 + uint32(z.minute)*60
	if total > math.MaxInt32 {
		return 0, false
	}
	return float64(z.sign) * float64(total), true
}

// readMilliseconds is the first three significant digits of a numeral,
// leading zeros counted.
func readMilliseconds(t dateToken) int {
	n, length := t.value, t.length
	switch {
	case length == 1:
		return n * 100
	case length == 2:
		return n * 10
	case length > 3:
		for length = min(length, 9); length > 3; length-- {
			n /= 10
		}
	}
	return n
}

// A dateToken is V8's DateToken: a tag, or a word's keyword type.
type dateToken struct{ tag, length, value int }

const (
	tokInvalid = iota - 6
	tokUnknown
	tokSpace
	tokNumber
	tokSymbol
	tokEnd
	// the keyword types of a word
	kwInvalid
	kwMonth
	kwZone
	kwTimeSeparator
	kwAmPm
)

func (t dateToken) isSymbol(c rune) bool     { return t.tag == tokSymbol && t.value == int(c) }
func (t dateToken) isSign() bool             { return t.isSymbol('+') || t.isSymbol('-') }
func (t dateToken) sign() int                { return 44 - t.value }
func (t dateToken) isFixedNumber(n int) bool { return t.tag == tokNumber && t.length == n }
func (t dateToken) isZ() bool                { return t.tag == kwZone && t.length == 1 && t.value == 0 }

// dateKeywords are V8's KeywordTable: a word is one by its first three
// letters, a month name of any length.
var dateKeywords = []struct {
	prefix      string
	kind, value int
}{
	{"jan", kwMonth, 1}, {"feb", kwMonth, 2}, {"mar", kwMonth, 3}, {"apr", kwMonth, 4},
	{"may", kwMonth, 5}, {"jun", kwMonth, 6}, {"jul", kwMonth, 7}, {"aug", kwMonth, 8},
	{"sep", kwMonth, 9}, {"oct", kwMonth, 10}, {"nov", kwMonth, 11}, {"dec", kwMonth, 12},
	{"am", kwAmPm, 0}, {"pm", kwAmPm, 12},
	{"ut", kwZone, 0}, {"utc", kwZone, 0}, {"z", kwZone, 0}, {"gmt", kwZone, 0},
	{"cdt", kwZone, -5}, {"cst", kwZone, -6}, {"edt", kwZone, -4}, {"est", kwZone, -5},
	{"mdt", kwZone, -6}, {"mst", kwZone, -7}, {"pdt", kwZone, -7}, {"pst", kwZone, -8},
	{"t", kwTimeSeparator, 0},
}

// dateScanner is V8's DateStringTokenizer over a JS string's characters,
// read up to the end or a NUL, with one token of lookahead.
type dateScanner struct {
	s    string
	i    int  // the byte after ch
	ch   rune // 0 at the end
	peek dateToken
}

func newDateScanner(s string) *dateScanner {
	sc := &dateScanner{s: s}
	sc.advance()
	sc.peek = sc.scan()
	return sc
}

func (sc *dateScanner) advance() {
	if sc.i >= len(sc.s) {
		sc.ch = 0
		return
	}
	r, n := decode(sc.s[sc.i:])
	sc.ch, sc.i = r, sc.i+n
}

func (sc *dateScanner) next() dateToken {
	t := sc.peek
	sc.peek = sc.scan()
	return t
}

func (sc *dateScanner) skip(c rune) bool {
	if sc.peek.isSymbol(c) {
		sc.peek = sc.scan()
		return true
	}
	return false
}

// isJsWhiteSpace is ECMAScript's WhiteSpace: tab, vertical tab, form feed,
// no-break space, BOM and the space separators.
func isJsWhiteSpace(c rune) bool {
	return c == '\t' || c == '\v' || c == '\f' || c == 0xFEFF || unicode.Is(unicode.Zs, c)
}

func (sc *dateScanner) scan() dateToken {
	c := sc.ch
	switch {
	case c == 0:
		return dateToken{tokEnd, 0, -1}
	case c >= '0' && c <= '9':
		// nine significant digits; the rest count in the length only
		n, length, digits := 0, 0, 0
		for ; sc.ch == '0'; sc.advance() {
			length++
		}
		for ; sc.ch >= '0' && sc.ch <= '9'; sc.advance() {
			if digits < 9 {
				n = n*10 + int(sc.ch-'0')
			}
			digits++
			length++
		}
		return dateToken{tokNumber, length, n}
	case c == ':' || c == '-' || c == '+' || c == '.' || c == ')':
		sc.advance()
		return dateToken{tokSymbol, 1, int(c)}
	case c >= 'A' && !isJsWhiteSpace(c):
		// a word, by its first three characters lowered as V8 does
		var prefix [3]rune
		length := 0
		for ; sc.ch >= 'A' && !isJsWhiteSpace(sc.ch); sc.advance() {
			if length < len(prefix) {
				prefix[length] = sc.ch | 0x20
			}
			length++
		}
		for _, k := range dateKeywords {
			var want [3]rune
			for i, r := range k.prefix {
				want[i] = r
			}
			if want == prefix && (length <= len(prefix) || k.kind == kwMonth) {
				return dateToken{k.kind, length, k.value}
			}
		}
		return dateToken{kwInvalid, length, 0}
	case isJsWhiteSpace(c) || c == '\n' || c == '\r' || c == 0x2028 || c == 0x2029:
		sc.advance()
		return dateToken{tokSpace, 1, -1}
	case c == '(':
		// parenthesized text, nested, up to its end or the input's
		for balance := 0; ; {
			if sc.ch == ')' {
				balance--
			} else if sc.ch == '(' {
				balance++
			}
			sc.advance()
			if balance <= 0 || sc.ch == 0 {
				break
			}
		}
		return dateToken{tokUnknown, 1, -1}
	}
	sc.advance()
	return dateToken{tokUnknown, 1, -1}
}
