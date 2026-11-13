package flickrt

import "os"

// The console modes of raw input, as libuv's raw mode, with the keys that
// move the cursor read as the escape sequences the decoder knows, and of
// output that draws escape sequences.
const (
	enableProcessedInput            = 0x0001
	enableLineInput                 = 0x0002
	enableEchoInput                 = 0x0004
	enableVirtualTerminalInput      = 0x0200
	enableVirtualTerminalProcessing = 0x0004
)

// ttyMakeRaw puts the console of stdin in raw mode and lets the console of
// stdout draw escape sequences; it gives what restores both, nil when
// stdin is not a console.
func ttyMakeRaw() func() {
	in, out := os.Stdin.Fd(), os.Stdout.Fd()
	mode, ok := consoleMode(in)
	if !ok {
		return nil
	}
	raw := mode&^(enableProcessedInput|enableLineInput|enableEchoInput) | enableVirtualTerminalInput
	if r, _, _ := procSetConsoleMode.Call(in, uintptr(raw)); r == 0 {
		return nil
	}
	outMode, outOk := consoleMode(out)
	if outOk && outMode&enableVirtualTerminalProcessing == 0 {
		procSetConsoleMode.Call(out, uintptr(outMode|enableVirtualTerminalProcessing))
	}
	return func() {
		procSetConsoleMode.Call(in, uintptr(mode))
		if outOk {
			procSetConsoleMode.Call(out, uintptr(outMode))
		}
	}
}
