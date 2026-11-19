package flickrt

import (
	"sort"
	"strings"
)

// Effect's prompts of choices: select, auto-complete and multi-select.

type promptChoice struct {
	title, description string
	value              Value
	disabled, selected bool
}

func choicesOf(v Value) []promptChoice {
	out := make([]promptChoice, len(List(v)))
	for i, c := range List(v) {
		r := record(c)
		out[i] = promptChoice{title: r.Get("title").(string), value: r.Get("value"),
			description: optString(r, "description", ""),
			disabled:    r.Get("disabled").(bool), selected: r.Get("selected").(bool)}
	}
	return out
}

// initialChoice is the one choice selected at first: Effect allows one.
func initialChoice(choices []promptChoice) int {
	seen := -1
	for i, c := range choices {
		if c.selected {
			if seen != -1 {
				panic(Defect{"Error: InvalidArgumentException: only a single choice can be selected by default for Prompt.Select"})
			}
			seen = i
		}
	}
	return max(seen, 0)
}

type selectOptions struct {
	message *string // none draws no question line
	choices []promptChoice
	perPage *int
}

func selectOptionsOf(o *Record) *selectOptions {
	opts := &selectOptions{choices: choicesOf(o.Get("choices")), perPage: optInt(o, "maxPerPage")}
	if m, ok := some(o.Get("message")); ok {
		text := m.(string)
		opts.message = &text
	}
	if opts.perPage == nil {
		ten := 10
		opts.perPage = &ten
	}
	return opts
}

func (o *selectOptions) header(leading, trailing string, t *promptTheme, plain bool) *string {
	if o.message == nil {
		return nil
	}
	line := renderPrompt("", *o.message, leading, trailing, t, plain)
	return &line
}

func withHeader(header *string, body string) string {
	if header == nil {
		return body
	}
	return *header + "\n" + body
}

func choiceTitle(c promptChoice, selected bool, t *promptTheme, plain bool) string {
	switch {
	case plain:
		return c.title
	case selected && c.disabled:
		return annotate(t.colors, c.title, ansiUnderline, t.muted)
	case selected:
		return annotate(t.colors, c.title, ansiUnderline, t.primary)
	case c.disabled:
		return annotate(t.colors, c.title, ansiStrikethrough, t.muted)
	}
	return c.title
}

func choiceDescription(c promptChoice, active bool, t *promptTheme, plain bool) string {
	if c.disabled || c.description == "" || !active {
		return ""
	}
	text := t.descriptionSeparator + c.description
	if plain {
		return text
	}
	return annotate(t.colors, text, t.muted)
}

// choicePrefix is the pointer at the active choice, or its width of space.
func choicePrefix(active, disabled bool, paging string, t *promptTheme, plain bool) string {
	if !active {
		return paging + strings.Repeat(" ", jsLen(t.pointer))
	}
	switch {
	case plain:
		return t.pointer + paging
	case disabled:
		return annotateSymbol(t, t.pointer, ansiBold, t.muted) + paging
	}
	return annotateSymbol(t, t.pointer, t.primary) + paging
}

func (o *selectOptions) choiceLines(cursor int, t *promptTheme, plain bool) string {
	start, end := pageOf(cursor, len(o.choices), o.perPage)
	lines := make([]string, 0, end-start)
	for i := start; i < end; i++ {
		c := o.choices[i]
		paging := pagingPrefix(t, i == start && start > 0, i == end-1 && end < len(o.choices))
		lines = append(lines, choicePrefix(cursor == i, c.disabled, paging, t, plain)+
			choiceTitle(c, cursor == i, t, plain)+" "+choiceDescription(c, cursor == i, t, plain))
	}
	return strings.Join(lines, "\n")
}

