//go:build darwin || linux

package flickrt

import (
	"io"
	"os"
	"syscall"
	"time"
	"unsafe"
)

type promptTerminalState struct {
	input, output *os.File
	original      *syscall.Termios
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
	tty := &promptTerminalState{input: input, output: output}
	var original syscall.Termios
	if err := promptIoctl(input.Fd(), promptGetTermios, unsafe.Pointer(&original)); err != nil {
		if err != syscall.ENOTTY {
			return nil, err
		}
	} else {
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

func (tty *promptTerminalState) readByte(fb *Fiber, timeout int) (byte, bool, error) {
	var deadline time.Time
	if timeout >= 0 {
		deadline = time.Now().Add(time.Duration(timeout) * time.Millisecond)
	}
	fd := int(tty.input.Fd())
	for {
		Check(fb)
		delay := 30 * time.Millisecond
		if !deadline.IsZero() {
			remaining := time.Until(deadline)
			if remaining <= 0 {
				return 0, false, nil
			}
			delay = min(delay, remaining)
		}
		var set syscall.FdSet
		bits := int(unsafe.Sizeof(set.Bits[0])) * 8
		if fd < 0 || fd >= len(set.Bits)*bits {
			return 0, false, syscall.EINVAL
		}
		set.Bits[fd/bits] |= 1 << uint(fd%bits)
		tv := syscall.NsecToTimeval(int64(delay))
		if err := promptSelect(fd+1, &set, &tv); err != nil {
			if err == syscall.EINTR {
				continue
			}
			return 0, false, err
		}
		if set.Bits[fd/bits]&(1<<uint(fd%bits)) == 0 {
			continue
		}
		Check(fb)
		var buffer [1]byte
		n, err := syscall.Read(fd, buffer[:])
		if err == syscall.EINTR || err == syscall.EAGAIN {
			continue
		}
		if err == syscall.EIO || n == 0 && err == nil {
			return 0, false, io.EOF
		}
		if err != nil {
			return 0, false, err
		}
		return buffer[0], true, nil
	}
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
