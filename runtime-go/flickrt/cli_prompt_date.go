package flickrt

import (
	"math"
	"regexp"
	"strings"
	"time"
)

// Effect's date prompt: a mask split into parts, each edited on a JS Date
// in the local time zone that all of them share, as Effect's do.

// promptDate is that JS Date: an instant read and set in local time.
type promptDate struct{ t *DateTime }

var localTime = &TimeZone{id: "Local", loc: time.Local}

func (d *promptDate) wall() *date {
	if math.IsNaN(d.t.millis) {
		return &date{math.NaN()}
	}
	return d.t.wall()
}

// set changes the local date and time, placed back as JS places them.
func (d *promptDate) set(f func(*date)) {
	if !math.IsNaN(d.t.millis) {
		d.t = d.t.mutate(f)
	}
}

// datePart is one part of the mask: a token of text, or a field.
type datePart struct {
	kind   byte // 0 text, else the field: D M Y a H m s S
	token  string
	date   *promptDate
	locale *dateLocale
}

type dateLocale struct{ months, monthsShort, weekdays, weekdaysShort []string }

func (p *datePart) isToken() bool { return p.kind == 0 }

func atOr(names []string, i int) string {
	if i >= 0 && i < len(names) {
		return names[i]
	}
	return ""
}

func padNumber(n float64, width int) string {
	s := jsNumber(n)
	for jsLen(s) < width {
		s = "0" + s
	}
	return s
}

func (p *datePart) String() string {
	w := p.date.wall()
	switch p.kind {
	case 'S':
		return jsSlice(padNumber(w.milliseconds(), 4), 0, len(p.token))
	case 's', 'm':
		n := w.seconds()
		if p.kind == 'm' {
			n = w.minutes()
		}
		if len(p.token) > 1 {
			return padNumber(n, 2)
		}
		return jsNumber(n)
	case 'H':
		h := w.hours()
		if strings.Contains(p.token, "h") {
			if h = math.Mod(h, 12); h == 0 {
				h = 12
			}
		}
		if len(p.token) > 1 {
			return padNumber(h, 2)
		}
		return jsNumber(h)
	case 'D':
		day, weekday := w.day(), int(w.weekDay())
		switch p.token {
		case "DD":
			return padNumber(day, 2)
		case "Do":
			return jsNumber(day) + ordinalIndicator(int(day))
		case "d":
			return jsNumber(float64(weekday + 1))
		case "ddd":
			return atOr(p.locale.weekdaysShort, weekday)
		case "dddd":
			return atOr(p.locale.weekdays, weekday)
		}
		return jsNumber(day)
	case 'M':
		m := int(w.month())
		switch len(p.token) {
		case 2:
			return padNumber(float64(m+1), 2)
		case 3:
			return atOr(p.locale.monthsShort, m)
		case 4:
			return atOr(p.locale.months, m)
		}
		return jsNumber(float64(m + 1))
	case 'Y':
		year := padNumber(w.year(), 4)
		if len(p.token) == 2 {
			return jsSlice(year, jsLen(year)-2, jsLen(year))
		}
		return year
	case 'a':
		meridiem := "am"
		if w.hours() >= 12 {
			meridiem = "pm"
		}
		if strings.Contains(p.token, "A") {
			return strings.ToUpper(meridiem)
		}
		return meridiem
	}
	return p.token
}

func ordinalIndicator(day int) string {
	if day >= 11 && day <= 13 {
		return "th"
	}
	switch day % 10 {
	case 1:
		return "st"
	case 2:
		return "nd"
	case 3:
		return "rd"
	}
	return "th"
}

