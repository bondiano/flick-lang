package flickrt

import (
	"slices"
	"strings"
	"unicode/utf8"
)

// The select, auto-complete and multiple selection prompts.

type promptChoice struct {
	title, description string
	value              Value
	disabled, selected bool
}

func promptChoices(v Value) []promptChoice {
	list := List(v)
	out := make([]promptChoice, len(list))
	for i, c := range list {
		r := c.(*Record)
		out[i] = promptChoice{r.Get("title").(string), promptOr(r.Get("description"), ""), r.Get("value"), r.Get("disabled").(bool), r.Get("selected").(bool)}
	}
	return out
}

func promptMaxPerPage(option Value) int {
	if v, ok := some(option); ok {
		return int(v.(int32))
	}
	return 10
}

// promptSelectInitial is the one choice selected by default, or the first.
func promptSelectInitial(choices []promptChoice) int {
	initial := -1
	for i, c := range choices {
		if c.selected {
			if initial != -1 {
				panic(Defect{"InvalidArgumentException: only a single choice can be selected by default for Prompt.Select"})
			}
			initial = i
		}
	}
	return max(initial, 0)
}

// promptWindow is the entries to display: the page around `cursor`.
func promptWindow(cursor, total, perPage int) (int, int) {
	start := max(min(total-perPage, cursor-perPage/2), 0)
	return start, min(start+perPage, total)
}

func promptPaging(t *promptTheme, up, down bool) string {
	width := max(utf8.RuneCountInString(t.arrowUp), utf8.RuneCountInString(t.arrowDown))
	pad := func(s string) string { return s + strings.Repeat(" ", width-utf8.RuneCountInString(s)) }
	switch {
	case up:
		return pad(t.arrowUp)
	case down:
		return pad(t.arrowDown)
	}
	return strings.Repeat(" ", width)
}

func promptChoicePrefix(t *promptTheme, paging string, active, disabled, plain bool) string {
	if !active {
		return paging + strings.Repeat(" ", utf8.RuneCountInString(t.pointer))
	}
	switch {
	case plain:
		return t.pointer + paging
	case disabled:
		return promptAnnotateSymbol(t, t.pointer, ansiBold, t.muted) + paging
	}
	return promptAnnotateSymbol(t, t.pointer, t.primary) + paging
}

func promptChoiceTitle(c promptChoice, active bool, t *promptTheme, plain bool) string {
	switch {
	case plain:
		return c.title
	case active && c.disabled:
		return ansiAnnotate(t.colors, c.title, ansiUnderline, t.muted)
	case active:
		return ansiAnnotate(t.colors, c.title, ansiUnderline, t.primary)
	case c.disabled:
		return ansiAnnotate(t.colors, c.title, ansiStrike, t.muted)
	}
	return c.title
}

func promptChoiceDescription(c promptChoice, active bool, t *promptTheme, plain bool) string {
	if c.disabled || c.description == "" || !active {
		return ""
	}
	text := t.descriptionSeparator + c.description
	if plain {
		return text
	}
	return ansiAnnotate(t.colors, text, t.muted)
}

// promptHeader is the message line of a selection: none without a message.
func promptHeader(message Value, leading, trailing string, t *promptTheme, plain bool) (string, bool) {
	m, ok := some(message)
	if !ok {
		return "", false
	}
	return promptRender("", m.(string), leading, trailing, t, plain), true
}

func promptWithHeader(header string, ok bool, body string) string {
	if !ok {
		return body
	}
	return header + "\n" + body
}

func promptSubmitted(t *promptTheme, header string, ok bool, answer string) string {
	if !ok {
		header = promptAnnotateSymbol(t, t.tick, t.success)
	}
	return header + " " + ansiAnnotate(t.colors, answer, t.submitted) + "\n"
}

func promptKeyName(input Value) (string, bool) {
	key := input.(*Record).Get("key").(*Record)
	return key.Get("name").(string), key.Get("ctrl").(bool)
}

// Select.

func PromptSelect(options Value) Value {
	o := options.(*Record)
	message, choices, perPage := o.Get("message"), promptChoices(o.Get("choices")), promptMaxPerPage(o.Get("maxPerPage"))
	initial := promptSelectInitial(choices)
	lines := func(t *promptTheme, state int, plain bool) string {
		start, end := promptWindow(state, len(choices), perPage)
		docs := make([]string, 0, end-start)
		for i := start; i < end; i++ {
			c, active := choices[i], state == i
			paging := promptPaging(t, i == start && start > 0, i == end-1 && end < len(choices))
			docs = append(docs, promptChoicePrefix(t, paging, active, c.disabled, plain)+promptChoiceTitle(c, active, t, plain)+" "+promptChoiceDescription(c, active, t, plain))
		}
		return strings.Join(docs, "\n")
	}
	return &promptLoop{
		initial: func(*promptCtx) Value { return initial },
		render: func(c *promptCtx, state Value, action promptAction) string {
			t := c.theme
			switch action.kind {
			case 'b':
				return ansiBeep
			case 's':
				header, ok := promptHeader(message, promptAnnotateSymbol(t, t.tick, t.success), promptAnnotateSymbol(t, t.ellipsis, t.muted), t, false)
				return promptSubmitted(t, header, ok, choices[state.(int)].title)
			}
			header, ok := promptHeader(message, promptAnnotateSymbol(t, t.prefix, t.primary), promptAnnotateSymbol(t, t.pointerSmall, t.muted), t, false)
			return ansiCursorHide + promptWithHeader(header, ok, lines(t, action.value.(int), false))
		},
		process: func(_ *promptCtx, input, state Value) promptAction {
			s := state.(int)
			name, _ := promptKeyName(input)
			switch name {
			case "k", "up":
				if s == 0 {
					return promptNext(len(choices) - 1)
				}
				return promptNext(s - 1)
			case "j", "down":
				if s == len(choices)-1 {
					return promptNext(0)
				}
				return promptNext(s + 1)
			case "tab":
				return promptNext((s + 1) % len(choices))
			case "enter", "return":
				if choices[s].disabled {
					return promptBeep
				}
				return promptSubmit(choices[s].value)
			}
			return promptBeep
		},
		clear: func(c *promptCtx, state Value, _ promptAction) string {
			t := c.theme
			header, ok := promptHeader(message, t.prefix, t.pointerSmall, t, true)
			return promptEraseText(promptWithHeader(header, ok, lines(t, state.(int), true)), c.columns()) + promptResetLine
		},
	}
}

