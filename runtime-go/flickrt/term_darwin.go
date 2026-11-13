package flickrt

import "syscall"

// The ioctl requests of a terminal's attributes on darwin.
const (
	ioctlGetTermios = syscall.TIOCGETA
	ioctlSetTermios = syscall.TIOCSETA
)
