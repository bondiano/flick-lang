package flickrt

import (
	"io"
	"reflect"
	"strings"
	"testing"
)

const promptTestKey = "test/Prompt/Tty"

type promptScript struct {
	keys   []Value
	frames []string
	check  func(*Fiber)
}

func (script *promptScript) service() Value {
	operation := func(f func(*Fiber, Value) Value) Value {
		return Fn(func(fb *Fiber, argument Value) Value {
			if script.check != nil {
				script.check(fb)
			}
			return Eff(func(fb *Fiber) Value {
				if script.check != nil {
					script.check(fb)
				}
				return f(fb, argument)
			})
		})
	}
	return Rec(
		"read_key", operation(func(*Fiber, Value) Value {
			if len(script.keys) == 0 {
				return Fail(promptQuit)
			}
			key := script.keys[0]
			script.keys = script.keys[1:]
			return key
		}),
		"display", operation(func(_ *Fiber, text Value) Value {
			script.frames = append(script.frames, text.(string))
			return Unit
		}),
		"columns", operation(func(*Fiber, Value) Value { return int32(80) }),
		"rows", operation(func(*Fiber, Value) Value { return int32(24) }),
	)
}

func promptTestPress(name string) Value { return promptInput(name, None, false, false, false) }

func promptTestTyped(text string) []Value {
	var keys []Value
	for _, r := range text {
		s := string(r)
		keys = append(keys, promptInput(s, Some(s), false, false, false))
	}
	return keys
}

func promptTestText(initial, validate Value) Value {
	return PromptText(Rec("message", "Answer?", "default", initial, "validate", validate))
}

func promptTestRun(script *promptScript, prompt Value) Value {
	return Provide(rootFiber(), LayerSucceed(promptTestKey, script.service()), Eff(func(fb *Fiber) Value {
		return PromptRun(fb, promptTestKey, prompt)
	}))
}

func TestPromptTextValidationRetainsInvalidInput(t *testing.T) {
	cases := []struct {
		name      string
		first     string
		edits     []Value
		answer    string
		validated []string
	}{
		{"wizard", "  ", append([]Value{promptTestPress("backspace"), promptTestPress("backspace")}, promptTestTyped("tea")...), "tea", []string{"  ", "tea"}},
		{"retained_text", "bad", append([]Value{promptTestPress("backspace")}, promptTestTyped("t")...), "bat", []string{"bad", "bat"}},
	}
	for _, test := range cases {
		t.Run(test.name, func(t *testing.T) {
			var validated []string
			validate := Fn(func(_ *Fiber, value Value) Value {
				text := value.(string)
				validated = append(validated, text)
				if text == test.first {
					return Err("please try again")
				}
				return Ok(text)
			})
			keys := append(promptTestTyped(test.first), promptTestPress("return"))
			keys = append(keys, test.edits...)
			keys = append(keys, promptTestPress("return"), promptTestPress("unused"))
			script := &promptScript{keys: keys}
			if got := promptTestRun(script, promptTestText(None, Some(validate))); got != test.answer {
				t.Fatalf("got %v, want %s", got, test.answer)
			}
			if !reflect.DeepEqual(validated, test.validated) {
				t.Fatalf("validated %q, want %q", validated, test.validated)
			}
			if !strings.Contains(strings.Join(script.frames, ""), "please try again") {
				t.Error("validation error was not displayed")
			}
			if len(script.keys) != 1 {
				t.Fatalf("read ahead beyond submit: %d keys left", len(script.keys))
			}
			if got := script.frames[len(script.frames)-1]; got != "\x1b[?25h" {
				t.Fatalf("cursor was not restored: %q", got)
			}
		})
	}
}

