//go:build linux || darwin

package flickrt

import (
	"syscall"
	"unsafe"
)

// The terminal of a file descriptor through the system's ioctl calls, as
// Node's `tty` module reads it.

func ioctl(fd, request uintptr, arg unsafe.Pointer) error {
	if _, _, errno := syscall.Syscall(syscall.SYS_IOCTL, fd, request, uintptr(arg)); errno != 0 {
		return errno
	}
	return nil
}

// isTTY tells whether the descriptor is a terminal, as `isatty`.
func isTTY(fd uintptr) bool {
	var t syscall.Termios
	return ioctl(fd, ioctlGetTermios, unsafe.Pointer(&t)) == nil
}

// windowSize is the columns and rows of the terminal of the descriptor.
func windowSize(fd uintptr) (columns, rows int, ok bool) {
	var ws struct{ rows, columns, x, y uint16 }
	if ioctl(fd, syscall.TIOCGWINSZ, unsafe.Pointer(&ws)) != nil {
		return 0, 0, false
	}
	return int(ws.columns), int(ws.rows), true
}
