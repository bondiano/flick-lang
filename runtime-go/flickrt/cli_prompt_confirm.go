package flickrt

import "regexp"

// Effect's yes/no prompts: confirm, typed, and toggle, moved.

var (
	confirmTrue  = regexp.MustCompile(`^y|t$`)
	confirmFalse = regexp.MustCompile(`^n|f$`)
)

// PromptConfirm is `Cli.Prompt.confirm`; return answers the initial value.
func PromptConfirm(options Value) Value {
	o := record(options)
	message := o.Get("message").(string)
	initial := false
	if v, ok := some(o.Get("initial")); ok {
		initial = v.(bool)
	}
	yes, no := "yes", "no"
	if l, ok := some(o.Get("label")); ok {
		yes, no = record(l).Get("confirm").(string), record(l).Get("deny").(string)
	}
	placeYes, placeNo := "(Y/n)", "(y/N)"
	if p, ok := some(o.Get("placeholder")); ok {
		placeYes = optString(record(p), "defaultConfirm", placeYes)
		placeNo = optString(record(p), "defaultDeny", placeNo)
	}
	placeholder := func(value bool) string {
		if value {
			return placeYes
		}
		return placeNo
	}
	return loopOp(&promptLoop{
		initial: func(*promptRunner) Value { return initial },
		render: func(r *promptRunner, _ Value, a promptAction) string {
			t := r.theme
			switch a.kind {
			case actBeep:
				return ansiBeep
			case actSubmit:
				answer := no
				if a.value.(bool) {
					answer = yes
				}
				return renderPrompt(answer, message, t.leading(true), t.trailing(true), t, false) + "\n"
			}
			hint := annotate(t.colors, placeholder(a.state.(bool)), t.muted)
			return ansiCursorHide + renderPrompt(hint, message, t.leading(false), t.trailing(false), t, false)
		},
		process: func(_ *promptRunner, input, _ Value) promptAction {
			k := promptKeyOf(input)
			typed := ToLowerCase(k.input).(string)
			switch {
			case k.is("enter", "return"):
				return submitAction(initial)
			case confirmTrue.MatchString(typed):
				return submitAction(true)
			case confirmFalse.MatchString(typed):
				return submitAction(false)
			}
			return beepAction()
		},
		clear: func(r *promptRunner, state Value, _ promptAction) string {
			t := r.theme
			text := renderPrompt(placeholder(state.(bool)), message, t.prefix, t.pointerSmall, t, true)
			return eraseText(text, r.columns()) + promptResetLine
		},
	})
}

// PromptToggle is `Cli.Prompt.toggle`. As Effect's, it names the space
// key " ", which Node calls "space", so only tab flips it.
func PromptToggle(options Value) Value {
	o := record(options)
	message := o.Get("message").(string)
	initial := false
	if v, ok := some(o.Get("initial")); ok {
		initial = v.(bool)
	}
	active, inactive := optString(o, "active", "on"), optString(o, "inactive", "off")
	toggle := func(value bool, t *promptTheme, submitted bool) string {
		separator := annotateSymbol(t, t.toggleSeparator, t.muted)
		color := t.primary
		if submitted {
			color = t.submittedColor
		}
		on, off := active, inactive
		if value {
			on = annotate(t.colors, active, ansiUnderline, color)
		} else {
			off = annotate(t.colors, inactive, ansiUnderline, color)
		}
		return on + " " + separateSymbol(separator, off)
	}
	return loopOp(&promptLoop{
		initial: func(*promptRunner) Value { return initial },
		render: func(r *promptRunner, state Value, a promptAction) string {
			t := r.theme
			switch a.kind {
			case actBeep:
				return ansiBeep
			case actSubmit:
				return renderPrompt(toggle(state.(bool), t, true), message, t.leading(true), t.trailing(true), t, false) + "\n"
			}
			return ansiCursorHide + renderPrompt(toggle(state.(bool), t, false), message, t.leading(false),
				t.trailing(false), t, false)
		},
		process: func(_ *promptRunner, input, state Value) promptAction {
			switch promptKeyOf(input).name {
			case "0", "j", "delete", "right", "down":
				return nextAction(false)
			case "1", "k", "left", "up":
				return nextAction(true)
			case " ", "tab":
				return nextAction(!state.(bool))
			case "enter", "return":
				return submitAction(state)
			}
			return beepAction()
		},
		clear: func(r *promptRunner, _ Value, _ promptAction) string {
			t := r.theme
			text := renderPrompt(active+" "+separateSymbol(t.toggleSeparator, inactive), message, t.prefix,
				t.pointerSmall, t, true)
			return eraseText(text, r.columns()) + promptResetLine
		},
	})
}
