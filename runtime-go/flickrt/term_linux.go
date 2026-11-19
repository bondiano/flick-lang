package flickrt

import "syscall"

// The ioctl requests of a terminal's attributes on linux.
const (
	ioctlGetTermios = syscall.TCGETS
	ioctlSetTermios = syscall.TCSETS
)