// PromptSelect is `Cli.Prompt.select`: the state is the index of the choice.
func PromptSelect(options Value) Value {
	o := selectOptionsOf(record(options))
	first := initialChoice(o.choices)
	n := len(o.choices)
	return loopOp(&promptLoop{
		initial: func(*promptRunner) Value { return first },
		render: func(r *promptRunner, state Value, a promptAction) string {
			t := r.theme
			switch a.kind {
			case actBeep:
				return ansiBeep
			case actSubmit:
				head := t.leading(true)
				if h := o.header(t.leading(true), t.trailing(true), t, false); h != nil {
					head = *h
				}
				return head + " " + annotate(t.colors, o.choices[state.(int)].title, t.submittedColor) + "\n"
			}
			return ansiCursorHide + withHeader(o.header(t.leading(false), t.trailing(false), t, false),
				o.choiceLines(a.state.(int), t, false))
		},
		process: func(_ *promptRunner, input, state Value) promptAction {
			i := state.(int)
			switch promptKeyOf(input).name {
			case "k", "up":
				if i == 0 {
					return nextAction(n - 1)
				}
				return nextAction(i - 1)
			case "j", "down":
				if i == n-1 {
					return nextAction(0)
				}
				return nextAction(i + 1)
			case "tab":
				return nextAction((i + 1) % n)
			case "enter", "return":
				if o.choices[i].disabled {
					return beepAction()
				}
				return submitAction(o.choices[i].value)
			}
			return beepAction()
		},
		clear: func(r *promptRunner, state Value, _ promptAction) string {
			t := r.theme
			return eraseText(withHeader(o.header(t.prefix, t.pointerSmall, t, true), o.choiceLines(state.(int), t, true)),
				r.columns()) + promptResetLine
		},
	})
}

type autoState struct {
	query    string
	index    int
	filtered []int
}

type autoOptions struct {
	message, filterLabel, placeholder, empty string
	choices                                  []promptChoice
	perPage                                  *int
}

func (o *autoOptions) filter(query string) []int {
	q := ToLowerCase(query).(string)
	out := []int{}
	for i, c := range o.choices {
		if strings.Contains(ToLowerCase(c.title).(string), q) {
			out = append(out, i)
		}
	}
	return out
}

func (o *autoOptions) update(s autoState, query string) autoState {
	filtered := o.filter(query)
	switch {
	case len(filtered) == 0:
		return autoState{query: query, filtered: filtered}
	case indexIn(filtered, s.index) >= 0:
		return autoState{query: query, index: s.index, filtered: filtered}
	}
	return autoState{query: query, index: filtered[0], filtered: filtered}
}

func indexIn(xs []int, x int) int {
	for i, y := range xs {
		if y == x {
			return i
		}
	}
	return -1
}

func (s autoState) cursor() int { return max(indexIn(s.filtered, s.index), 0) }

func (o *autoOptions) output(s autoState, leading, trailing string, t *promptTheme, plain bool) string {
	value := s.query
	switch {
	case s.query == "" && plain:
		value = o.placeholder
	case s.query == "":
		value = annotate(t.colors, o.placeholder, t.muted)
	case !plain:
		value = annotate(t.colors, s.query, ansiUnderline, t.primary)
	}
	return renderPrompt("["+o.filterLabel+": "+value+"]", o.message, leading, trailing, t, plain)
}

func (o *autoOptions) choiceLines(s autoState, t *promptTheme, plain bool) string {
	if len(s.filtered) == 0 {
		if plain {
			return o.empty
		}
		return annotate(t.colors, o.empty, t.muted)
	}
	start, end := pageOf(s.cursor(), len(s.filtered), o.perPage)
	lines := make([]string, 0, end-start)
	for i := start; i < end; i++ {
		c := o.choices[s.filtered[i]]
		active := s.index == s.filtered[i]
		paging := pagingPrefix(t, i == start && start > 0, i == end-1 && end < len(s.filtered))
		lines = append(lines, choicePrefix(active, c.disabled, paging, t, plain)+
			choiceTitle(c, active, t, plain)+" "+choiceDescription(c, active, t, plain))
	}
	return strings.Join(lines, "\n")
}

func (o *autoOptions) move(s autoState, step int) promptAction {
	if len(s.filtered) == 0 {
		return beepAction()
	}
	next := (s.cursor() + step + len(s.filtered)) % len(s.filtered)
	return nextAction(autoState{query: s.query, index: s.filtered[next], filtered: s.filtered})
}

