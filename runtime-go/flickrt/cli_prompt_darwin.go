package flickrt

import "syscall"

const (
	promptGetTermios = syscall.TIOCGETA
	promptSetTermios = syscall.TIOCSETA
)

func promptSelect(n int, set *syscall.FdSet, timeout *syscall.Timeval) error {
	return syscall.Select(n, set, nil, nil, timeout)
}
