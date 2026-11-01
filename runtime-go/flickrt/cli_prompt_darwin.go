package flickrt

import "syscall"

const (
	promptGetTermios = syscall.TIOCGETA
	promptSetTermios = syscall.TIOCSETA
)
