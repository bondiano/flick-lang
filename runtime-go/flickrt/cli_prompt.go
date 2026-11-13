package flickrt

import (
	"os"
	"runtime"
	"strconv"
	"strings"
)

// Effect's prompts (`effect/cli`'s Prompt) on the go target. A prompt is a
// tree of loops, each drawing frames of a state and stepping it on key
// presses, and of the steps between them. `run` asks them on the service
// `Cli.Prompt.Tty`, reading a key press each time a loop waits for one, as
// the JS runtime's runner does, so keys a prompt does not need stay with
// the service for the next one.

// The steps of a loop: ring the bell, draw the next state, or end.
const (
	actBeep = iota
	actNext
	actSubmit
)

type promptAction struct {
	kind  int
	state Value // of a next frame
	value Value // of a submission
}

func beepAction() promptAction              { return promptAction{kind: actBeep} }
func nextAction(state Value) promptAction   { return promptAction{kind: actNext, state: state} }
func submitAction(value Value) promptAction { return promptAction{kind: actSubmit, value: value} }

// promptLoop is Effect's `Loop`: the first state, and how a state is drawn
// after an action, stepped on an input, and erased before the next frame.
type promptLoop struct {
	initial func(r *promptRunner) Value
	render  func(r *promptRunner, state Value, a promptAction) string
	process func(r *promptRunner, input, state Value) promptAction
	clear   func(r *promptRunner, state Value, a promptAction) string
	events  *Queue // of `custom_events`, raced with the key presses
}

// promptOp is a `Cli.Prompt.Prompt`: a loop, a prompt and what follows its
// answer, or an answer.
type promptOp struct {
	loop   *promptLoop
	prompt *promptOp
	then   func(Value) *promptOp
	value  Value
}

func promptOf(v Value) *promptOp { return v.(*promptOp) }

func loopOp(l *promptLoop) Value { return &promptOp{loop: l} }

// promptTty is the terminal a prompt runs on.
type promptTty interface {
	readKey(fb *Fiber) Value // a `Terminal.UserInput`; fails with `Terminal.QuitError`
	display(fb *Fiber, text string)
	columns(fb *Fiber) int
}

// promptService is the terminal of a `Cli.Prompt.Tty` record.
type promptService struct{ impl *Record }

func (s promptService) call(fb *Fiber, op string, arg Value) Value {
	return Run(fb, Apply(s.impl.Get(op), arg))
}
func (s promptService) readKey(fb *Fiber) Value        { return s.call(fb, "read_key", Unit) }
func (s promptService) display(fb *Fiber, text string) { s.call(fb, "display", text) }
func (s promptService) columns(fb *Fiber) int {
	return int(s.call(fb, "columns", Unit).(int32))
}

// promptRunner is what a loop's handlers see, as Effect's do through its
// services: the fiber, the theme and the terminal.
type promptRunner struct {
	fb    *Fiber
	theme *promptTheme
	tty   promptTty
}

func (r *promptRunner) columns() int { return r.tty.columns(r.fb) }

func (r *promptRunner) run(op *promptOp) Value {
	switch {
	case op.loop != nil:
		return r.runLoop(op.loop)
	case op.prompt != nil:
		return r.run(op.then(r.run(op.prompt)))
	}
	return op.value
}

var (
	processInputTag = Tag("Cli.Prompt.ProcessInput.Input")
	processEventTag = Tag("Cli.Prompt.ProcessInput.Event")
)

// runLoop draws the frames of a loop until it submits; the cursor shows
// again however it ends.
func (r *promptRunner) runLoop(l *promptLoop) Value {
	// as Effect's `ensuring` of an ignored display: uninterruptible, its failure dropped
	defer catch(func() Value {
		return masked(r.fb, r.fb.state.mask+1, func() Value {
			r.tty.display(r.fb, ansiCursorShow)
			return Unit
		})
	})
	state := l.initial(r)
	action := nextAction(state)
	clear := ""
	for {
		r.tty.display(r.fb, clear+l.render(r, state, action))
		clear = ""
		var input Value
		if l.events == nil {
			input = r.tty.readKey(r.fb)
		} else {
			key := Eff(func(fb *Fiber) Value { return NewData(processInputTag, r.tty.readKey(fb)) })
			event := Eff(func(fb *Fiber) Value { return NewData(processEventTag, l.events.take(fb)) })
			input = raceAll(r.fb, []Value{key, event}, true)
		}
		action = l.process(r, input, state)
		if action.kind == actBeep {
			continue
		}
		clear = l.clear(r, state, action)
		if action.kind == actNext {
			state = action.state
			continue
		}
		r.tty.display(r.fb, clear+l.render(r, state, action))
		return action.value
	}
}

