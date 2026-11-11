package flickrt

import (
	"regexp"
	"strconv"
	"strings"
	"time"
)

// The date prompt: the mask split into parts that edit one date in local
// time, as JS `Date` setters do.

type promptDateLocales struct{ months, monthsShort, weekdays, weekdaysShort []string }

var promptDefaultLocales = promptDateLocales{
	strings.Fields("January February March April May June July August September October November December"),
	strings.Fields("Jan Feb Mar Apr May Jun Jul Aug Sep Oct Nov Dec"),
	strings.Fields("Sunday Monday Tuesday Wednesday Thursday Friday Saturday"),
	strings.Fields("Sun Mon Tue Wed Thu Fri Sat"),
}

// promptDatePart is a part of the mask: a literal token (kind 0) or one of
// day, month, year, meridiem, hours, minutes, seconds and milliseconds.
type promptDatePart struct {
	kind  int
	token string
}

const (
	promptToken = iota
	promptDay
	promptMonth
	promptYear
	promptMeridiem
	promptHours
	promptMinutes
	promptSeconds
	promptMillis
)

var promptDateMask = regexp.MustCompile(`\\(.)|"((?:\\["\\]|[^"])+)"|(D[Do]?|d{3,4}|d)|(M{1,4})|(YY(?:YY)?)|([aA])|([Hh]{1,2})|(m{1,2})|(s{1,2})|(S{1,4})|.`)

var promptUnescape = regexp.MustCompile(`\\(.)`)

func promptDateParts(mask string) []promptDatePart {
	var parts []promptDatePart
	for _, m := range promptDateMask.FindAllStringSubmatch(mask, -1) {
		part := promptDatePart{promptToken, m[0]}
		switch group := promptFirstGroup(m); {
		case group == 1:
			part.token = m[1]
		case group == 2:
			part.token = promptUnescape.ReplaceAllString(m[2], "$1")
		case group > 2:
			part = promptDatePart{group - 2, m[group]}
		}
		if n := len(parts); part.kind == promptToken && n > 0 && parts[n-1].kind == promptToken {
			parts[n-1].token += part.token
			continue
		}
		parts = append(parts, part)
	}
	return parts
}

func promptFirstGroup(m []string) int {
	for i := 1; i < len(m); i++ {
		if m[i] != "" {
			return i
		}
	}
	return 0
}

// promptDateValue is the date the parts edit, shared by the states of one run.
type promptDateValue struct{ t time.Time }

func (d *promptDateValue) set(year, month, day, hour, minute, second, milli int) {
	t := d.t
	d.t = time.Date(t.Year()+year, t.Month()+time.Month(month), t.Day()+day, t.Hour()+hour,
		t.Minute()+minute, t.Second()+second, t.Nanosecond()+milli*int(time.Millisecond), time.Local)
}

func (p promptDatePart) step(d *promptDateValue, by int) {
	switch p.kind {
	case promptMillis:
		d.set(0, 0, 0, 0, 0, 0, by)
	case promptSeconds:
		d.set(0, 0, 0, 0, 0, by, 0)
	case promptMinutes:
		d.set(0, 0, 0, 0, by, 0, 0)
	case promptHours:
		d.set(0, 0, 0, by, 0, 0, 0)
	case promptDay:
		d.set(0, 0, by, 0, 0, 0, 0)
	case promptMonth:
		d.set(0, by, 0, 0, 0, 0, 0)
	case promptYear:
		d.set(by, 0, 0, 0, 0, 0, 0)
	case promptMeridiem:
		d.set(0, 0, 0, (d.t.Hour()+12)%24-d.t.Hour(), 0, 0, 0)
	}
}

