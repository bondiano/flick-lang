package flickrt

import (
	"math"
	"regexp"
	"strconv"
	"strings"
)

// The integer and number prompts: the typed text is kept as JS's
// `parseInt` and `parseFloat` read it.

type promptNumberState struct {
	cursor int
	value  string
	err    *string
}

type promptNumberOptions struct {
	message            string
	incrementBy        float64
	decrementBy        float64
	float              bool
	precision          float64
	min, max           float64
	validate           Value // a Flick check of the answer, or None
	boundsWithValidate bool  // the bounds hold before `validate`
}

var (
	promptIntPrefix   = regexp.MustCompile(`^[+-]?\d+`)
	promptFloatPrefix = regexp.MustCompile(`^[+-]?(Infinity|(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?)`)
)

// promptParse is JS `parseInt` (base 10) or `parseFloat`: the number the
// text starts with, NaN for none.
func promptParse(text string, float bool) float64 {
	text = strings.TrimLeft(text, " \t\n\r\v\f")
	prefix := promptIntPrefix
	if float {
		prefix = promptFloatPrefix
	}
	match := prefix.FindString(text)
	if match == "" {
		return math.NaN()
	}
	// strconv reads "Infinity" and overflows to it as JS does
	n, _ := strconv.ParseFloat(strings.TrimSuffix(match, "."), 64)
	return n
}

// promptJsString is JS `String(x)`.
func promptJsString(x float64) string {
	if x == 0 {
		return "0"
	}
	return ShowNumber(x)
}

func promptNumberLoop(o *Record, opts *promptNumberOptions) Value {
	initial := ""
	if v, ok := some(o.Get("default")); ok {
		initial = promptJsString(promptToFloat(v))
	}
	return &promptLoop{
		initial: func(*promptCtx) Value { return promptNumberState{len(initial), initial, nil} },
		render:  opts.render,
		process: opts.process,
		clear: func(c *promptCtx, state Value, _ promptAction) string {
			s, t := state.(promptNumberState), c.theme
			output := promptRender(s.value, opts.message, t.prefix, t.pointerSmall, t, true)
			return promptClearWithError(output, c.columns(), promptError(s.err, t.pointerSmall, t, true)) + promptResetLine
		},
	}
}

func promptToFloat(v Value) float64 {
	if n, ok := v.(int32); ok {
		return float64(n)
	}
	return v.(float64)
}

func promptFloatOr(option Value, otherwise float64) float64 {
	if v, ok := some(option); ok {
		return promptToFloat(v)
	}
	return otherwise
}

// PromptInt is `Cli.Prompt.int`: the bounds default to those of Flick's
// 32-bit `Int` and hold before `validate` runs, as the JS binding has it.
func PromptInt(options Value) Value {
	o := options.(*Record)
	return promptNumberLoop(o, &promptNumberOptions{
		message:     o.Get("message").(string),
		incrementBy: promptFloatOr(o.Get("incrementBy"), 1),
		decrementBy: promptFloatOr(o.Get("decrementBy"), 1),
		min:         promptFloatOr(o.Get("min"), math.MinInt32),
		max:         promptFloatOr(o.Get("max"), math.MaxInt32),
		validate:    o.Get("validate"), boundsWithValidate: true,
	})
}

func PromptNumber(options Value) Value {
	o := options.(*Record)
	return promptNumberLoop(o, &promptNumberOptions{
		message:     o.Get("message").(string),
		incrementBy: promptFloatOr(o.Get("incrementBy"), 1),
		decrementBy: promptFloatOr(o.Get("decrementBy"), 1),
		float:       true,
		precision:   promptFloatOr(o.Get("precision"), 2),
		min:         promptFloatOr(o.Get("min"), math.Inf(-1)),
		max:         promptFloatOr(o.Get("max"), math.Inf(1)),
		validate:    o.Get("validate"),
	})
}

func (o *promptNumberOptions) render(c *promptCtx, state Value, action promptAction) string {
	t := c.theme
	switch action.kind {
	case 'b':
		return ansiBeep
	case 's':
		s := state.(promptNumberState)
		return promptRender(s.value, o.message, promptAnnotateSymbol(t, t.tick, t.success), promptAnnotateSymbol(t, t.ellipsis, t.muted), t, false) + "\n"
	}
	s := action.value.(promptNumberState)
	annotation := []string{ansiUnderline, t.primary}
	if s.err != nil {
		annotation = []string{t.error}
	}
	value := ansiAnnotate(t.colors, s.value, annotation...)
	output := promptRender(value, o.message, promptAnnotateSymbol(t, t.prefix, t.primary), promptAnnotateSymbol(t, t.pointerSmall, t.muted), t, false)
	return output + promptError(s.err, t.pointerSmall, t, false)
}

// check is the answer `n` after the bounds and `validate`, or the message.
func (o *promptNumberOptions) check(fb *Fiber, n float64) (Value, *string) {
	_, custom := some(o.validate)
	if !custom || o.boundsWithValidate {
		var message string
		if n < o.min {
			message = promptJsString(n) + " must be greater than or equal to " + promptJsString(o.min)
		} else if n > o.max {
			message = promptJsString(n) + " must be less than or equal to " + promptJsString(o.max)
		}
		if message != "" {
			return nil, &message
		}
	}
	var value Value = n
	if !o.float {
		value = int32(n)
	}
	return promptValidate(fb, o.validate, value)
}

func (o *promptNumberOptions) process(c *promptCtx, input, state Value) promptAction {
	s := state.(promptNumberState)
	key := input.(*Record).Get("key").(*Record)
	name := key.Get("name").(string)
	next := func(value string) promptAction { return promptNext(promptNumberState{s.cursor, value, nil}) }
	if key.Get("ctrl").(bool) && name == "u" {
		return promptNext(promptNumberState{})
	}
	switch name {
	case "backspace":
		if s.value == "" {
			return promptBeep
		}
		return next(s.value[:len(s.value)-1])
	case "k", "up", "j", "down":
		up := name == "k" || name == "up"
		if s.value == "" || s.value == "-" {
			if up {
				return next(promptJsString(o.incrementBy))
			}
			return next("-" + promptJsString(o.decrementBy))
		}
		if up {
			return next(promptJsString(promptParse(s.value, o.float) + o.incrementBy))
		}
		return next(promptJsString(promptParse(s.value, o.float) - o.decrementBy))
	case "enter", "return":
		parsed := promptParse(s.value, o.float)
		if math.IsNaN(parsed) {
			message := "Must provide an integer value"
			if o.float {
				message = "Must provide a floating point value"
			}
			return promptNext(promptNumberState{s.cursor, s.value, &message})
		}
		if o.float {
			factor := math.Pow(10, o.precision)
			parsed = FloatRound(parsed*factor).(float64) / factor
		}
		value, err := o.check(c.fb, parsed)
		if err != nil {
			return promptNext(promptNumberState{s.cursor, s.value, err})
		}
		return promptSubmit(value)
	}
	typed := promptOr(input.(*Record).Get("input"), "")
	if o.float && typed == "." && strings.Contains(s.value, ".") {
		return promptBeep
	}
	if s.value == "" && typed == "-" {
		return next("-")
	}
	parsed := promptParse(s.value+typed, o.float)
	switch {
	case math.IsNaN(parsed):
		return promptBeep
	case o.float && typed == ".":
		return next(promptJsString(parsed) + ".")
	case o.float && strings.Contains(s.value, ".") && len(typed) == 1 && typed[0] >= '0' && typed[0] <= '9':
		return next(s.value + typed)
	}
	return next(promptJsString(parsed))
}
