package flickrt

import (
	"reflect"
	"testing"
)

// scriptTty plays key presses and keeps the frames, as the prompt tests'
// scripted `Tty` does.
type scriptTty struct {
	keys   []string
	frames []string
}

func (s *scriptTty) readKey(*Fiber) Value {
	if len(s.keys) == 0 {
		return Fail(Nullary(quitErrorTag))
	}
	name := s.keys[0]
	s.keys = s.keys[1:]
	input := None
	if len(name) == 1 {
		input = Some(name)
	}
	return Rec("input", input, "key", Rec("name", name, "ctrl", false, "meta", false, "shift", false))
}

func (s *scriptTty) display(_ *Fiber, text string) { s.frames = append(s.frames, text) }
func (s *scriptTty) columns(*Fiber) int            { return 80 }

func colorTheme() Value { return record(PromptDefaultTheme()).set("colors", true) }

// The frames are those Effect's prompts draw on Node for the same keys.
func TestPromptFramesAsEffect(t *testing.T) {
	choice := func(title, value, description string, disabled bool) Value {
		d := None
		if description != "" {
			d = Some(description)
		}
		return Rec("title", title, "value", value, "description", d, "disabled", disabled, "selected", false)
	}
	cases := []struct {
		prompt Value
		keys   []string
		value  Value
		frames []string
	}{
		{
			PromptConfirm(Rec("message", "Sure", "initial", None, "label", None, "placeholder", None)),
			[]string{"x", "return"}, false,
			[]string{
				"\x1b[?25l\x1b[96m?\x1b[0m \x1b[1mSure\x1b[0m \x1b[90m›\x1b[0m \x1b[90m(y/N)\x1b[0m",
				"\x07",
				"\x1b[2K\x1b[G\x1b[2K\x1b[G\x1b[32m✔\x1b[0m \x1b[1mSure\x1b[0m \x1b[90m…\x1b[0m no\n",
				"\x1b[?25h",
			},
		},
		{
			PromptSelect(Rec("message", None, "choices", []Value{choice("red", "r", "warm", false),
				choice("green", "g", "", true), choice("blue", "b", "", false)}, "maxPerPage", None)),
			[]string{"return"}, "r",
			[]string{
				"\x1b[?25l\x1b[96m❯\x1b[0m \x1b[4m\x1b[96mred\x1b[0m \x1b[90m- warm\x1b[0m\n  \x1b[9m\x1b[90mgreen\x1b[0m \n  blue ",
				"\x1b[2K\x1b[1A\x1b[2K\x1b[1A\x1b[2K\x1b[G\x1b[2K\x1b[G\x1b[32m✔\x1b[0m \x1b[37mred\x1b[0m\n",
				"\x1b[?25h",
			},
		},
	}
	for _, c := range cases {
		tty := &scriptTty{keys: c.keys}
		fb := rootFiber()
		got := withEnv(fb, promptThemeKey, colorTheme(), Eff(func(fb *Fiber) Value { return runPrompt(fb, tty, c.prompt) }))
		if !Equals(got, c.value) || !reflect.DeepEqual(tty.frames, c.frames) {
			t.Errorf("got %v with frames\n%q\nwant %v with\n%q", got, tty.frames, c.value, c.frames)
		}
	}
}

func TestPromptDateMask(t *testing.T) {
	d := &promptDate{t: &DateTime{millis: 0, zone: zoneNamed("UTC")}}
	parts := makeDateParts(`\Y "Do" YYYY-MM`, d, nil)
	var tokens []string
	for _, p := range parts {
		tokens = append(tokens, string(rune('0'+min(p.kind, 1)))+p.token)
	}
	want := []string{`0Y Do `, "1YYYY", "0-", "1MM"}
	if !reflect.DeepEqual(tokens, want) {
		t.Fatalf("parts %q, want %q", tokens, want)
	}
	if firstField(parts) != 1 || parts[1].String() != "1970" || parts[3].String() != "01" {
		t.Fatal("fields of the epoch")
	}
}

func TestJsParseNumbers(t *testing.T) {
	for text, want := range map[string]float64{"42x": 42, " -7": -7, "0x1A": 26, "1.5e3z": 1500, ".5": 0.5} {
		parse := jsParseInt
		if text == "1.5e3z" || text == ".5" {
			parse = jsParseFloat
		}
		if got := parse(text); got != want {
			t.Errorf("%q: %v, want %v", text, got, want)
		}
	}
	if got := jsParseInt("x"); got == got {
		t.Errorf("parseInt(x) = %v, want NaN", got)
	}
}