// setValue sets the part to the last digits typed.
// shortcut: typed text that does not end in digits leaves the date as it is
// where JS makes it invalid; only a pasted key press can type such text.
func (p promptDatePart) setValue(d *promptDateValue, typed string) {
	last := func(n int) (int, bool) {
		r := []rune(typed)
		v, err := strconv.Atoi(string(r[max(len(r)-n, 0):]))
		return v, err == nil
	}
	t := d.t
	switch p.kind {
	case promptMillis:
		if v, ok := last(len(p.token)); ok {
			d.set(0, 0, 0, 0, 0, 0, v-t.Nanosecond()/int(time.Millisecond))
		}
	case promptSeconds:
		if v, ok := last(2); ok {
			d.set(0, 0, 0, 0, 0, v-t.Second(), 0)
		}
	case promptMinutes:
		if v, ok := last(2); ok {
			d.set(0, 0, 0, 0, v-t.Minute(), 0, 0)
		}
	case promptHours:
		if v, ok := last(2); ok {
			d.set(0, 0, 0, v-t.Hour(), 0, 0, 0)
		}
	case promptDay:
		if v, ok := last(2); ok {
			d.set(0, 0, v-t.Day(), 0, 0, 0, 0)
		}
	case promptMonth:
		if v, ok := last(2); ok {
			d.set(0, max(v-1, 0)-int(t.Month()-1), 0, 0, 0, 0, 0)
		}
	case promptYear:
		if v, ok := last(4); ok {
			d.set(v-t.Year(), 0, 0, 0, 0, 0, 0)
		}
	}
}

func promptPad(n, width int) string {
	s := strconv.Itoa(n)
	return strings.Repeat("0", max(width-len(s), 0)) + s
}

func promptLocale(names []string, i int) string {
	if i < len(names) {
		return names[i]
	}
	return ""
}

func (p promptDatePart) String(d *promptDateValue, l *promptDateLocales) string {
	t, long := d.t, len(p.token) > 1
	pick := func(n, width int) string {
		if long {
			return promptPad(n, width)
		}
		return strconv.Itoa(n)
	}
	switch p.kind {
	case promptMillis:
		return promptPad(t.Nanosecond()/int(time.Millisecond), 4)[:len(p.token)]
	case promptSeconds:
		return pick(t.Second(), 2)
	case promptMinutes:
		return pick(t.Minute(), 2)
	case promptHours:
		hours := t.Hour()
		if strings.Contains(p.token, "h") {
			hours = (hours+11)%12 + 1
		}
		return pick(hours, 2)
	case promptDay:
		switch p.token {
		case "DD":
			return promptPad(t.Day(), 2)
		case "Do":
			return strconv.Itoa(t.Day()) + promptOrdinal(t.Day())
		case "d":
			return strconv.Itoa(int(t.Weekday()) + 1)
		case "ddd":
			return promptLocale(l.weekdaysShort, int(t.Weekday()))
		case "dddd":
			return promptLocale(l.weekdays, int(t.Weekday()))
		}
		return strconv.Itoa(t.Day())
	case promptMonth:
		switch month := int(t.Month()) - 1; len(p.token) {
		case 2:
			return promptPad(month+1, 2)
		case 3:
			return promptLocale(l.monthsShort, month)
		case 4:
			return promptLocale(l.months, month)
		default:
			return strconv.Itoa(month + 1)
		}
	case promptYear:
		year := promptPad(t.Year(), 4)
		if len(p.token) == 2 {
			return year[len(year)-2:]
		}
		return year
	case promptMeridiem:
		meridiem := "am"
		if t.Hour() >= 12 {
			meridiem = "pm"
		}
		if strings.Contains(p.token, "A") {
			return strings.ToUpper(meridiem)
		}
		return meridiem
	}
	return p.token
}