// promptThemeKey is where `run_themed` puts its theme, as Effect's
// reference `Prompt.Theme`.
const promptThemeKey = "effect/cli/Prompt/Theme"

func runPrompt(fb *Fiber, tty promptTty, prompt Value) Value {
	theme, ok := fb.env.get(promptThemeKey)
	if !ok {
		theme = PromptDefaultTheme()
	}
	r := &promptRunner{fb: fb, theme: themeOf(theme), tty: tty}
	return r.run(promptOf(prompt))
}

// PromptRun is `Cli.Prompt.run` on the service of the key.
func PromptRun(fb *Fiber, key, prompt Value) Value {
	impl, ok := fb.env.get(key.(string))
	if !ok {
		panic(Defect{"Service not found: " + key.(string)})
	}
	return runPrompt(fb, promptService{impl.(*Record)}, prompt)
}

func PromptRunThemed(fb *Fiber, key, theme, prompt Value) Value {
	return withEnv(fb, promptThemeKey, theme, Eff(func(fb *Fiber) Value { return PromptRun(fb, key, prompt) }))
}

// promptRunValue runs a `Cli.Prompt.Prompt` on the terminal of the process:
// quit is true when it ended with `Terminal.QuitError`.
func promptRunValue(fb *Fiber, prompt Value) (value Value, quit bool) {
	out, cause := catch(func() Value {
		tty := openTty()
		defer tty.close()
		return runPrompt(fb, tty, prompt)
	})
	if cause != nil {
		if cause.kind == causeFail && isQuitError(cause.err) {
			return nil, true
		}
		panic(&Failure{cause})
	}
	return out, false
}

func isQuitError(err Value) bool {
	d, ok := err.(*Data)
	return ok && d.Tag == quitErrorTag
}

// promptTheme is a `Cli.Prompt.Theme`.
type promptTheme struct {
	prefix, pointer, pointerSmall, ellipsis, tick, arrowUp, arrowDown    string
	checkboxOn, checkboxOff, descriptionSeparator, passwordMask          string
	toggleSeparator, primary, muted, success, errorColor, submittedColor string
	colors                                                               bool
}

func themeOf(v Value) *promptTheme {
	t := record(v)
	s := func(name string) string { return t.Get(name).(string) }
	return &promptTheme{
		prefix: s("prefix"), pointer: s("pointer"), pointerSmall: s("pointerSmall"),
		ellipsis: s("ellipsis"), tick: s("tick"), arrowUp: s("arrowUp"), arrowDown: s("arrowDown"),
		checkboxOn: s("checkboxOn"), checkboxOff: s("checkboxOff"),
		descriptionSeparator: s("descriptionSeparator"), passwordMask: s("passwordMask"),
		toggleSeparator: s("toggleSeparator"), primary: s("primaryColor"), muted: s("mutedColor"),
		success: s("successColor"), errorColor: s("errorColor"), submittedColor: s("submittedColor"),
		colors: t.Get("colors").(bool),
	}
}

// PromptDefaultTheme is Effect's `makeTheme()`: windows' plainer symbols
// there, and colors when stdout is a terminal and NO_COLOR is not set.
func PromptDefaultTheme() Value {
	checkboxOn, checkboxOff, tick, ellipsis, pointerSmall, pointer := "☒", "☐", "✔", "…", "›", "❯"
	if runtime.GOOS == "windows" {
		checkboxOn, checkboxOff, tick, ellipsis, pointerSmall, pointer = "[*]", "[ ]", "√", "...", "»", ">"
	}
	colors := isTTY(os.Stdout.Fd()) && os.Getenv("NO_COLOR") == ""
	return Rec("prefix", "?", "arrowUp", "↑", "arrowDown", "↓", "checkboxOn", checkboxOn,
		"checkboxOff", checkboxOff, "tick", tick, "ellipsis", ellipsis, "pointerSmall", pointerSmall,
		"pointer", pointer, "descriptionSeparator", "- ", "passwordMask", "*", "toggleSeparator", "/",
		"primaryColor", ansiCyanBright, "mutedColor", ansiBlackBright, "successColor", ansiGreen,
		"errorColor", ansiRed, "submittedColor", ansiWhite, "colors", colors)
}

// The combinators of prompts.

func PromptSucceed(value Value) Value { return &promptOp{value: value} }

func PromptFlatMap(f, prompt Value) Value {
	return &promptOp{prompt: promptOf(prompt), then: func(a Value) *promptOp { return promptOf(Apply(f, a)) }}
}

