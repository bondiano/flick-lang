package flickrt

import (
	"regexp"
	"strconv"
	"strings"
)

// The text, confirm and toggle prompts, and the drawing they share.

var promptNewline = regexp.MustCompile(`\r?\n`)

func promptLines(text string) []string { return promptNewline.Split(text, -1) }

// promptAnnotateSymbol is a symbol in its styles: nothing for no symbol.
func promptAnnotateSymbol(t *promptTheme, symbol string, styles ...string) string {
	if symbol == "" {
		return ""
	}
	return ansiAnnotate(t.colors, symbol, styles...)
}

func promptSeparate(symbol, text string) string {
	if symbol == "" {
		return text
	}
	return symbol + " " + text
}

// promptRender is the message between its symbols, then what it asks for.
func promptRender(confirm, message, leading, trailing string, t *promptTheme, plain bool) string {
	lines := promptLines(message)
	if !plain {
		for i, line := range lines {
			lines[i] = ansiAnnotate(t.colors, line, ansiBold)
		}
	}
	output := strings.Join(lines, "\n")
	if leading != "" {
		output = leading + " " + output
	}
	if trailing != "" {
		output += " " + trailing
	}
	if confirm != "" {
		output += " " + confirm
	} else if trailing != "" {
		output += " "
	}
	return output
}

// promptEraseText clears the rows `text` took.
// shortcut: rows count display columns where the JS target counts UTF-16
// units, so wide characters wrap as the terminal wraps them.
func promptEraseText(text string, columns int) string {
	if columns == 0 {
		return ansiEraseLine + "\x1b[1G"
	}
	rows := 0
	for _, line := range promptLines(text) {
		rows += 1 + max(promptWidth(line)-1, 0)/columns
	}
	return ansiEraseLines(rows)
}

func promptRows(text string, columns int) int {
	lines := promptLines(text)
	if columns == 0 {
		return len(lines)
	}
	rows := 0
	for _, line := range lines {
		rows += (promptWidth(line) + columns - 1) / columns
	}
	return rows
}

func promptClearWithError(output string, columns int, errorText string) string {
	if errorText != "" {
		return "\x1b[" + strconv.Itoa(promptRows(errorText, columns)) + "B" +
			promptEraseText("\n"+errorText, columns) + promptEraseText(output, columns)
	}
	return promptEraseText(output, columns)
}

// promptError is the error line under a prompt, drawn below the cursor.
func promptError(err *string, pointer string, t *promptTheme, plain bool) string {
	if err == nil {
		return ""
	}
	lines := promptLines(*err)
	if plain {
		return promptSeparate(pointer, strings.Join(lines, "\n"))
	}
	for i, line := range lines {
		lines[i] = ansiAnnotate(t.colors, line, ansiItalic, t.error)
	}
	prefix := promptAnnotateSymbol(t, pointer, t.error)
	return ansiCursorSave + "\n" + promptSeparate(prefix, strings.Join(lines, "\n")) + ansiCursorRestore
}

// promptValidate runs a Flick check `a -> Result String a`, if any.
func promptValidate(fb *Fiber, validate, value Value) (Value, *string) {
	f, ok := some(validate)
	if !ok {
		return value, nil
	}
	result := Apply(fb, f, value).(*Data)
	if result.Tag == "Failure" {
		message := result.Fields()[0].(string)
		return nil, &message
	}
	return result.Fields()[0], nil
}

func promptOr(option Value, otherwise string) string {
	if v, ok := some(option); ok {
		return v.(string)
	}
	return otherwise
}

const promptResetLine = ansiEraseLine + ansiCursorLeft

// Text.

type promptTextState struct {
	cursor int
	value  []rune
	err    *string
}

type promptTextOptions struct {
	kind, message, initial string
	validate               Value
}

func promptTextLoop(options Value, kind string) Value {
	o := options.(*Record)
	opts := &promptTextOptions{kind, o.Get("message").(string), promptOr(o.Get("default"), ""), o.Get("validate")}
	initial := []rune(opts.initial)
	return &promptLoop{
		initial: func(*promptCtx) Value { return promptTextState{len(initial), initial, nil} },
		render:  opts.render,
		process: opts.process,
		clear: func(c *promptCtx, state Value, _ promptAction) string {
			s, t := state.(promptTextState), c.theme
			output := opts.output(s, t.prefix, t.pointerSmall, t, true, false)
			return promptClearWithError(output, c.columns(), promptError(s.err, t.pointerSmall, t, true)) + promptResetLine
		},
	}
}

func PromptText(options Value) Value { return promptTextLoop(options, "text") }

func PromptPassword(options Value) Value {
	return promptMapGo(promptTextLoop(options, "password"), RedactedMake)
}

