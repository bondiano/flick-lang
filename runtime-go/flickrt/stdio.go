package flickrt

// `Stdio.node` and `Terminal.node`: the standard streams and the terminal
// of the process, as on Node.js.

import "os"

// stderrWrite writes text to stderr after what stdout holds so far.
func stderrWrite(text string) {
	outMu.Lock()
	stdout.Flush()
	os.Stderr.WriteString(wellFormed(text))
	outMu.Unlock()
}

// StdioNode is `Stdio.node`.
func StdioNode(key Value) Value {
	impl := Rec(
		"args", Fn(func(*Fiber, Value) Value { return Succeed(PlatformArgs()) }),
		"stdin_is_terminal", Fn(func(*Fiber, Value) Value { return Succeed(fileIsTerminal(os.Stdin)) }),
		"stdout_is_terminal", Fn(func(*Fiber, Value) Value { return Succeed(fileIsTerminal(os.Stdout)) }),
		"stderr_is_terminal", Fn(func(*Fiber, Value) Value { return Succeed(fileIsTerminal(os.Stderr)) }),
		"write_stdout", Fn(func(_ *Fiber, text Value) Value {
			return Eff(func(*Fiber) Value { return StdoutWrite(text) })
		}),
		"write_stderr", Fn(func(_ *Fiber, text Value) Value {
			return Eff(func(*Fiber) Value { stderrWrite(text.(string)); return Unit })
		}),
	)
	return LayerSucceed(key, markNode(impl))
}

// terminalReadLine is the next line of stdin without its line break;
// false at its end.
func terminalReadLine(fb *Fiber) (string, bool) {
	idle()
	return processStdin().line(fb)
}

// TerminalNode is `Terminal.node` and `node_quitting`; `shouldQuit` picks
// the key presses that quit, which only raw key input reads.
func TerminalNode(key, shouldQuit Value) Value {
	return LayerSucceed(key, Rec(
		"columns", Fn(func(*Fiber, Value) Value { c, _ := fileTerminalSize(os.Stdout); return Succeed(int32(c)) }),
		"rows", Fn(func(*Fiber, Value) Value { _, r := fileTerminalSize(os.Stdout); return Succeed(int32(r)) }),
		"read_line", Fn(func(*Fiber, Value) Value {
			return Eff(func(fb *Fiber) Value {
				line, ok := terminalReadLine(fb)
				if !ok {
					return Fail(promptQuit)
				}
				return line
			})
		}),
		"display", Fn(func(_ *Fiber, text Value) Value {
			return Eff(func(*Fiber) Value { StdoutWrite(text); idle(); return Unit })
		}),
	))
}
