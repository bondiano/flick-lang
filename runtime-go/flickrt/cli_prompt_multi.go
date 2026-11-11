package flickrt

import (
	"strconv"
	"strings"
	"unicode/utf8"
)

// The multiple selection: two bulk actions above the choices.

type promptMultiState struct {
	index    int
	selected []bool // by choice; a new slice for each state
	err      *string
}

const promptMetaOptions = 2

func PromptMultiSelect(options, multi Value) Value {
	o, m := options.(*Record), multi.(*Record)
	message, choices, perPage := o.Get("message"), promptChoices(o.Get("choices")), promptMaxPerPage(o.Get("maxPerPage"))
	selectAll, selectNone := promptOr(m.Get("selectAll"), "Select All"), promptOr(m.Get("selectNone"), "Select None")
	inverse := promptOr(m.Get("inverseSelection"), "Inverse Selection")
	least, hasLeast := some(m.Get("min"))
	most, hasMost := some(m.Get("max"))
	total := len(choices) + promptMetaOptions
	counts := func(selected []bool) (selectable, chosen int) {
		for i, c := range choices {
			if !c.disabled {
				selectable++
				if selected[i] {
					chosen++
				}
			}
		}
		return
	}
	lines := func(t *promptTheme, s promptMultiState, plain bool) string {
		checkboxWidth := max(utf8.RuneCountInString(t.checkboxOn), utf8.RuneCountInString(t.checkboxOff))
		selectable, chosen := counts(s.selected)
		bulk := selectAll
		if chosen == selectable {
			bulk = selectNone
		}
		start, end := promptWindow(s.index, total, perPage)
		docs := make([]string, 0, end-start)
		for i := start; i < end; i++ {
			active := s.index == i
			paging := promptPaging(t, i == start && start > 0, i == end-1 && end < total)
			if paging != "" {
				paging += " "
			}
			if i < promptMetaOptions {
				title := []string{bulk, inverse}[i]
				if !plain && active {
					title = ansiAnnotate(t.colors, title, ansiUnderline, t.primary)
				}
				docs = append(docs, paging+title)
				continue
			}
			c := choices[i-promptMetaOptions]
			checkbox := t.checkboxOff
			if s.selected[i-promptMetaOptions] {
				checkbox = t.checkboxOn
			}
			box := ""
			if checkboxWidth > 0 {
				box = strings.Repeat(" ", checkboxWidth-utf8.RuneCountInString(checkbox)+1)
				if active && !plain {
					box = ansiAnnotate(t.colors, checkbox, t.primary) + box
				} else {
					box = checkbox + box
				}
			}
			docs = append(docs, paging+box+promptChoiceTitle(c, active, t, plain)+" "+promptChoiceDescription(c, active, t, plain))
		}
		return strings.Join(docs, "\n")
	}
	initial := promptMultiState{selected: make([]bool, len(choices))}
	for i, c := range choices {
		initial.selected[i] = c.selected && !c.disabled
	}
	return &promptLoop{
		initial: func(*promptCtx) Value { return initial },
		render: func(c *promptCtx, state Value, action promptAction) string {
			t := c.theme
			switch action.kind {
			case 'b':
				return ansiBeep
			case 's':
				var titles []string
				for i, on := range state.(promptMultiState).selected {
					if on {
						titles = append(titles, choices[i].title)
					}
				}
				header, ok := promptHeader(message, promptAnnotateSymbol(t, t.tick, t.success), promptAnnotateSymbol(t, t.ellipsis, t.muted), t, false)
				return promptSubmitted(t, header, ok, strings.Join(titles, ", "))
			}
			s := action.value.(promptMultiState)
			header, ok := promptHeader(message, promptAnnotateSymbol(t, t.prefix, t.primary), promptAnnotateSymbol(t, t.pointerSmall, t.muted), t, false)
			return ansiCursorHide + promptWithHeader(header, ok, lines(t, s, false)) + promptError(s.err, t.pointer, t, false)
		},
		process: func(_ *promptCtx, input, state Value) promptAction {
			s := state.(promptMultiState)
			name, _ := promptKeyName(input)
			switch name {
			case "k", "up":
				return promptNext(promptMultiState{(s.index - 1 + total) % total, s.selected, nil})
			case "j", "down", "tab":
				return promptNext(promptMultiState{(s.index + 1) % total, s.selected, nil})
			case "space":
				selected := append([]bool{}, s.selected...)
				switch s.index {
				case 0:
					selectable, chosen := counts(s.selected)
					for i, c := range choices {
						selected[i] = chosen != selectable && (selected[i] || !c.disabled)
					}
				case 1:
					for i, c := range choices {
						selected[i] = !c.disabled && !s.selected[i]
					}
				default:
					i := s.index - promptMetaOptions
					if choices[i].disabled {
						return promptBeep
					}
					selected[i] = !selected[i]
				}
				return promptNext(promptMultiState{s.index, selected, s.err})
			case "enter", "return":
				var values []Value
				for i, on := range s.selected {
					if on && !choices[i].disabled {
						values = append(values, choices[i].value)
					}
				}
				var message string
				if hasLeast && len(values) < int(least.(int32)) {
					message = "At least " + strconv.Itoa(int(least.(int32))) + " are required"
				} else if hasMost && len(values) > int(most.(int32)) {
					message = "At most " + strconv.Itoa(int(most.(int32))) + " choices are allowed"
				}
				if message != "" {
					return promptNext(promptMultiState{s.index, s.selected, &message})
				}
				if values == nil {
					values = []Value{}
				}
				return promptSubmit(values)
			}
			return promptBeep
		},
		clear: func(c *promptCtx, state Value, _ promptAction) string {
			s, t := state.(promptMultiState), c.theme
			header, ok := promptHeader(message, t.prefix, t.pointerSmall, t, true)
			output := promptWithHeader(header, ok, lines(t, s, true))
			return promptClearWithError(output, c.columns(), promptError(s.err, t.pointer, t, true)) + promptResetLine
		},
	}
}
