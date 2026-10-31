package flickrt

import (
	"io"
	"os"
	"strconv"
	"strings"
	"sync"
	"unicode"
	"unicode/utf8"
)

var promptQuit = Nullary(Tag("Terminal.QuitError.QuitError"))

// Control bytes must not become terminal commands when echoed by a prompt.
func promptVisible(text string) string {
	return strings.Map(func(r rune) rune {
		if unicode.IsControl(r) {
			return -1
		}
		return r
	}, wellFormed(text))
}

func promptRuneWidth(r rune) int {
	if unicode.Is(unicode.Mn, r) || unicode.Is(unicode.Me, r) || r == 0x200d || r >= 0xfe00 && r <= 0xfe0f {
		return 0
	}
	if r >= 0x1100 && (r <= 0x115f || r >= 0x2329 && r <= 0x232a ||
		r >= 0x2e80 && r <= 0xa4cf && r != 0x303f || r >= 0xac00 && r <= 0xd7a3 ||
		r >= 0xf900 && r <= 0xfaff || r >= 0xfe10 && r <= 0xfe19 ||
		r >= 0xfe30 && r <= 0xfe6f || r >= 0xff00 && r <= 0xff60 ||
		r >= 0xffe0 && r <= 0xffe6 || r >= 0x1f300 && r <= 0x1faff || r >= 0x20000 && r <= 0x3fffd) {
		return 2
	}
	return 1
}

func promptWidth(text string) int {
	width := 0
	for _, r := range text {
		width += promptRuneWidth(r)
	}
	return width
}

type promptProcessTty struct {
	input, output *os.File
	native        *promptTerminalState
	readGate      chan struct{}
}

var processPromptTty struct {
	sync.Mutex
	tty  *promptProcessTty
	refs int
}

func acquirePromptTty() *promptProcessTty {
	processPromptTty.Lock()
	defer processPromptTty.Unlock()
	if processPromptTty.refs == 0 {
		native, err := promptOpenTerminal(os.Stdin, os.Stdout)
		if err != nil {
			panic(Defect{"Cli.Prompt terminal setup: " + err.Error()})
		}
		tty := &promptProcessTty{os.Stdin, os.Stdout, native, make(chan struct{}, 1)}
		tty.readGate <- struct{}{}
		processPromptTty.tty = tty
	}
	processPromptTty.refs++
	return processPromptTty.tty
}

func releasePromptTty() Value {
	processPromptTty.Lock()
	defer processPromptTty.Unlock()
	processPromptTty.refs--
	if processPromptTty.refs == 0 {
		tty := processPromptTty.tty
		processPromptTty.tty = nil
		if err := promptRestoreTerminal(tty.native); err != nil {
			panic(Defect{"Cli.Prompt terminal restoration: " + err.Error()})
		}
	}
	return Unit
}

func (tty *promptProcessTty) read(fb *Fiber) Value {
	Check(fb)
	idle()
	if fb.state.mask > 0 {
		<-tty.readGate
	} else {
		select {
		case <-tty.readGate:
		case <-fb.state.signal:
			panic(fb.state.stopped())
		}
	}
	defer func() { tty.readGate <- struct{}{} }()
	value, err := promptTerminalRead(fb, tty.native)
	if err == io.EOF {
		return Fail(promptQuit)
	}
	if err != nil {
		panic(Defect{"Cli.Prompt terminal input: " + err.Error()})
	}
	return value
}

// PromptNode is `Cli.Prompt.node`: the terminal of the process.
func PromptNode(key Value) Value {
	return memoized(func(fb *Fiber, _ *MemoMap, scope *Owner) Services {
		tty := acquirePromptTty()
		scope.add(fb, func(*Fiber, *Cause) Value { return releasePromptTty() })
		return Services{key.(string): promptProcessService(tty)}
	})
}

