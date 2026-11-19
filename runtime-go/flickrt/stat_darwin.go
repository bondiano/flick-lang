package flickrt

import "syscall"

// statTimes are the access, modification and birth times of a stat.
func statTimes(st *syscall.Stat_t) (atime, mtime, birth float64) {
	return timespecMillis(st.Atimespec), timespecMillis(st.Mtimespec), timespecMillis(st.Birthtimespec)
}
