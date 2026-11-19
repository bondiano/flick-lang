package flickrt

import (
	"syscall"
	"unsafe"
)

// The terminal of a handle through the console API of kernel32, as
// Node's `tty` module reads it on windows.

var (
	kernel32                       = syscall.NewLazyDLL("kernel32.dll")
	procGetConsoleMode             = kernel32.NewProc("GetConsoleMode")
	procSetConsoleMode             = kernel32.NewProc("SetConsoleMode")
	procGetConsoleScreenBufferInfo = kernel32.NewProc("GetConsoleScreenBufferInfo")
)

// consoleMode is the mode of a console handle; not ok for another handle.
func consoleMode(fd uintptr) (mode uint32, ok bool) {
	r, _, _ := procGetConsoleMode.Call(fd, uintptr(unsafe.Pointer(&mode)))
	return mode, r != 0
}

// isTTY tells whether the handle is a console.
func isTTY(fd uintptr) bool {
	_, ok := consoleMode(fd)
	return ok
}

// windowSize is the columns and rows of the window of a console handle.
func windowSize(fd uintptr) (columns, rows int, ok bool) {
	var info struct {
		size, cursor                    [2]int16
		attributes                      uint16
		left, top, right, bottom        int16
		maxWindowWidth, maxWindowHeight int16
	}
	if r, _, _ := procGetConsoleScreenBufferInfo.Call(fd, uintptr(unsafe.Pointer(&info))); r == 0 {
		return 0, 0, false
	}
	return int(info.right-info.left) + 1, int(info.bottom-info.top) + 1, true
}
