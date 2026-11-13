//go:build !linux && !darwin && !windows

package flickrt

// shortcut: no terminal is seen on systems other than linux, darwin and windows; read it with their ioctl requests when Flick programs run there
func isTTY(uintptr) bool { return false }

func windowSize(uintptr) (columns, rows int, ok bool) { return 0, 0, false }