func (o *autoOptions) process(_ *promptRunner, input, state Value) promptAction {
	s, k := state.(autoState), promptKeyOf(input)
	if k.ctrl {
		switch k.name {
		case "u":
			return nextAction(o.update(s, ""))
		case "p", "k":
			return o.move(s, -1)
		case "n":
			return o.move(s, 1)
		}
		return beepAction()
	}
	switch k.name {
	case "up":
		return o.move(s, -1)
	case "down", "tab":
		return o.move(s, 1)
	case "backspace":
		if s.query == "" {
			return beepAction()
		}
		return nextAction(o.update(s, jsSlice(s.query, 0, jsLen(s.query)-1)))
	case "enter", "return":
		if len(s.filtered) == 0 || o.choices[s.index].disabled {
			return beepAction()
		}
		return submitAction(o.choices[s.index].value)
	}
	if k.input == "" {
		return beepAction()
	}
	return nextAction(o.update(s, s.query+k.input))
}

func PromptAutoComplete(options Value) Value {
	r := record(options)
	o := &autoOptions{message: r.Get("message").(string), choices: choicesOf(r.Get("choices")),
		perPage: optInt(r, "maxPerPage"), filterLabel: optString(r, "filterLabel", "filter"),
		placeholder: optString(r, "filterPlaceholder", "type to filter"),
		empty:       optString(r, "emptyMessage", "No matches")}
	if o.perPage == nil {
		ten := 10
		o.perPage = &ten
	}
	initial := initialChoice(o.choices)
	filtered := o.filter("")
	index := 0
	if len(filtered) > 0 {
		index = filtered[0]
		if indexIn(filtered, initial) >= 0 {
			index = initial
		}
	}
	first := autoState{index: index, filtered: filtered}
	return loopOp(&promptLoop{
		initial: func(*promptRunner) Value { return first },
		render: func(r *promptRunner, state Value, a promptAction) string {
			t := r.theme
			switch a.kind {
			case actBeep:
				return ansiBeep
			case actSubmit:
				s := state.(autoState)
				return o.output(s, t.leading(true), t.trailing(true), t, false) + " " +
					annotate(t.colors, o.choices[s.index].title, t.submittedColor) + "\n"
			}
			s := a.state.(autoState)
			return ansiCursorHide + o.output(s, t.leading(false), t.trailing(false), t, false) + "\n" +
				o.choiceLines(s, t, false)
		},
		process: o.process,
		clear: func(r *promptRunner, state Value, _ promptAction) string {
			s, t := state.(autoState), r.theme
			return eraseText(o.output(s, t.prefix, t.pointerSmall, t, true)+"\n"+o.choiceLines(s, t, true),
				r.columns()) + promptResetLine
		},
	})
}

type multiState struct {
	index    int
	selected map[int]bool
	err      *string
}

type multiOptions struct {
	*selectOptions
	selectAll, selectNone, inverse *string
	min, max                       *int
}

const metaOptionsCount = 2

func (o *multiOptions) counts(s multiState) (selectable, selected int) {
	for i, c := range o.choices {
		if !c.disabled {
			selectable++
			if s.selected[i] {
				selected++
			}
		}
	}
	return selectable, selected
}

func orText(s *string, otherwise string) string {
	if s != nil {
		return *s
	}
	return otherwise
}

func (o *multiOptions) choiceLines(s multiState, t *promptTheme, plain bool) string {
	width := max(jsLen(t.checkboxOn), jsLen(t.checkboxOff))
	selectable, selected := o.counts(s)
	meta := []string{orText(o.selectAll, "Select All"), orText(o.inverse, "Inverse Selection")}
	if selected == selectable {
		meta[0] = orText(o.selectNone, "Select None")
	}
	total := len(meta) + len(o.choices)
	start, end := pageOf(s.index, total, o.perPage)
	lines := make([]string, 0, end-start)
	for i := start; i < end; i++ {
		active := s.index == i
		prefix := pagingPrefix(t, i == start && start > 0, i == end-1 && end < total)
		if prefix != "" {
			prefix += " "
		}
		if i < len(meta) {
			title := meta[i]
			if !plain && active {
				title = annotate(t.colors, title, ansiUnderline, t.primary)
			}
			lines = append(lines, prefix+title)
			continue
		}
		c := o.choices[i-len(meta)]
		box := t.checkboxOff
		if s.selected[i-len(meta)] {
			box = t.checkboxOn
		}
		shown := box
		if active && !plain {
			shown = annotate(t.colors, box, t.primary)
		}
		if width > 0 {
			shown += strings.Repeat(" ", width-jsLen(box)+1)
		} else {
			shown = ""
		}
		lines = append(lines, prefix+shown+choiceTitle(c, active, t, plain)+" "+choiceDescription(c, active, t, plain))
	}
	return strings.Join(lines, "\n")
}