func TestPromptTextEditingUnicodeAndDefaults(t *testing.T) {
	cases := []struct {
		name    string
		keys    []Value
		initial Value
		answer  string
	}{
		{"default", nil, Some("Paris"), "Paris"},
		{"typed_after_default", promptTestTyped("Lima"), Some("Paris"), "ParisLima"},
		{"unicode_backspace", append(promptTestTyped("é🙂"), promptTestPress("backspace")), None, "é"},
		{"cursor_editing", append(promptTestTyped("a界c"), promptTestPress("left"), promptTestPress("delete"), promptTestPress("left"), promptInput("🙂", Some("🙂"), false, false, false), promptTestPress("end"), promptInput("!", Some("!"), false, false, false), promptTestPress("home"), promptTestPress("right"), promptTestPress("backspace")), None, "🙂界c!"},
	}
	for _, test := range cases {
		t.Run(test.name, func(t *testing.T) {
			keys := append(test.keys, promptTestPress("return"))
			if got := promptTestRun(&promptScript{keys: keys}, promptTestText(test.initial, None)); got != test.answer {
				t.Fatalf("got %q, want %q", got, test.answer)
			}
		})
	}
}

func TestPromptTextCallbacksUseExecutionFiber(t *testing.T) {
	const contextKey = "test/Prompt/context"
	calls := 0
	check := func(fb *Fiber) {
		t.Helper()
		if got, ok := fb.env.get(contextKey); !ok || got != "executing" {
			t.Fatalf("callback used construction context: %v", got)
		}
		calls++
	}
	validate := Fn(func(fb *Fiber, text Value) Value {
		check(fb)
		return Ok(strings.TrimSpace(text.(string)))
	})
	construction := provided(rootFiber(), Services{contextKey: "constructing"})
	makePrompt := Fn(func(*Fiber, Value) Value { return promptTestText(None, Some(validate)) })
	prompt := Apply(construction, makePrompt, Unit)
	if calls != 0 {
		t.Fatal("constructing a prompt executed callbacks")
	}
	script := &promptScript{keys: append(promptTestTyped(" tea "), promptTestPress("return")), check: check}
	layers := LayerMerge(LayerSucceed(contextKey, "executing"), LayerSucceed(promptTestKey, script.service()))
	got := Provide(rootFiber(), layers, Eff(func(fb *Fiber) Value { return PromptRun(fb, promptTestKey, prompt) }))
	if got != "tea" || calls == 0 {
		t.Fatalf("execution returned %v after %d callback calls", got, calls)
	}
}

func TestPromptTextCancellation(t *testing.T) {
	cases := []struct {
		name string
		key  Value
	}{
		{"eof", nil},
		// as Effect's, the prompt beeps at these; the terminal decides to quit
		{"escape", promptTestPress("escape")},
		{"ctrl_c", promptInput("c", None, true, false, false)},
	}
	for _, test := range cases {
		t.Run(test.name, func(t *testing.T) {
			script := &promptScript{}
			if test.key != nil {
				script.keys = []Value{test.key}
			}
			_, cause := catch(func() Value { return promptTestRun(script, promptTestText(None, None)) })
			if cause == nil || cause.kind != causeFail || cause.err != promptQuit {
				t.Fatalf("not QuitError: %v", cause)
			}
			if got := script.frames[len(script.frames)-1]; got != "\x1b[?25h" {
				t.Fatalf("cancellation did not restore cursor: %q", got)
			}
		})
	}
}

func TestPromptTextRestoresCursorAfterInterruption(t *testing.T) {
	script := &promptScript{}
	tty := script.service().(*Record).set("read_key", Fn(func(*Fiber, Value) Value {
		return Eff(func(fb *Fiber) Value {
			fb.state.interrupt(37)
			Check(fb)
			return Unit
		})
	}))
	_, cause := catch(func() Value {
		return Provide(rootFiber(), LayerSucceed(promptTestKey, tty), Eff(func(fb *Fiber) Value {
			return PromptRun(fb, promptTestKey, promptTestText(None, None))
		}))
	})
	if cause == nil || cause.kind != causeInterrupt || cause.fiber != 37 {
		t.Fatalf("lost interruption: %v", cause)
	}
	if got := script.frames[len(script.frames)-1]; got != "\x1b[?25h" {
		t.Fatalf("interruption did not restore cursor: %q", got)
	}
}