func PromptHidden(options Value) Value {
	return promptMapGo(promptTextLoop(options, "hidden"), RedactedMake)
}

func PromptList(options Value) Value {
	delimiter := promptOr(options.(*Record).Get("delimiter"), "")
	if delimiter == "" {
		delimiter = ","
	}
	return promptMapGo(PromptText(options), func(v Value) Value {
		parts := strings.Split(v.(string), delimiter)
		out := make([]Value, len(parts))
		for i, p := range parts {
			out[i] = p
		}
		return out
	})
}

func (o *promptTextOptions) input(s promptTextState, t *promptTheme, plain, submitted bool) string {
	text := string(s.value)
	switch {
	case o.kind == "hidden":
		return ""
	case plain && o.kind == "password":
		return strings.Repeat(t.passwordMask, len(s.value))
	case plain:
		return text
	case text == "":
		return ""
	}
	annotation := []string{ansiUnderline, t.primary}
	if s.err != nil {
		annotation = []string{t.error}
	} else if submitted {
		annotation = []string{t.submitted}
	}
	if o.kind == "password" {
		return promptAnnotateSymbol(t, strings.Repeat(t.passwordMask, len(s.value)), annotation...)
	}
	return ansiAnnotate(t.colors, text, annotation...)
}

func (o *promptTextOptions) output(s promptTextState, leading, trailing string, t *promptTheme, plain, submitted bool) string {
	return promptRender(o.input(s, t, plain, submitted), o.message, leading, trailing, t, plain)
}

func (o *promptTextOptions) render(c *promptCtx, state Value, action promptAction) string {
	t := c.theme
	switch action.kind {
	case 'b':
		return ansiBeep
	case 's':
		s := state.(promptTextState)
		leading, trailing := promptAnnotateSymbol(t, t.tick, t.success), promptAnnotateSymbol(t, t.ellipsis, t.muted)
		return o.output(s, leading, trailing, t, false, true) + "\n"
	}
	s := action.value.(promptTextState)
	leading, trailing := promptAnnotateSymbol(t, t.prefix, t.primary), promptAnnotateSymbol(t, t.pointerSmall, t.muted)
	// shortcut: the text after the cursor is measured in display columns where
	// the JS target counts UTF-16 units; the hidden prompt still moves as there
	offset := -promptWidth(string(s.value[s.cursor:]))
	if o.kind == "password" {
		offset = (s.cursor - len(s.value)) * len([]rune(t.passwordMask))
	} else if o.kind == "hidden" {
		offset = s.cursor - len(s.value)
	}
	return o.output(s, leading, trailing, t, false, false) + promptError(s.err, t.pointerSmall, t, false) + ansiCursorMove(offset)
}

func (o *promptTextOptions) process(c *promptCtx, input, state Value) promptAction {
	s := state.(promptTextState)
	key := input.(*Record).Get("key").(*Record)
	name := key.Get("name").(string)
	next := func(cursor int, value []rune) promptAction { return promptNext(promptTextState{cursor, value, nil}) }
	if key.Get("ctrl").(bool) {
		switch name {
		case "u":
			return next(0, nil)
		case "a":
			return next(0, s.value)
		case "e":
			return next(len(s.value), s.value)
		}
		return promptBeep
	}
	switch name {
	case "backspace":
		if s.cursor <= 0 {
			return promptBeep
		}
		value := append(append([]rune{}, s.value[:s.cursor-1]...), s.value[s.cursor:]...)
		return next(s.cursor-1, value)
	case "left":
		if s.cursor <= 0 {
			return promptBeep
		}
		return next(s.cursor-1, s.value)
	case "right":
		if s.cursor >= len(s.value) {
			return promptBeep
		}
		return next(s.cursor+1, s.value)
	case "home":
		return next(0, s.value)
	case "end":
		return next(len(s.value), s.value)
	case "enter", "return":
		value, err := promptValidate(c.fb, o.validate, string(s.value))
		if err != nil {
			return promptNext(promptTextState{s.cursor, s.value, err})
		}
		return promptSubmit(value)
	case "tab":
		if string(s.value) == o.initial {
			return promptBeep
		}
		value := s.value
		if len(value) == 0 {
			value = []rune(o.initial)
		}
		return next(len(value), value)
	}
	// typed control bytes must not become terminal commands when echoed
	typed := []rune(promptVisible(promptOr(input.(*Record).Get("input"), "")))
	value := append(append(append([]rune{}, s.value[:s.cursor]...), typed...), s.value[s.cursor:]...)
	return next(s.cursor+len(typed), value)
}

// Confirm.

