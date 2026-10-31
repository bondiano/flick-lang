package flickrt

import (
	"os"
	"runtime"
	"strconv"
	"strings"
	"sync"
)

// `Cli.Prompt`: Effect's prompts as a tree of three nodes, run on the
// service `Cli.Prompt.Tty`. The runner reads one key press each time a
// prompt waits for one, so the keys a prompt does not need stay with the
// service for the next prompt, as the JS runner does.

// A promptAction is a step's outcome: ring the bell, draw the next state,
// or end with a value.
type promptAction struct {
	kind  byte // 'b', 'n' or 's'
	value Value
}

var promptBeep = promptAction{kind: 'b'}

func promptNext(state Value) promptAction   { return promptAction{'n', state} }
func promptSubmit(value Value) promptAction { return promptAction{'s', value} }

// promptCtx is what a step sees: its fiber, the terminal and the theme.
type promptCtx struct {
	fb    *Fiber
	tty   *Record
	theme *promptTheme
}

func (c *promptCtx) columns() int { return int(promptCall(c.fb, c.tty, "columns", Unit).(int32)) }

// promptLoop is Effect's `Loop`: a render loop from a first state.
type promptLoop struct {
	initial func(*promptCtx) Value
	render  func(c *promptCtx, state Value, action promptAction) string
	process func(c *promptCtx, input, state Value) promptAction
	clear   func(c *promptCtx, state Value, action promptAction) string
	events  *Queue // with one, a message is an input too
}

// promptThen is Effect's `OnSuccess`, promptDone its `Succeed`, and
// promptAll the prompts one after another.
type promptThen struct {
	prompt Value
	next   func(*Fiber, Value) Value
}

type promptDone struct{ value Value }

type promptAll []Value

func promptCall(fb *Fiber, tty *Record, name string, argument Value) Value {
	return Run(fb, Apply(fb, tty.Get(name), argument))
}

func promptTty(fb *Fiber, key Value) *Record {
	impl, ok := fb.env.get(key.(string))
	if !ok {
		panic(Defect{"Service not found: " + key.(string)})
	}
	return impl.(*Record)
}

func PromptRun(fb *Fiber, key, prompt Value) Value {
	return promptRunOp(&promptCtx{fb, promptTty(fb, key), promptCurrentTheme()}, prompt)
}

func PromptRunThemed(fb *Fiber, key, theme, prompt Value) Value {
	return promptRunOp(&promptCtx{fb, promptTty(fb, key), promptThemeOf(theme)}, prompt)
}

func promptRunOp(c *promptCtx, prompt Value) Value {
	for {
		switch op := prompt.(type) {
		case *promptDone:
			return op.value
		case *promptLoop:
			return promptRunLoop(c, op)
		case promptAll:
			out := make([]Value, len(op))
			for i, p := range op {
				out[i] = promptRunOp(c, p)
			}
			return out
		case *promptThen:
			prompt = op.next(c.fb, promptRunOp(c, op.prompt))
		}
	}
}

// promptRunLoop is Effect's `runLoop`; the cursor is shown again however
// the loop ends.
func promptRunLoop(c *promptCtx, loop *promptLoop) Value {
	out, cause := catch(func() Value {
		state := loop.initial(c)
		action := promptNext(state)
		clear := ""
		for {
			Check(c.fb)
			promptCall(c.fb, c.tty, "display", clear+loop.render(c, state, action))
			clear = ""
			action = loop.process(c, promptInputOf(c, loop), state)
			switch action.kind {
			case 'n':
				clear = loop.clear(c, state, action)
				state = action.value
			case 's':
				clear = loop.clear(c, state, action)
				promptCall(c.fb, c.tty, "display", clear+loop.render(c, state, action))
				return action.value
			}
		}
	})
	_, cleanup := catch(func() Value {
		return masked(c.fb, c.fb.state.mask+1, func() Value {
			return promptCall(c.fb, c.tty, "display", ansiCursorShow)
		})
	})
	if cause = Sequential(cause, cleanup); cause != nil {
		panic(&Failure{cause})
	}
	return out
}