func promptOrdinal(day int) string {
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

type promptDateState struct {
	date   *promptDateValue
	typed  string
	cursor int
	err    *string
}

func PromptDate(options Value) Value {
	o := options.(*Record)
	message, mask := o.Get("message").(string), promptOr(o.Get("dateMask"), "YYYY-MM-DD HH:mm:ss")
	locales := promptDefaultLocales
	if l, ok := some(o.Get("locales")); ok {
		names := func(field string) []string {
			var out []string
			for _, v := range List(l.(*Record).Get(field)) {
				out = append(out, v.(string))
			}
			return out
		}
		locales = promptDateLocales{names("months"), names("monthsShort"), names("weekdays"), names("weekdaysShort")}
	}
	parts := promptDateParts(mask)
	first := -1
	for i, p := range parts {
		if p.kind != promptToken {
			first = i
			break
		}
	}
	// the nearest editable part before (-1) or after (1) the cursor
	near := func(cursor, by int) int {
		for i := cursor + by; i >= 0 && i < len(parts); i += by {
			if parts[i].kind != promptToken {
				return i
			}
		}
		return -1
	}
	text := func(t *promptTheme, s promptDateState, active bool) string {
		var b strings.Builder
		for i, p := range parts {
			part := p.String(s.date, &locales)
			if active && i == s.cursor {
				part = ansiAnnotate(t.colors, part, ansiUnderline, t.primary)
			}
			b.WriteString(part)
		}
		return b.String()
	}
	return &promptLoop{
		initial: func(*promptCtx) Value {
			start := time.Now()
			if v, ok := some(o.Get("initial")); ok {
				start = v.(*DateTime).time()
			}
			return promptDateState{&promptDateValue{start.Local()}, "", first, nil}
		},
		render: func(c *promptCtx, state Value, action promptAction) string {
			t := c.theme
			switch action.kind {
			case 'b':
				return ansiBeep
			case 's':
				return promptRender(text(t, state.(promptDateState), false), message, promptAnnotateSymbol(t, t.tick, t.success), promptAnnotateSymbol(t, t.ellipsis, t.muted), t, false) + "\n"
			}
			s := action.value.(promptDateState)
			output := promptRender(text(t, s, true), message, promptAnnotateSymbol(t, t.prefix, t.primary), promptAnnotateSymbol(t, t.pointerSmall, t.muted), t, false)
			return ansiCursorHide + output + promptError(s.err, t.pointerSmall, t, false)
		},
		process: func(c *promptCtx, input, state Value) promptAction {
			s := state.(promptDateState)
			moved := func(cursor int) promptAction {
				if cursor < 0 {
					return promptBeep
				}
				return promptNext(promptDateState{s.date, "", cursor, s.err})
			}
			name, _ := promptKeyName(input)
			if s.cursor < 0 && name != "enter" && name != "return" {
				return promptBeep // a mask without an editable part
			}
			switch name {
			case "left":
				return moved(near(s.cursor, -1))
			case "right":
				return moved(near(s.cursor, 1))
			case "k", "up", "j", "down":
				by := 1
				if name == "j" || name == "down" {
					by = -1
				}
				parts[s.cursor].step(s.date, by)
				return moved(s.cursor)
			case "tab":
				if next := near(s.cursor, 1); next >= 0 {
					return moved(next)
				}
				return moved(first)
			case "enter", "return":
				value, err := promptValidate(c.fb, o.Get("validate"), &DateTime{ms: float64(s.date.t.UnixMilli())})
				if err != nil {
					return promptNext(promptDateState{s.date, s.typed, s.cursor, err})
				}
				return promptSubmit(value)
			}
			typed := promptOr(input.(*Record).Get("input"), "")
			if !strings.ContainsAny(typed, "0123456789") {
				return promptBeep
			}
			s.typed += typed
			parts[s.cursor].setValue(s.date, s.typed)
			return promptNext(s)
		},
		clear: func(c *promptCtx, state Value, _ promptAction) string {
			s, t := state.(promptDateState), c.theme
			output := promptRender(text(t, s, false), message, t.prefix, t.pointerSmall, t, true)
			return promptClearWithError(output, c.columns(), promptError(s.err, t.pointerSmall, t, true)) + promptResetLine
		},
	}
}