func (o *multiOptions) indices(s multiState) []int {
	out := []int{}
	for i := range s.selected {
		if !o.choices[i].disabled {
			out = append(out, i)
		}
	}
	sort.Ints(out)
	return out
}

func (o *multiOptions) process(_ *promptRunner, input, state Value) promptAction {
	s := state.(multiState)
	total := len(o.choices) + metaOptionsCount
	move := func(index int) promptAction {
		return nextAction(multiState{index: index, selected: s.selected})
	}
	switch promptKeyOf(input).name {
	case "k", "up":
		if s.index == 0 {
			return move(total - 1)
		}
		return move(s.index - 1)
	case "j", "down", "tab":
		return move((s.index + 1) % total)
	case "space":
		selected := make(map[int]bool, len(s.selected))
		for i := range s.selected {
			selected[i] = true
		}
		switch s.index {
		case 0:
			if selectable, n := o.counts(s); n == selectable {
				selected = map[int]bool{}
			} else {
				for i, c := range o.choices {
					if !c.disabled {
						selected[i] = true
					}
				}
			}
		case 1:
			for i, c := range o.choices {
				if c.disabled || s.selected[i] {
					delete(selected, i)
				} else {
					selected[i] = true
				}
			}
		default:
			i := s.index - metaOptionsCount
			switch {
			case o.choices[i].disabled:
				return beepAction()
			case selected[i]:
				delete(selected, i)
			default:
				selected[i] = true
			}
		}
		return nextAction(multiState{index: s.index, selected: selected, err: s.err})
	case "enter", "return":
		chosen := o.indices(s)
		fail := func(message string) promptAction {
			return nextAction(multiState{index: s.index, selected: s.selected, err: &message})
		}
		if o.min != nil && len(chosen) < *o.min {
			return fail("At least " + jsNumber(float64(*o.min)) + " are required")
		}
		if o.max != nil && len(chosen) > *o.max {
			return fail("At most " + jsNumber(float64(*o.max)) + " choices are allowed")
		}
		values := make([]Value, len(chosen))
		for j, i := range chosen {
			values[j] = o.choices[i].value
		}
		return submitAction(values)
	}
	return beepAction()
}

func PromptMultiSelect(options, multi Value) Value {
	m := record(multi)
	o := &multiOptions{selectOptions: selectOptionsOf(record(options)), min: optInt(m, "min"), max: optInt(m, "max")}
	for name, field := range map[string]**string{"selectAll": &o.selectAll, "selectNone": &o.selectNone,
		"inverseSelection": &o.inverse} {
		if v, ok := some(m.Get(name)); ok {
			text := v.(string)
			*field = &text
		}
	}
	first := multiState{selected: map[int]bool{}}
	for i, c := range o.choices {
		if c.selected && !c.disabled {
			first.selected[i] = true
		}
	}
	return loopOp(&promptLoop{
		initial: func(*promptRunner) Value { return first },
		render: func(r *promptRunner, state Value, a promptAction) string {
			t := r.theme
			switch a.kind {
			case actBeep:
				return ansiBeep
			case actSubmit:
				s := state.(multiState)
				indices := make([]int, 0, len(s.selected))
				for i := range s.selected {
					indices = append(indices, i)
				}
				sort.Ints(indices)
				titles := make([]string, len(indices))
				for j, i := range indices {
					titles[j] = o.choices[i].title
				}
				head := t.leading(true)
				if h := o.header(t.leading(true), t.trailing(true), t, false); h != nil {
					head = *h
				}
				return head + " " + annotate(t.colors, strings.Join(titles, ", "), t.submittedColor) + "\n"
			}
			s := a.state.(multiState)
			return ansiCursorHide + withHeader(o.header(t.leading(false), t.trailing(false), t, false),
				o.choiceLines(s, t, false)) + errorText(s.err, t.pointer, t, false)
		},
		process: o.process,
		clear: func(r *promptRunner, state Value, _ promptAction) string {
			s, t := state.(multiState), r.theme
			body := withHeader(o.header(t.prefix, t.pointerSmall, t, true), o.choiceLines(s, t, true))
			return clearOutputWithError(body, r.columns(), errorText(s.err, t.pointer, t, true)) + promptResetLine
		},
	})
}