func TestPromptTextPreservesBodyAndCleanupFailures(t *testing.T) {
	body := &Cause{kind: causeDie, defect: "read failed"}
	cleanup := &Cause{kind: causeDie, defect: "display failed"}
	tty := (&promptScript{}).service().(*Record)
	tty = tty.set("read_key", Fn(func(*Fiber, Value) Value {
		return Eff(func(*Fiber) Value { panic(&Failure{body}) })
	})).set("display", Fn(func(_ *Fiber, text Value) Value {
		return Eff(func(*Fiber) Value {
			if text == "\x1b[?25h" {
				panic(&Failure{cleanup})
			}
			return Unit
		})
	}))
	_, cause := catch(func() Value {
		return Provide(rootFiber(), LayerSucceed(promptTestKey, tty), Eff(func(fb *Fiber) Value {
			return PromptRun(fb, promptTestKey, promptTestText(None, None))
		}))
	})
	if got := cause.reasons(); len(got) != 2 || got[0] != body || got[1] != cleanup {
		t.Fatalf("lost body or cleanup error: %v", got)
	}
}

func TestPromptNativeKeyDecoding(t *testing.T) {
	cases := []struct {
		encoded string
		name    string
		input   string
		ctrl    bool
		meta    bool
		shift   bool
	}{
		{"a", "a", "a", false, false, false},
		{"é", "é", "é", false, false, false},
		{"🙂", "🙂", "🙂", false, false, false},
		{"\r", "return", "", false, false, false},
		{"\n", "return", "", false, false, false},
		{"\x7f", "backspace", "", false, false, false},
		{"\x03", "c", "", true, false, false},
		{"\x04", "d", "", true, false, false},
		{"\x1b", "escape", "", false, false, false},
		{"\x1b[D", "left", "", false, false, false},
		{"\x1bOC", "right", "", false, false, false},
		{"\x1b[3~", "delete", "", false, false, false},
		{"\x1b[H", "home", "", false, false, false},
		{"\x1b[4~", "end", "", false, false, false},
		{"\x1b[1;5D", "left", "", true, false, false},
		{"\x1bx", "x", "x", false, true, false},
	}
	for _, test := range cases {
		t.Run(test.encoded, func(t *testing.T) {
			remaining := []byte(test.encoded)
			reader := promptKeyReader{read: func(*Fiber, int) (byte, bool, error) {
				if len(remaining) == 0 {
					return 0, false, io.EOF
				}
				b := remaining[0]
				remaining = remaining[1:]
				return b, true, nil
			}}
			value, err := reader.key(rootFiber())
			if err != nil {
				t.Fatal(err)
			}
			input := value.(*Record)
			key := input.Get("key").(*Record)
			if key.Get("name") != test.name || key.Get("ctrl") != test.ctrl || key.Get("meta") != test.meta || key.Get("shift") != test.shift {
				t.Fatalf("decoded key %v, want %+v", key, test)
			}
			text, exists := some(input.Get("input"))
			if exists != (test.input != "") || exists && text != test.input {
				t.Fatalf("decoded input %v, want %q", text, test.input)
			}
		})
	}
}

func TestPromptTextCursorAndEcho(t *testing.T) {
	theme := promptMakeTheme()
	theme.colors = false
	c := &promptCtx{theme: theme}
	opts := &promptTextOptions{kind: "text", message: "Q"}
	frame := opts.render(c, nil, promptNext(promptTextState{1, []rune("界🙂x"), nil}))
	if want := "? Q " + theme.pointerSmall + " 界🙂x\x1b[3D"; frame != want {
		t.Fatalf("incorrect Unicode cursor position: %q, want %q", frame, want)
	}
	if got := promptVisible("hello\x1b[31m\nworld"); strings.ContainsAny(got, "\x1b\n") {
		t.Fatalf("echo contains terminal control bytes: %q", got)
	}
}
