package flickrt

import "syscall"

// statTimes are the access, modification and birth times of a stat.
//
// shortcut: the birth time is the change time, where Node reads statx's when the file system keeps one; call statx when a program reads birth times on linux
func statTimes(st *syscall.Stat_t) (atime, mtime, birth float64) {
	return timespecMillis(st.Atim), timespecMillis(st.Mtim), timespecMillis(st.Ctim)
}
