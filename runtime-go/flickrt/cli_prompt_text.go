package flickrt

import (
	"math"
	"regexp"
	"strings"
)

// The text and number prompts of Effect: a line edited at a cursor, in
// UTF-16 units as JS strings are.

type textState struct {
	cursor int
	value  string
	err    *string
}

type textOptions struct {
	kind     string // text, password or hidden
	message  string
	fallback string
	validate func(Value) (Value, *string)
}

func jsSlice(s string, from, to int) string {
	units := utf16Units(s)
	from, to = max(0, min(from, len(units))), max(0, min(to, len(units)))
	if from >= to {
		return ""
	}
	return fromUnits(units[from:to])
}

func (o *textOptions) input(s textState, t *promptTheme, submitted, plain bool) string {
	text := s.value
	if plain {
		switch o.kind {
		case "hidden":
			return ""
		case "password":
			return strings.Repeat(t.passwordMask, jsLen(text))
		}
		return text
	}
	if text == "" {
		// an empty value in ANSI codes would get the spacing of content
		return ""
	}
	styles := []string{ansiUnderline, t.primary}
	switch {
	case s.err != nil:
		styles = []string{t.errorColor}
	case submitted:
		styles = []string{t.submittedColor}
	}
	switch o.kind {
	case "hidden":
		return ""
	case "password":
		return annotateSymbol(t, strings.Repeat(t.passwordMask, jsLen(text)), styles...)
	}
	return annotate(t.colors, text, styles...)
}

func (o *textOptions) output(s textState, leading, trailing string, t *promptTheme, plain, submitted bool) string {
	return renderPrompt(o.input(s, t, submitted, plain), o.message, leading, trailing, t, plain)
}

func (o *textOptions) render(r *promptRunner, state Value, a promptAction) string {
	t := r.theme
	switch a.kind {
	case actBeep:
		return ansiBeep
	case actSubmit:
		return o.output(state.(textState), t.leading(true), t.trailing(true), t, false, true) + "\n"
	}
	s := a.state.(textState)
	width := 1
	if o.kind == "password" {
		width = jsLen(t.passwordMask)
	}
	return o.output(s, t.leading(false), t.trailing(false), t, false, false) +
		errorText(s.err, t.pointerSmall, t, false) + ansiCursorMove((s.cursor-jsLen(s.value))*width, 0)
}

func (o *textOptions) clear(r *promptRunner, state Value, _ promptAction) string {
	s, t := state.(textState), r.theme
	return clearOutputWithError(o.output(s, t.prefix, t.pointerSmall, t, true, false), r.columns(),
		errorText(s.err, t.pointerSmall, t, true)) + promptResetLine
}

func (o *textOptions) process(_ *promptRunner, input, state Value) promptAction {
	s, k := state.(textState), promptKeyOf(input)
	next := func(cursor int, value string) promptAction {
		return nextAction(textState{cursor: cursor, value: value})
	}
	if k.ctrl {
		switch k.name {
		case "u":
			return next(0, "")
		case "a":
			return next(0, s.value)
		case "e":
			return next(jsLen(s.value), s.value)
		}
		return beepAction()
	}
	n := jsLen(s.value)
	switch k.name {
	case "backspace":
		if s.cursor <= 0 {
			return beepAction()
		}
		return next(s.cursor-1, jsSlice(s.value, 0, s.cursor-1)+jsSlice(s.value, s.cursor, n))
	case "left":
		if s.cursor <= 0 {
			return beepAction()
		}
		return next(s.cursor-1, s.value)
	case "right":
		if s.cursor >= n {
			return beepAction()
		}
		return next(min(s.cursor+1, n), s.value)
	case "home":
		return next(0, s.value)
	case "end":
		return next(n, s.value)
	case "enter", "return":
		if o.validate == nil {
			return submitAction(s.value)
		}
		value, err := o.validate(s.value)
		if err != nil {
			return nextAction(textState{cursor: s.cursor, value: s.value, err: err})
		}
		return submitAction(value)
	case "tab":
		if s.value == o.fallback {
			return beepAction()
		}
		value := s.value
		if n == 0 {
			value = o.fallback
		}
		return next(jsLen(value), value)
	}
	return next(s.cursor+jsLen(k.input), jsSlice(s.value, 0, s.cursor)+k.input+jsSlice(s.value, s.cursor, n))
}