// step moves the field by one, as Effect's increment and decrement.
func (p *datePart) step(by float64) {
	if p.kind == 0 {
		return
	}
	p.date.set(func(d *date) {
		switch p.kind {
		case 'S':
			d.setMillis(d.milliseconds() + by)
		case 's':
			d.setSeconds(d.seconds()+by, d.milliseconds())
		case 'm':
			d.setMinutes(d.minutes()+by, d.seconds(), d.milliseconds())
		case 'H':
			d.setHours(d.hours()+by, d.minutes(), d.seconds(), d.milliseconds())
		case 'D':
			d.setDate(d.day() + by)
		case 'M':
			d.setMonth(d.month()+by, d.day())
		case 'Y':
			d.setFullYear(d.year()+by, d.month(), d.day())
		case 'a':
			d.setHours(math.Mod(d.hours()+12, 24), d.minutes(), d.seconds(), d.milliseconds())
		}
	})
}

// setValue sets the field from the digits typed into it.
func (p *datePart) setValue(typed string) {
	switch p.kind {
	case 0:
		p.token += typed
		return
	case 'a':
		return
	}
	last := func(n int) float64 { return jsParseInt(jsSlice(typed, max(jsLen(typed)-n, 0), jsLen(typed))) }
	p.date.set(func(d *date) {
		switch p.kind {
		case 'S':
			d.setMillis(last(len(p.token)))
		case 's':
			d.setSeconds(last(2), d.milliseconds())
		case 'm':
			d.setMinutes(last(2), d.seconds(), d.milliseconds())
		case 'H':
			d.setHours(last(2), d.minutes(), d.seconds(), d.milliseconds())
		case 'D':
			d.setDate(last(2))
		case 'M':
			d.setMonth(math.Max(last(2)-1, 0), d.day())
		case 'Y':
			d.setFullYear(last(4), d.month(), d.day())
		}
	})
}

var datePartPattern = regexp.MustCompile(`\\(.)|"((?:\\["\\]|[^"])+)"|(D[Do]?|d{3,4}|d)|(M{1,4})|(YY(?:YY)?)|([aA])|([Hh]{1,2})|(m{1,2})|(s{1,2})|(S{1,4})|.`)

// datePartKinds are the fields of the groups of the pattern after the
// first two, which are text.
const datePartKinds = "DMYaHmsS"

var escapedChar = regexp.MustCompile(`\\(.)`)

// makeDateParts splits the mask into parts, adjacent text joined.
func makeDateParts(mask string, d *promptDate, locale *dateLocale) []*datePart {
	var parts []*datePart
	for _, m := range datePartPattern.FindAllStringSubmatchIndex(mask, -1) {
		part := &datePart{token: mask[m[0]:m[1]], date: d, locale: locale}
		for g := 1; g <= 10; g++ {
			if m[2*g] < 0 {
				continue
			}
			text := mask[m[2*g]:m[2*g+1]]
			switch {
			case g == 1:
				part.token = text
			case g == 2:
				part.token = escapedChar.ReplaceAllString(text, "$1")
			default:
				part.kind, part.token = datePartKinds[g-3], text
			}
			break
		}
		if n := len(parts); part.isToken() && n > 0 && parts[n-1].isToken() {
			parts[n-1].setValue(part.token)
			continue
		}
		parts = append(parts, part)
	}
	return parts
}

type dateState struct {
	parts  []*datePart
	typed  string
	cursor int
	err    *string
}

func firstField(parts []*datePart) int {
	for i, p := range parts {
		if !p.isToken() {
			return i
		}
	}
	return -1
}

// neighbour is the field after (or before) the part, -1 for none.
func neighbour(parts []*datePart, at, step int) int {
	for i := at + step; i >= 0 && i < len(parts); i += step {
		if !parts[i].isToken() {
			return i
		}
	}
	return -1
}

func (s dateState) text(t *promptTheme, submitted, plain bool) string {
	var out strings.Builder
	for i, p := range s.parts {
		if i == s.cursor && !submitted && !plain {
			out.WriteString(annotate(t.colors, p.String(), ansiUnderline, t.primary))
		} else {
			out.WriteString(p.String())
		}
	}
	return out.String()
}

