package flickrt

import (
	"strings"
)

// What Effect's prompts share to draw a frame and to erase it. Widths are
// JS lengths, in UTF-16 units, as Effect measures them.

func jsLen(s string) int { return int(Length(s).(int32)) }

// jsNumber is a number as JS writes it in a template string.
func jsNumber(x float64) string {
	if x == 0 {
		return "0"
	}
	return ShowNumber(x)
}

// splitLines is `text.split(/\r?\n/)`.
func splitLines(text string) []string {
	parts := strings.Split(text, "\n")
	for i := range parts[:len(parts)-1] {
		parts[i] = strings.TrimSuffix(parts[i], "\r")
	}
	return parts
}

// promptKey is what the built-in prompts read of a `Terminal.UserInput`:
// the key's name, whether ctrl was held, and the text typed, "" for none.
type promptKey struct {
	name  string
	ctrl  bool
	input string
}

func promptKeyOf(input Value) promptKey {
	in := record(input)
	key := record(in.Get("key"))
	k := promptKey{name: key.Get("name").(string), ctrl: key.Get("ctrl").(bool)}
	if typed, ok := some(in.Get("input")); ok {
		k.input = typed.(string)
	}
	return k
}

func (k promptKey) is(names ...string) bool {
	for _, n := range names {
		if k.name == n {
			return true
		}
	}
	return false
}

func annotateSymbol(t *promptTheme, symbol string, styles ...string) string {
	if symbol == "" {
		return ""
	}
	return annotate(t.colors, symbol, styles...)
}

func separateSymbol(symbol, text string) string {
	if symbol == "" {
		return text
	}
	return symbol + " " + text
}

// leading and trailing are the symbols around a question: being asked, or
// answered.
func (t *promptTheme) leading(submitted bool) string {
	if submitted {
		return annotateSymbol(t, t.tick, t.success)
	}
	return annotateSymbol(t, t.prefix, t.primary)
}

func (t *promptTheme) trailing(submitted bool) string {
	if submitted {
		return annotateSymbol(t, t.ellipsis, t.muted)
	}
	return annotateSymbol(t, t.pointerSmall, t.muted)
}

// renderPrompt is the line of a question: the symbols, the message (bold
// unless plain) and what follows it.
func renderPrompt(confirm, message, leading, trailing string, t *promptTheme, plain bool) string {
	line := strings.Join(splitLines(message), "\n")
	if !plain {
		lines := splitLines(message)
		for i, l := range lines {
			lines[i] = annotate(t.colors, l, ansiBold)
		}
		line = strings.Join(lines, "\n")
	}
	out := line
	if leading != "" {
		out = leading + " " + line
	}
	if trailing != "" {
		out += " " + trailing
	}
	if confirm != "" {
		out += " " + confirm
	} else if trailing != "" {
		out += " "
	}
	return out
}

// eraseText erases the lines the text takes on a terminal of the width.
func eraseText(text string, columns int) string {
	if columns == 0 {
		return ansiEraseLine + "\x1b[1G"
	}
	rows := 0
	for _, line := range splitLines(text) {
		rows += 1 + max(jsLen(line)-1, 0)/columns
	}
	return ansiEraseLines(rows)
}

func lineCount(text string, columns int) int {
	lines := splitLines(text)
	if columns == 0 {
		return len(lines)
	}
	n := 0
	for _, line := range lines {
		n += (jsLen(line) + columns - 1) / columns
	}
	return n
}

func clearOutputWithError(output string, columns int, errorText string) string {
	if errorText != "" {
		return ansiCursorDown(lineCount(errorText, columns)) + eraseText("\n"+errorText, columns) +
			eraseText(output, columns)
	}
	return eraseText(output, columns)
}

// errorText is the error under a question: plain to erase it, else in the
// error color below the line, the cursor kept where it was.
func errorText(err *string, pointer string, t *promptTheme, plain bool) string {
	if err == nil {
		return ""
	}
	lines := splitLines(*err)
	if plain {
		return separateSymbol(pointer, strings.Join(lines, "\n"))
	}
	for i, l := range lines {
		lines[i] = annotate(t.colors, l, ansiItalic, t.errorColor)
	}
	prefix := annotateSymbol(t, pointer, t.errorColor)
	return ansiCursorSave + "\n" + separateSymbol(prefix, strings.Join(lines, "\n")) + ansiCursorRestore
}

// pagingPrefix marks the first or last line of a page with more above or
// below it.
func pagingPrefix(t *promptTheme, up, down bool) string {
	width := max(jsLen(t.arrowUp), jsLen(t.arrowDown))
	pad := func(s string) string { return s + strings.Repeat(" ", max(width-jsLen(s), 0)) }
	switch {
	case up:
		return pad(t.arrowUp)
	case down:
		return pad(t.arrowDown)
	}
	return strings.Repeat(" ", width)
}

// pageOf is the entries shown around the cursor: at most `perPage`.
func pageOf(cursor, total int, perPage *int) (start, end int) {
	most := total
	if perPage != nil {
		most = *perPage
	}
	start = min(total-most, cursor-floorDiv(most, 2))
	if start < 0 {
		start = 0
	}
	return start, min(start+most, total)
}

func floorDiv(a, b int) int {
	q := a / b
	if (a%b != 0) && ((a < 0) != (b < 0)) {
		q--
	}
	return q
}

// The options of the prompts, read from Flick records.

func optString(o *Record, name, otherwise string) string {
	if v, ok := some(o.Get(name)); ok {
		return v.(string)
	}
	return otherwise
}

func optInt(o *Record, name string) *int {
	if v, ok := some(o.Get(name)); ok {
		n := int(v.(int32))
		return &n
	}
	return nil
}

// optCheck is a Flick `validate`: the value it gives, or its message.
func optCheck(o *Record, name string) func(Value) (Value, *string) {
	f, ok := some(o.Get(name))
	if !ok {
		return nil
	}
	return func(v Value) (Value, *string) { return promptResult(Apply(f, v)) }
}

func promptResult(r Value) (Value, *string) {
	d := r.(*Data)
	if d.Tag == "Success" {
		return d.Fields[0], nil
	}
	message := d.Fields[0].(string)
	return nil, &message
}