// textPrompt is Effect's `basePrompt` of the options of a `TextOptions`.
func textPrompt(options Value, kind string) Value {
	o := record(options)
	opts := &textOptions{kind: kind, message: o.Get("message").(string),
		fallback: optString(o, "default", ""), validate: optCheck(o, "validate")}
	first := textState{cursor: jsLen(opts.fallback), value: opts.fallback}
	return loopOp(&promptLoop{
		initial: func(*promptRunner) Value { return first },
		render:  opts.render, process: opts.process, clear: opts.clear,
	})
}

var redactedOf = Fn(RedactedMake)

func PromptText(options Value) Value { return textPrompt(options, "text") }
func PromptPassword(options Value) Value {
	return PromptMap(redactedOf, textPrompt(options, "password"))
}
func PromptHidden(options Value) Value { return PromptMap(redactedOf, textPrompt(options, "hidden")) }

// PromptList splits the line at the delimiter, a comma when none or empty.
func PromptList(options Value) Value {
	delimiter := optString(record(options), "delimiter", "")
	if delimiter == "" {
		delimiter = ","
	}
	return PromptMap(Fn(func(line Value) Value { return Split(delimiter, line) }), textPrompt(options, "text"))
}

// numberOptions are those of Effect's `Int` and `Number`.
type numberOptions struct {
	message              string
	min, max             float64
	increment, decrement float64
	precision            *int // a float's digits; nil for an integer
	validate             func(n float64) (Value, *string)
}

type numberState = textState

func (o *numberOptions) input(s numberState, t *promptTheme, submitted, plain bool) string {
	if submitted || plain {
		return s.value
	}
	styles := []string{ansiUnderline, t.primary}
	if s.err != nil {
		styles = []string{t.errorColor}
	}
	return annotate(t.colors, s.value, styles...)
}

func (o *numberOptions) output(s numberState, leading, trailing string, t *promptTheme, plain, submitted bool) string {
	return renderPrompt(o.input(s, t, submitted, plain), o.message, leading, trailing, t, plain)
}

func (o *numberOptions) render(r *promptRunner, state Value, a promptAction) string {
	t := r.theme
	switch a.kind {
	case actBeep:
		return ansiBeep
	case actSubmit:
		return o.output(state.(numberState), t.leading(true), t.trailing(true), t, false, true) + "\n"
	}
	s := a.state.(numberState)
	return o.output(s, t.leading(false), t.trailing(false), t, false, false) + errorText(s.err, t.pointerSmall, t, false)
}

func (o *numberOptions) clear(r *promptRunner, state Value, _ promptAction) string {
	s, t := state.(numberState), r.theme
	return clearOutputWithError(o.output(s, t.prefix, t.pointerSmall, t, true, false), r.columns(),
		errorText(s.err, t.pointerSmall, t, true)) + promptResetLine
}

var oneDigit = regexp.MustCompile(`^\d$`)

func (o *numberOptions) process(_ *promptRunner, input, state Value) promptAction {
	s, k := state.(numberState), promptKeyOf(input)
	set := func(value string) promptAction { return nextAction(numberState{cursor: s.cursor, value: value}) }
	parse := jsParseFloat
	if o.precision == nil {
		parse = jsParseInt
	}
	if k.ctrl && k.name == "u" {
		return nextAction(numberState{})
	}
	switch k.name {
	case "backspace":
		if s.value == "" {
			return beepAction()
		}
		return set(jsSlice(s.value, 0, jsLen(s.value)-1))
	case "k", "up":
		if s.value == "" || s.value == "-" {
			return set(jsNumber(o.increment))
		}
		return set(jsNumber(parse(s.value) + o.increment))
	case "j", "down":
		if s.value == "" || s.value == "-" {
			return set("-" + jsNumber(o.decrement))
		}
		return set(jsNumber(parse(s.value) - o.decrement))
	case "enter", "return":
		n := parse(s.value)
		if math.IsNaN(n) {
			message := "Must provide an integer value"
			if o.precision != nil {
				message = "Must provide a floating point value"
			}
			return nextAction(numberState{cursor: s.cursor, value: s.value, err: &message})
		}
		if o.precision != nil {
			n = jsRound(n, *o.precision)
		}
		value, err := o.validate(n)
		if err != nil {
			return nextAction(numberState{cursor: s.cursor, value: s.value, err: err})
		}
		return submitAction(value)
	}
	typed := k.input
	if o.precision != nil && typed == "." && strings.Contains(s.value, ".") {
		return beepAction()
	}
	if s.value == "" && typed == "-" {
		return set("-")
	}
	n := parse(s.value + typed)
	switch {
	case math.IsNaN(n):
		return beepAction()
	case o.precision == nil:
		return set(jsNumber(n))
	case typed == ".":
		return set(jsNumber(n) + ".")
	case strings.Contains(s.value, ".") && oneDigit.MatchString(typed):
		return set(s.value + typed)
	}
	return set(jsNumber(n))
}