var (
	promptInputTag = Tag("Cli.Prompt.ProcessInput.Input")
	promptEventTag = Tag("Cli.Prompt.ProcessInput.Event")
)

// promptInputOf is the next key press, or with a queue of events the
// first of a key press and a message, as `ProcessInput`.
func promptInputOf(c *promptCtx, loop *promptLoop) Value {
	if loop.events == nil {
		return promptCall(c.fb, c.tty, "read_key", Unit)
	}
	// a message already waiting wins without starting a key read
	if v, ok := loop.events.poll(); ok {
		return NewData(promptEventTag, v)
	}
	key := Eff(func(fb *Fiber) Value { return NewData(promptInputTag, promptCall(fb, c.tty, "read_key", Unit)) })
	event := Eff(func(fb *Fiber) Value { return NewData(promptEventTag, loop.events.take(fb)) })
	return RaceFirst(c.fb, key, event)
}

// promptFallback runs `prompt` with the current theme on the terminal of
// the process, as `Param.withFallbackPrompt` runs `Prompt.run`; false when
// the user quits or there is no input left.
func promptFallback(fb *Fiber, prompt Value) (Value, bool) {
	tty := acquirePromptTty()
	defer releasePromptTty()
	out, cause := catch(func() Value {
		return promptRunOp(&promptCtx{fb, promptProcessService(tty), promptCurrentTheme()}, prompt)
	})
	if cause != nil {
		if cause.kind == causeFail && cause.err == promptQuit {
			return nil, false
		}
		panic(&Failure{cause})
	}
	return out, true
}

// Combinators.

func PromptSucceed(value Value) Value { return &promptDone{value} }

func PromptMap(f, prompt Value) Value {
	return &promptThen{prompt, func(fb *Fiber, a Value) Value { return &promptDone{Apply(fb, f, a)} }}
}

func PromptFlatMap(f, prompt Value) Value {
	return &promptThen{prompt, func(fb *Fiber, a Value) Value { return Apply(fb, f, a) }}
}

func PromptAll(prompts Value) Value { return promptAll(List(prompts)) }

func promptMapGo(prompt Value, f func(Value) Value) Value {
	return &promptThen{prompt, func(_ *Fiber, a Value) Value { return &promptDone{f(a)} }}
}

var (
	promptBeepTag   = Tag("Cli.Prompt.Action.Beep")
	promptNextTag   = Tag("Cli.Prompt.Action.NextFrame")
	promptSubmitTag = Tag("Cli.Prompt.Action.Submit")
)

func promptActionToFlick(a promptAction) Value {
	switch a.kind {
	case 'n':
		return NewData(promptNextTag, a.value)
	case 's':
		return NewData(promptSubmitTag, a.value)
	}
	return Nullary(promptBeepTag)
}

func promptActionOf(v Value) promptAction {
	switch d := v.(*Data); d.Tag {
	case promptNextTag:
		return promptNext(d.Fields[0])
	case promptSubmitTag:
		return promptSubmit(d.Fields[0])
	}
	return promptBeep
}

func promptCustomLoop(state, handlers Value, events *Queue) Value {
	h := handlers.(*Record)
	draw := func(name string) func(*promptCtx, Value, promptAction) string {
		return func(c *promptCtx, s Value, a promptAction) string {
			return Apply(c.fb, Apply(c.fb, h.Get(name), s), promptActionToFlick(a)).(string)
		}
	}
	return &promptLoop{
		initial: func(*promptCtx) Value { return state },
		render:  draw("render"),
		process: func(c *promptCtx, input, s Value) promptAction {
			return promptActionOf(Apply(c.fb, Apply(c.fb, h.Get("process"), input), s))
		},
		clear:  draw("clear"),
		events: events,
	}
}

