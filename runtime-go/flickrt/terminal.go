package flickrt

import (
	"bufio"
	"os"
	"strings"
	"sync"
)

// Effect's `Terminal` and `Stdio` of Node.js as the Flick services
// `Terminal.Terminal` and `Stdio.Stdio`: the size of the terminal stdout
// is (0×0 when it is not one), lines of stdin until it ends, which quits.
//
// shortcut: lines are read in the terminal's own line mode, where Effect switches a terminal to raw mode and reads with Node's readline; read raw when a program needs the keys the line mode keeps

var terminalQuitTag = Tag("Terminal.QuitError.QuitError")

// stdinLines reads stdin a line at a time as readers ask, so a line read
// for a reader interrupted waiting stays for the next one, as in Effect's
// queue of lines, and nothing more of stdin is read.
var stdinLines struct {
	mu     sync.Mutex
	reader *bufio.Reader
	asked  bool
	ended  bool
	lines  chan stdinLine
}

type stdinLine struct {
	text  string
	ended bool
}

// readStdinLine is the next line of stdin, or false once stdin ended.
func readStdinLine(fb *Fiber) (string, bool) {
	s := &stdinLines
	s.mu.Lock()
	if s.ended {
		s.mu.Unlock()
		return "", false
	}
	if s.lines == nil {
		s.lines = make(chan stdinLine, 1)
		s.reader = bufio.NewReader(os.Stdin)
	}
	if !s.asked {
		s.asked = true
		go func() {
			line, err := s.reader.ReadString('\n')
			// readline ends a line at `\n` or `\r\n`, and gives the text
			// before the end of the input as a last line
			line = strings.TrimSuffix(strings.TrimSuffix(line, "\n"), "\r")
			s.lines <- stdinLine{text: line, ended: err != nil && line == ""}
			if err != nil {
				s.mu.Lock()
				s.ended = true
				s.mu.Unlock()
			}
		}()
	}
	s.mu.Unlock()
	park(fb)
	defer unpark(fb)
	idle()
	signal := fb.state.signal
	if fb.state.mask > 0 {
		signal = nil
	}
	select {
	case l := <-s.lines:
		s.mu.Lock()
		s.asked = false
		s.mu.Unlock()
		return l.text, !l.ended
	case <-signal:
		panic(fb.state.stopped())
	}
}

// terminalSize is the columns and rows of stdout's terminal, 0 for none.
func terminalSize() (int32, int32) {
	columns, rows, ok := windowSize(os.Stdout.Fd())
	if !ok {
		return 0, 0
	}
	return int32(columns), int32(rows)
}

// TerminalNode is the layer of the terminal of the process; the keys that
// quit matter to the prompts, which read key presses.
func TerminalNode(key, _ Value) Value {
	return LayerSucceed(key, Rec(
		"columns", Fn(func(Value) Value {
			return Eff(func(*Fiber) Value {
				c, _ := terminalSize()
				return c
			})
		}),
		"rows", Fn(func(Value) Value {
			return Eff(func(*Fiber) Value {
				_, r := terminalSize()
				return r
			})
		}),
		"read_line", Fn(func(Value) Value {
			return Eff(func(fb *Fiber) Value {
				line, ok := readStdinLine(fb)
				if !ok {
					return Fail(Nullary(terminalQuitTag))
				}
				return line
			})
		}),
		"display", Fn(func(text Value) Value {
			return Eff(func(*Fiber) Value {
				StdoutWrite(text)
				StdoutFlush()
				return Unit
			})
		}),
	))
}

// StdioNode is the layer of the arguments and standard streams of the
// process.
func StdioNode(key Value) Value {
	terminal := func(f *os.File) Value {
		return Fn(func(Value) Value { return Eff(func(*Fiber) Value { return isTTY(f.Fd()) }) })
	}
	return LayerSucceed(key, Rec(
		"args", Fn(func(Value) Value { return Eff(func(*Fiber) Value { return PlatformArgs() }) }),
		"stdin_is_terminal", terminal(os.Stdin),
		"stdout_is_terminal", terminal(os.Stdout),
		"stderr_is_terminal", terminal(os.Stderr),
		"write_stdout", Fn(func(text Value) Value { return Eff(func(*Fiber) Value { return StdoutWrite(text) }) }),
		"write_stderr", Fn(func(text Value) Value { return Eff(func(*Fiber) Value { return StderrWrite(text) }) }),
	))
}