// promptProcessService is the `Tty` of the process terminal; Ctrl+C and
// Ctrl+D quit, as Effect's NodeTerminal has it.
func promptProcessService(tty *promptProcessTty) *Record {
	return Rec(
		"read_key", Fn(func(_ *Fiber, _ Value) Value {
			return Eff(func(fb *Fiber) Value {
				input := tty.read(fb).(*Record)
				key := input.Get("key").(*Record)
				if name := key.Get("name"); key.Get("ctrl").(bool) && (name == "c" || name == "d") {
					return Fail(promptQuit)
				}
				return input
			})
		}),
		"display", Fn(func(_ *Fiber, text Value) Value {
			return Eff(func(*Fiber) Value {
				outMu.Lock()
				defer outMu.Unlock()
				stdout.Flush()
				if _, err := tty.output.WriteString(wellFormed(text.(string))); err != nil {
					panic(Defect{"Cli.Prompt terminal output: " + err.Error()})
				}
				return Unit
			})
		}),
		"columns", Fn(func(_ *Fiber, _ Value) Value {
			return Eff(func(*Fiber) Value { columns, _ := promptTerminalSize(tty.native); return int32(columns) })
		}),
		"rows", Fn(func(_ *Fiber, _ Value) Value {
			return Eff(func(*Fiber) Value { _, rows := promptTerminalSize(tty.native); return int32(rows) })
		}),
	).(*Record)
}

func promptInput(name string, input Value, ctrl, meta, shift bool) Value {
	return Rec("input", input, "key", Rec("name", name, "ctrl", ctrl, "meta", meta, "shift", shift))
}

type promptKeyReader struct {
	read    func(*Fiber, int) (byte, bool, error)
	pending []byte
}

func (reader *promptKeyReader) next(fb *Fiber, timeout int) (byte, bool, error) {
	if len(reader.pending) != 0 {
		b := reader.pending[0]
		reader.pending = reader.pending[1:]
		return b, true, nil
	}
	return reader.read(fb, timeout)
}

func (reader *promptKeyReader) key(fb *Fiber) (Value, error) {
	b, _, err := reader.next(fb, -1)
	if err != nil {
		return nil, err
	}
	switch b {
	case '\r', '\n':
		return promptInput("return", None, false, false, false), nil
	case '\b', 0x7f:
		return promptInput("backspace", None, false, false, false), nil
	case '\t':
		return promptInput("tab", None, false, false, false), nil
	case 0x1b:
		return reader.escape(fb)
	}
	if b < 0x20 {
		name := "space"
		if b > 0 {
			name = string(rune('a' + b - 1))
		}
		return promptInput(name, None, true, false, false), nil
	}
	var encoded [utf8.UTFMax]byte
	encoded[0] = b
	n := 1
	for !utf8.FullRune(encoded[:n]) {
		next, _, err := reader.next(fb, -1)
		if err != nil {
			if err == io.EOF {
				break
			}
			return nil, err
		}
		encoded[n] = next
		n++
	}
	r, size := utf8.DecodeRune(encoded[:n])
	if size < n {
		reader.pending = append(append([]byte{}, encoded[size:n]...), reader.pending...)
	}
	text := string(r)
	return promptInput(text, Some(text), false, false, unicode.IsUpper(r)), nil
}

func (reader *promptKeyReader) escape(fb *Fiber) (Value, error) {
	next, ready, err := reader.next(fb, 40)
	if err != nil && err != io.EOF {
		return nil, err
	}
	if !ready || err == io.EOF {
		return promptInput("escape", None, false, false, false), nil
	}
	if next != '[' && next != 'O' {
		reader.pending = append([]byte{next}, reader.pending...)
		value, err := reader.key(fb)
		if err != nil {
			return nil, err
		}
		input := value.(*Record)
		return input.set("key", input.Get("key").(*Record).set("meta", true)), nil
	}
	var sequence []byte
	for len(sequence) < 32 {
		b, ready, err := reader.next(fb, 40)
		if err != nil && err != io.EOF {
			return nil, err
		}
		if !ready || err == io.EOF {
			break
		}
		sequence = append(sequence, b)
		if b >= 0x40 && b <= 0x7e {
			break
		}
	}
	if len(sequence) == 0 {
		return promptInput("", None, false, false, false), nil
	}
	last := sequence[len(sequence)-1]
	params := strings.Split(string(sequence[:len(sequence)-1]), ";")
	modifier := 0
	if len(params) > 1 {
		if n, err := strconv.Atoi(params[len(params)-1]); err == nil {
			modifier = n - 1
		}
	}
	name := ""
	switch last {
	case 'A':
		name = "up"
	case 'B':
		name = "down"
	case 'C':
		name = "right"
	case 'D':
		name = "left"
	case 'H':
		name = "home"
	case 'F':
		name = "end"
	case 'Z':
		name, modifier = "tab", 1
	case '~':
		switch params[0] {
		case "1", "7":
			name = "home"
		case "4", "8":
			name = "end"
		case "3":
			name = "delete"
		}
	}
	return promptInput(name, None, modifier&4 != 0, modifier&2 != 0, modifier&1 != 0), nil
}
