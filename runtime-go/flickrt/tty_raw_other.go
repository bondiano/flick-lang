//go:build !linux && !darwin && !windows

package flickrt

// shortcut: stdin stays in line mode on systems other than linux, darwin and windows; set their termios when Flick programs prompt there
func ttyMakeRaw() func() { return nil }