func PromptConfirm(options Value) Value {
	o := options.(*Record)
	message := o.Get("message").(string)
	initial := false
	if v, ok := some(o.Get("initial")); ok {
		initial = v.(bool)
	}
	yes, no := "yes", "no"
	if label, ok := some(o.Get("label")); ok {
		yes, no = label.(*Record).Get("confirm").(string), label.(*Record).Get("deny").(string)
	}
	byDefault, otherwise := "(Y/n)", "(y/N)"
	if p, ok := some(o.Get("placeholder")); ok {
		byDefault = promptOr(p.(*Record).Get("defaultConfirm"), byDefault)
		otherwise = promptOr(p.(*Record).Get("defaultDeny"), otherwise)
	}
	placeholder := func(value bool) string {
		if value {
			return byDefault
		}
		return otherwise
	}
	return &promptLoop{
		initial: func(*promptCtx) Value { return initial },
		render: func(c *promptCtx, _ Value, action promptAction) string {
			t := c.theme
			switch action.kind {
			case 'b':
				return ansiBeep
			case 's':
				answer := no
				if action.value.(bool) {
					answer = yes
				}
				return promptRender(answer, message, promptAnnotateSymbol(t, t.tick, t.success), promptAnnotateSymbol(t, t.ellipsis, t.muted), t, false) + "\n"
			}
			confirm := ansiAnnotate(t.colors, placeholder(action.value.(bool)), t.muted)
			return ansiCursorHide + promptRender(confirm, message, promptAnnotateSymbol(t, t.prefix, t.primary), promptAnnotateSymbol(t, t.pointerSmall, t.muted), t, false)
		},
		process: func(_ *promptCtx, input, _ Value) promptAction {
			name := input.(*Record).Get("key").(*Record).Get("name").(string)
			if name == "enter" || name == "return" {
				return promptSubmit(initial)
			}
			value := strings.ToLower(promptOr(input.(*Record).Get("input"), ""))
			switch {
			case strings.HasPrefix(value, "y") || strings.HasSuffix(value, "t"):
				return promptSubmit(true)
			case strings.HasPrefix(value, "n") || strings.HasSuffix(value, "f"):
				return promptSubmit(false)
			}
			return promptBeep
		},
		clear: func(c *promptCtx, state Value, _ promptAction) string {
			t := c.theme
			text := promptRender(placeholder(state.(bool)), message, t.prefix, t.pointerSmall, t, true)
			return promptEraseText(text, c.columns()) + promptResetLine
		},
	}
}

// Toggle.

func PromptToggle(options Value) Value {
	o := options.(*Record)
	message := o.Get("message").(string)
	initial := false
	if v, ok := some(o.Get("initial")); ok {
		initial = v.(bool)
	}
	active, inactive := promptOr(o.Get("active"), "on"), promptOr(o.Get("inactive"), "off")
	toggle := func(t *promptTheme, value, submitted bool) string {
		separator := promptAnnotateSymbol(t, t.toggleSep, t.muted)
		selected := []string{ansiUnderline, t.primary}
		if submitted {
			selected[1] = t.submitted
		}
		on, off := active, ansiAnnotate(t.colors, inactive, selected...)
		if value {
			on, off = ansiAnnotate(t.colors, active, selected...), inactive
		}
		return on + " " + promptSeparate(separator, off)
	}
	return &promptLoop{
		initial: func(*promptCtx) Value { return initial },
		render: func(c *promptCtx, state Value, action promptAction) string {
			t := c.theme
			switch action.kind {
			case 'b':
				return ansiBeep
			case 's':
				return promptRender(toggle(t, state.(bool), true), message, promptAnnotateSymbol(t, t.tick, t.success), promptAnnotateSymbol(t, t.ellipsis, t.muted), t, false) + "\n"
			}
			return ansiCursorHide + promptRender(toggle(t, state.(bool), false), message, promptAnnotateSymbol(t, t.prefix, t.primary), promptAnnotateSymbol(t, t.pointerSmall, t.muted), t, false)
		},
		process: func(_ *promptCtx, input, state Value) promptAction {
			switch input.(*Record).Get("key").(*Record).Get("name").(string) {
			case "0", "j", "delete", "right", "down":
				return promptNext(false)
			case "1", "k", "left", "up":
				return promptNext(true)
			case " ", "tab":
				return promptNext(!state.(bool))
			case "enter", "return":
				return promptSubmit(state)
			}
			return promptBeep
		},
		clear: func(c *promptCtx, _ Value, _ promptAction) string {
			t := c.theme
			text := promptRender(active+" "+promptSeparate(t.toggleSep, inactive), message, t.prefix, t.pointerSmall, t, true)
			return promptEraseText(text, c.columns()) + promptResetLine
		},
	}
}