func PromptMap(f, prompt Value) Value {
	return &promptOp{prompt: promptOf(prompt), then: func(a Value) *promptOp { return &promptOp{value: Apply(f, a)} }}
}

// PromptAll asks the prompts in turn, as Effect's `all` of an array.
func PromptAll(prompts Value) Value {
	ps := List(prompts)
	if len(ps) == 0 {
		return PromptSucceed([]Value{})
	}
	result := PromptMap(Fn(func(x Value) Value { return []Value{x} }), ps[0])
	for _, p := range ps[1:] {
		result = PromptFlatMap(Fn(func(tuple Value) Value {
			return PromptMap(Fn(func(a Value) Value { return ListConcat(tuple, []Value{a}) }), p)
		}), result)
	}
	return result
}

// The Flick actions of custom prompts.
var (
	actionBeepTag   = Tag("Cli.Prompt.Action.Beep")
	actionNextTag   = Tag("Cli.Prompt.Action.NextFrame")
	actionSubmitTag = Tag("Cli.Prompt.Action.Submit")
)

func flickAction(a promptAction) Value {
	switch a.kind {
	case actNext:
		return NewData(actionNextTag, a.state)
	case actSubmit:
		return NewData(actionSubmitTag, a.value)
	}
	return Nullary(actionBeepTag)
}

func actionOf(v Value) promptAction {
	d := v.(*Data)
	switch d.Tag {
	case actionNextTag:
		return nextAction(d.Fields[0])
	case actionSubmitTag:
		return submitAction(d.Fields[0])
	}
	return beepAction()
}

// customLoop is the loop of `Cli.Prompt.Handlers`, pure Flick functions.
func customLoop(state, handlers Value, events *Queue) Value {
	h := record(handlers)
	return loopOp(&promptLoop{
		initial: func(*promptRunner) Value { return state },
		render: func(_ *promptRunner, s Value, a promptAction) string {
			return apply2(h.Get("render"), s, flickAction(a)).(string)
		},
		process: func(_ *promptRunner, input, s Value) promptAction {
			return actionOf(apply2(h.Get("process"), input, s))
		},
		clear: func(_ *promptRunner, s Value, a promptAction) string {
			return apply2(h.Get("clear"), s, flickAction(a)).(string)
		},
		events: events,
	})
}

func PromptCustom(state, handlers Value) Value { return customLoop(state, handlers, nil) }

func PromptCustomEvents(state, events, handlers Value) Value {
	return customLoop(state, handlers, events.(*Queue))
}

// The ANSI escapes Effect's prompts draw with.
const (
	ansiReset         = "\x1b[0m"
	ansiBold          = "\x1b[1m"
	ansiItalic        = "\x1b[3m"
	ansiUnderline     = "\x1b[4m"
	ansiStrikethrough = "\x1b[9m"
	ansiCursorShow    = "\x1b[?25h"
	ansiCursorHide    = "\x1b[?25l"
	ansiCursorLeft    = "\x1b[G"
	ansiCursorSave    = "\x1b[s"
	ansiCursorRestore = "\x1b[u"
	ansiEraseLine     = "\x1b[2K"
	ansiBeep          = "\x07"
	ansiRed           = "\x1b[31m"
	ansiGreen         = "\x1b[32m"
	ansiWhite         = "\x1b[37m"
	ansiBlackBright   = "\x1b[90m"
	ansiCyanBright    = "\x1b[96m"
	promptResetLine   = ansiEraseLine + ansiCursorLeft
)

func annotate(colors bool, text string, styles ...string) string {
	if !colors {
		return text
	}
	return strings.Join(styles, "") + text + ansiReset
}

func ansiCursorDown(lines int) string { return "\x1b[" + strconv.Itoa(lines) + "B" }

// ansiCursorMove moves the cursor by columns and rows.
func ansiCursorMove(column, row int) string {
	out := ""
	if row < 0 {
		out += "\x1b[" + strconv.Itoa(-row) + "A"
	}
	if row > 0 {
		out += "\x1b[" + strconv.Itoa(row) + "B"
	}
	if column > 0 {
		out += "\x1b[" + strconv.Itoa(column) + "C"
	}
	if column < 0 {
		out += "\x1b[" + strconv.Itoa(-column) + "D"
	}
	return out
}

func ansiEraseLines(rows int) string {
	out := ""
	for i := 0; i < rows; i++ {
		out += ansiEraseLine
		if i < rows-1 {
			out += "\x1b[1A"
		}
	}
	if rows > 0 {
		out += ansiCursorLeft
	}
	return out
}
