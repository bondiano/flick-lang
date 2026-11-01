package flickrt

import "syscall"

const (
	promptGetTermios = syscall.TCGETS
	promptSetTermios = syscall.TCSETS
)
