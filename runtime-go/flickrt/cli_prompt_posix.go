//go:build darwin || linux

package flickrt

import (
	"errors"
	"io"
	"os"
	"syscall"
	"unsafe"
)

type promptTerminalState struct {
	input, output *os.File
	original      *syscall.Termios
	pump          *stdinPump
	keys          promptKeyReader
}

func promptIoctl(fd uintptr, request uintptr, value unsafe.Pointer) error {
	for {
		_, _, err := syscall.Syscall(syscall.SYS_IOCTL, fd, request, uintptr(value))
		if err == syscall.EINTR {
			continue
		}
		if err != 0 {
			return err
		}
		return nil
	}
}

func promptOpenTerminal(input, output *os.File) (*promptTerminalState, error) {
	tty := &promptTerminalState{input: input, output: output, pump: pumpOf(input)}
	var original syscall.Termios
	// any error means no terminal (ENOTTY, or ENODEV for /dev/null on macOS): no raw mode, as Node checks isTTY
	if err := promptIoctl(input.Fd(), promptGetTermios, unsafe.Pointer(&original)); err == nil {
		raw := original
		raw.Iflag &^= syscall.IGNBRK | syscall.BRKINT | syscall.PARMRK | syscall.ISTRIP | syscall.INLCR | syscall.IGNCR | syscall.ICRNL | syscall.IXON
		raw.Oflag &^= syscall.OPOST
		raw.Lflag &^= syscall.ECHO | syscall.ECHONL | syscall.ICANON | syscall.ISIG | syscall.IEXTEN
		raw.Cflag &^= syscall.CSIZE | syscall.PARENB
		raw.Cflag |= syscall.CS8
		raw.Cc[syscall.VMIN], raw.Cc[syscall.VTIME] = 1, 0
		if err := promptIoctl(input.Fd(), promptSetTermios, unsafe.Pointer(&raw)); err != nil {
			return nil, err
		}
		tty.original = &original
	}
	tty.keys.read = tty.readByte
	return tty, nil
}

func promptRestoreTerminal(tty *promptTerminalState) error {
	if tty.original == nil {
		return nil
	}
	return promptIoctl(tty.input.Fd(), promptSetTermios, unsafe.Pointer(tty.original))
}

// readByte reads through the pump, so fd 0 has one reader; the raw mode
// set on the fd holds for it.
func (tty *promptTerminalState) readByte(fb *Fiber, timeout int) (byte, bool, error) {
	b, ok, err := tty.pump.byte(fb, timeout)
	if errors.Is(err, syscall.EIO) {
		err = io.EOF // the terminal hung up
	}
	return b, ok, err
}

func promptTerminalRead(fb *Fiber, tty *promptTerminalState) (Value, error) {
	return tty.keys.key(fb)
}

func promptTerminalSize(tty *promptTerminalState) (int, int) {
	var size struct{ rows, columns, x, y uint16 }
	if promptIoctl(tty.output.Fd(), syscall.TIOCGWINSZ, unsafe.Pointer(&size)) != nil {
		return 0, 0
	}
	return int(size.columns), int(size.rows)
}

func fileIsTerminal(f *os.File) bool {
	var termios syscall.Termios
	return promptIoctl(f.Fd(), promptGetTermios, unsafe.Pointer(&termios)) == nil
}

func fileTerminalSize(f *os.File) (int, int) {
	return promptTerminalSize(&promptTerminalState{output: f})
}
