//go:build linux || darwin

package flickrt

import (
	"os"
	"syscall"
	"unsafe"
)

// ttyMakeRaw puts stdin, when it is a terminal, in the raw mode of libuv,
// as Node's `setRawMode(true)`: key presses arrive as typed, without echo
// or signals, and output still turns "\n" into a new line. It gives what
// restores the mode, nil when there is none to restore.
func ttyMakeRaw() func() {
	fd := os.Stdin.Fd()
	var saved syscall.Termios
	if ioctl(fd, ioctlGetTermios, unsafe.Pointer(&saved)) != nil {
		return nil
	}
	raw := saved
	raw.Iflag &^= syscall.BRKINT | syscall.ICRNL | syscall.INPCK | syscall.ISTRIP | syscall.IXON
	raw.Oflag |= syscall.ONLCR
	raw.Cflag |= syscall.CS8
	raw.Lflag &^= syscall.ECHO | syscall.ICANON | syscall.IEXTEN | syscall.ISIG
	raw.Cc[syscall.VMIN] = 1
	raw.Cc[syscall.VTIME] = 0
	if ioctl(fd, ioctlSetTermios, unsafe.Pointer(&raw)) != nil {
		return nil
	}
	return func() { ioctl(fd, ioctlSetTermios, unsafe.Pointer(&saved)) }
}