// PromptDate is `Cli.Prompt.date`: `DateTime` in and out, the parts
// edited on a local date.
func PromptDate(options Value) Value {
	o := record(options)
	message := o.Get("message").(string)
	start := &promptDate{}
	if v, ok := some(o.Get("initial")); ok {
		start.t = &DateTime{millis: timeClip(dt(v).millis), zone: localTime}
	} else {
		start.t = &DateTime{millis: float64(time.Now().UnixMilli()), zone: localTime}
	}
	locale := &dateLocale{
		months: []string{"January", "February", "March", "April", "May", "June", "July", "August",
			"September", "October", "November", "December"},
		monthsShort:   []string{"Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"},
		weekdays:      []string{"Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"},
		weekdaysShort: []string{"Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"},
	}
	if l, ok := some(o.Get("locales")); ok {
		names := func(field string, n int) []string {
			out := make([]string, n)
			for i, name := range List(record(l).Get(field)) {
				if i < n {
					out[i] = name.(string)
				}
			}
			return out
		}
		locale = &dateLocale{names("months", 12), names("monthsShort", 12), names("weekdays", 7), names("weekdaysShort", 7)}
	}
	check := optCheck(o, "validate")
	first := dateState{parts: makeDateParts(optString(o, "dateMask", "YYYY-MM-DD HH:mm:ss"), start, locale)}
	first.cursor = firstField(first.parts)
	date := loopOp(&promptLoop{
		initial: func(*promptRunner) Value { return first },
		render: func(r *promptRunner, state Value, a promptAction) string {
			t := r.theme
			switch a.kind {
			case actBeep:
				return ansiBeep
			case actSubmit:
				text := state.(dateState).text(t, true, false)
				return renderPrompt(text, message, t.leading(true), t.trailing(true), t, false) + "\n"
			}
			s := a.state.(dateState)
			out := ansiCursorHide + renderPrompt(s.text(t, false, false), message, t.leading(false), t.trailing(false), t, false)
			if s.err != nil {
				out += errorText(s.err, t.pointerSmall, t, false)
			}
			return out
		},
		process: func(_ *promptRunner, input, state Value) promptAction {
			s, k := state.(dateState), promptKeyOf(input)
			moved := func(cursor int) promptAction {
				if cursor < 0 {
					return beepAction()
				}
				return nextAction(dateState{parts: s.parts, cursor: cursor, err: s.err})
			}
			switch k.name {
			case "left":
				return moved(neighbour(s.parts, s.cursor, -1))
			case "right":
				return moved(neighbour(s.parts, s.cursor, 1))
			case "k", "up", "j", "down":
				by := 1.0
				if k.is("j", "down") {
					by = -1
				}
				s.parts[s.cursor].step(by)
				return nextAction(dateState{parts: s.parts, cursor: s.cursor, err: s.err})
			case "tab":
				next := neighbour(s.parts, s.cursor, 1)
				if next < 0 {
					next = firstField(s.parts)
				}
				return nextAction(dateState{parts: s.parts, cursor: next, err: s.err})
			case "enter", "return":
				value := Value(utcAt(start.t.millis))
				if check != nil {
					checked, err := check(value)
					if err != nil {
						return nextAction(dateState{parts: s.parts, typed: s.typed, cursor: s.cursor, err: err})
					}
					value = checked
				}
				return submitAction(utcAt(timeClip(dt(value).millis)))
			}
			if !strings.ContainsAny(k.input, "0123456789") {
				return beepAction()
			}
			typed := s.typed + k.input
			s.parts[s.cursor].setValue(typed)
			return nextAction(dateState{parts: s.parts, typed: typed, cursor: s.cursor, err: s.err})
		},
		clear: func(r *promptRunner, state Value, _ promptAction) string {
			s, t := state.(dateState), r.theme
			text := renderPrompt(s.text(t, false, true), message, t.prefix, t.pointerSmall, t, true)
			return clearOutputWithError(text, r.columns(), errorText(s.err, t.pointerSmall, t, true)) + promptResetLine
		},
	})
	return date
}