// jsRound is Effect's `Number.round`: to the digits, halves up.
func jsRound(x float64, digits int) float64 {
	factor := math.Pow(10, float64(digits))
	return FloatRound(x*factor).(float64) / factor
}

func numberPrompt(o *numberOptions, fallback *string) Value {
	first := numberState{}
	if fallback != nil {
		first = numberState{cursor: jsLen(*fallback), value: *fallback}
	}
	return loopOp(&promptLoop{
		initial: func(*promptRunner) Value { return first },
		render:  o.render, process: o.process, clear: o.clear,
	})
}

// boundsCheck is Effect's default check of a number prompt.
func boundsCheck(n, low, high float64) *string {
	var message string
	switch {
	case n < low:
		message = jsNumber(n) + " must be greater than or equal to " + jsNumber(low)
	case n > high:
		message = jsNumber(n) + " must be less than or equal to " + jsNumber(high)
	default:
		return nil
	}
	return &message
}

func optNumber(o *Record, name string, otherwise float64, of func(Value) float64) float64 {
	if v, ok := some(o.Get(name)); ok {
		return of(v)
	}
	return otherwise
}

func intFloat(v Value) float64 { return float64(v.(int32)) }
func floatOf(v Value) float64  { return v.(float64) }

func defaultText(o *Record, of func(Value) float64) *string {
	if v, ok := some(o.Get("default")); ok {
		text := jsNumber(of(v))
		return &text
	}
	return nil
}

// PromptInt is `Cli.Prompt.int`: the bounds default to those of Flick's
// 32-bit `Int` and hold before `validate` runs.
func PromptInt(options Value) Value {
	o := record(options)
	low := optNumber(o, "min", math.MinInt32, intFloat)
	high := optNumber(o, "max", math.MaxInt32, intFloat)
	check := optCheck(o, "validate")
	return numberPrompt(&numberOptions{
		message:   o.Get("message").(string),
		min:       low,
		max:       high,
		increment: optNumber(o, "incrementBy", 1, intFloat),
		decrement: optNumber(o, "decrementBy", 1, intFloat),
		validate: func(n float64) (Value, *string) {
			if err := boundsCheck(n, low, high); err != nil {
				return nil, err
			}
			if check == nil {
				return int32(n), nil
			}
			return check(int32(n))
		},
	}, defaultText(o, intFloat))
}

func PromptNumber(options Value) Value {
	o := record(options)
	low := optNumber(o, "min", math.Inf(-1), floatOf)
	high := optNumber(o, "max", math.Inf(1), floatOf)
	precision := 2
	if p := optInt(o, "precision"); p != nil {
		precision = *p
	}
	check := optCheck(o, "validate")
	validate := func(n float64) (Value, *string) {
		if check != nil {
			return check(n)
		}
		if err := boundsCheck(n, low, high); err != nil {
			return nil, err
		}
		return n, nil
	}
	return numberPrompt(&numberOptions{
		message:   o.Get("message").(string),
		min:       low,
		max:       high,
		increment: optNumber(o, "incrementBy", 1, floatOf),
		decrement: optNumber(o, "decrementBy", 1, floatOf),
		precision: &precision,
		validate:  validate,
	}, defaultText(o, floatOf))
}

var (
	jsIntPrefix   = regexp.MustCompile(`^[+-]?\d+`)
	jsHexPrefix   = regexp.MustCompile(`^([+-]?)0[xX]([0-9a-fA-F]+)`)
	jsFloatPrefix = regexp.MustCompile(`^[+-]?(?:Infinity|(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?)`)
)

// jsParseInt is JS `parseInt` of decimal or `0x` hexadecimal text.
func jsParseInt(s string) float64 {
	s = strings.TrimLeftFunc(s, isJSSpace)
	if m := jsHexPrefix.FindStringSubmatch(s); m != nil {
		n := 0.0
		for _, c := range strings.ToLower(m[2]) {
			d := float64(strings.IndexRune("0123456789abcdef", c))
			n = n*16 + d
		}
		if m[1] == "-" {
			n = -n
		}
		return n
	}
	m := jsIntPrefix.FindString(s)
	if m == "" {
		return math.NaN()
	}
	return jsParseFloat(m)
}

// jsParseFloat is JS `parseFloat`: the longest number at the start.
func jsParseFloat(s string) float64 {
	m := jsFloatPrefix.FindString(strings.TrimLeftFunc(s, isJSSpace))
	if m == "" {
		return math.NaN()
	}
	n, _ := some(StringToNumber(m))
	return n.(float64)
}
