package flickrt

import "syscall"

const (
	promptGetTermios = syscall.TCGETS
	promptSetTermios = syscall.TCSETS
)

func promptSelect(n int, set *syscall.FdSet, timeout *syscall.Timeval) error {
	_, err := syscall.Select(n, set, nil, nil, timeout)
	return err
}