// Auto-complete.

type promptAutoState struct {
	query    string
	index    int
	filtered []int
}

func PromptAutoComplete(options Value) Value {
	o := options.(*Record)
	message, choices, perPage := o.Get("message").(string), promptChoices(o.Get("choices")), promptMaxPerPage(o.Get("maxPerPage"))
	label, placeholder := promptOr(o.Get("filterLabel"), "filter"), promptOr(o.Get("filterPlaceholder"), "type to filter")
	empty := promptOr(o.Get("emptyMessage"), "No matches")
	filter := func(query string) []int {
		query = strings.ToLower(query)
		var out []int
		for i, c := range choices {
			if strings.Contains(strings.ToLower(c.title), query) {
				out = append(out, i)
			}
		}
		return out
	}
	position := func(s promptAutoState) int {
		for i, index := range s.filtered {
			if index == s.index {
				return i
			}
		}
		return 0
	}
	update := func(s promptAutoState, query string) promptAction {
		s.query, s.filtered = query, filter(query)
		if len(s.filtered) == 0 {
			s.index = 0
		} else if !slices.Contains(s.filtered, s.index) {
			s.index = s.filtered[0]
		}
		return promptNext(s)
	}
	move := func(s promptAutoState, by int) promptAction {
		if len(s.filtered) == 0 {
			return promptBeep
		}
		s.index = s.filtered[(position(s)+by+len(s.filtered))%len(s.filtered)]
		return promptNext(s)
	}
	output := func(t *promptTheme, s promptAutoState, leading, trailing string, plain bool) string {
		value := s.query
		switch {
		case s.query == "" && plain:
			value = placeholder
		case s.query == "":
			value = ansiAnnotate(t.colors, placeholder, t.muted)
		case !plain:
			value = ansiAnnotate(t.colors, s.query, ansiUnderline, t.primary)
		}
		return promptRender("["+label+": "+value+"]", message, leading, trailing, t, plain)
	}
	lines := func(t *promptTheme, s promptAutoState, plain bool) string {
		if len(s.filtered) == 0 {
			if plain {
				return empty
			}
			return ansiAnnotate(t.colors, empty, t.muted)
		}
		start, end := promptWindow(position(s), len(s.filtered), perPage)
		docs := make([]string, 0, end-start)
		for i := start; i < end; i++ {
			c, active := choices[s.filtered[i]], s.index == s.filtered[i]
			paging := promptPaging(t, i == start && start > 0, i == end-1 && end < len(s.filtered))
			docs = append(docs, promptChoicePrefix(t, paging, active, c.disabled, plain)+promptChoiceTitle(c, active, t, plain)+" "+promptChoiceDescription(c, active, t, plain))
		}
		return strings.Join(docs, "\n")
	}
	initial := promptAutoState{filtered: filter("")}
	if len(initial.filtered) > 0 {
		initial.index = promptSelectInitial(choices)
		initial = update(initial, "").value.(promptAutoState)
	}
	return &promptLoop{
		initial: func(*promptCtx) Value { return initial },
		render: func(c *promptCtx, state Value, action promptAction) string {
			t := c.theme
			switch action.kind {
			case 'b':
				return ansiBeep
			case 's':
				s := state.(promptAutoState)
				header := output(t, s, promptAnnotateSymbol(t, t.tick, t.success), promptAnnotateSymbol(t, t.ellipsis, t.muted), false)
				return promptSubmitted(t, header, true, choices[s.index].title)
			}
			s := action.value.(promptAutoState)
			return ansiCursorHide + output(t, s, promptAnnotateSymbol(t, t.prefix, t.primary), promptAnnotateSymbol(t, t.pointerSmall, t.muted), false) + "\n" + lines(t, s, false)
		},
		process: func(_ *promptCtx, input, state Value) promptAction {
			s := state.(promptAutoState)
			name, ctrl := promptKeyName(input)
			if ctrl {
				switch name {
				case "u":
					return update(s, "")
				case "p", "k":
					return move(s, -1)
				case "n":
					return move(s, 1)
				}
				return promptBeep
			}
			switch name {
			case "up":
				return move(s, -1)
			case "down", "tab":
				return move(s, 1)
			case "backspace":
				if s.query == "" {
					return promptBeep
				}
				query := []rune(s.query)
				return update(s, string(query[:len(query)-1]))
			case "enter", "return":
				if len(s.filtered) == 0 || choices[s.index].disabled {
					return promptBeep
				}
				return promptSubmit(choices[s.index].value)
			}
			typed := promptOr(input.(*Record).Get("input"), "")
			if typed == "" {
				return promptBeep
			}
			return update(s, s.query+typed)
		},
		clear: func(c *promptCtx, state Value, _ promptAction) string {
			s, t := state.(promptAutoState), c.theme
			return promptEraseText(output(t, s, t.prefix, t.pointerSmall, true)+"\n"+lines(t, s, true), c.columns()) + promptResetLine
		},
	}
}