func PromptCustom(state, handlers Value) Value { return promptCustomLoop(state, handlers, nil) }

func PromptCustomEvents(state, events, handlers Value) Value {
	return promptCustomLoop(state, handlers, events.(*Queue))
}

// Themes.

type promptTheme struct {
	prefix, pointer, pointerSmall, ellipsis, tick, arrowUp, arrowDown      string
	checkboxOn, checkboxOff, descriptionSeparator, passwordMask, toggleSep string
	primary, muted, success, error, submitted                              string
	colors                                                                 bool
}

var promptThemeFields = []string{
	"prefix", "pointer", "pointerSmall", "ellipsis", "tick", "arrowUp", "arrowDown",
	"checkboxOn", "checkboxOff", "descriptionSeparator", "passwordMask", "toggleSeparator",
	"primaryColor", "mutedColor", "successColor", "errorColor", "submittedColor",
}

func (t *promptTheme) strings() []*string {
	return []*string{
		&t.prefix, &t.pointer, &t.pointerSmall, &t.ellipsis, &t.tick, &t.arrowUp, &t.arrowDown,
		&t.checkboxOn, &t.checkboxOff, &t.descriptionSeparator, &t.passwordMask, &t.toggleSep,
		&t.primary, &t.muted, &t.success, &t.error, &t.submitted,
	}
}

func promptThemeOf(v Value) *promptTheme {
	r, t := v.(*Record), &promptTheme{}
	for i, p := range t.strings() {
		*p = r.Get(promptThemeFields[i]).(string)
	}
	t.colors = r.Get("colors").(bool)
	return t
}

// promptMakeTheme is Effect's `makeTheme()`: the Windows symbols on
// Windows, colors when stdout is a terminal and NO_COLOR is unset or empty.
func promptMakeTheme() *promptTheme {
	t := &promptTheme{"?", "❯", "›", "…", "✔", "↑", "↓", "☒", "☐", "- ", "*", "/",
		ansiCyanBright, ansiBlackBright, ansiGreen, ansiRed, ansiWhite, false}
	if runtime.GOOS == "windows" {
		t.checkboxOn, t.checkboxOff, t.tick, t.ellipsis, t.pointerSmall, t.pointer = "[*]", "[ ]", "√", "...", "»", ">"
	}
	columns, _ := fileTerminalSize(os.Stdout)
	t.colors = columns > 0 && os.Getenv("NO_COLOR") == ""
	return t
}

// The theme of `run`, made once per process as Effect's `Theme` default.
var promptCurrentTheme = sync.OnceValue(promptMakeTheme)

func PromptDefaultTheme() Value {
	t := promptMakeTheme()
	fields := make([]Value, 0, 2*len(promptThemeFields)+2)
	for i, p := range t.strings() {
		fields = append(fields, promptThemeFields[i], *p)
	}
	return Rec(append(fields, "colors", t.colors)...)
}

// Effect's `internal/ansi`.

const (
	ansiReset         = "\x1b[0m"
	ansiBold          = "\x1b[1m"
	ansiItalic        = "\x1b[3m"
	ansiUnderline     = "\x1b[4m"
	ansiStrike        = "\x1b[9m"
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
)

func ansiAnnotate(colors bool, text string, styles ...string) string {
	if !colors {
		return text
	}
	return strings.Join(styles, "") + text + ansiReset
}

func ansiCursorMove(column int) string {
	switch {
	case column > 0:
		return "\x1b[" + strconv.Itoa(column) + "C"
	case column < 0:
		return "\x1b[" + strconv.Itoa(-column) + "D"
	}
	return ""
}

func ansiEraseLines(rows int) string {
	var b strings.Builder
	for i := 0; i < rows; i++ {
		b.WriteString(ansiEraseLine)
		if i < rows-1 {
			b.WriteString("\x1b[1A")
		}
	}
	if rows > 0 {
		b.WriteString(ansiCursorLeft)
	}
	return b.String()
}
