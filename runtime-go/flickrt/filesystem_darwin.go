package flickrt

import (
	"io/fs"
	"syscall"
	"time"
)

func fsAccessTime(info fs.FileInfo) time.Time {
	t := info.Sys().(*syscall.Stat_t).Atimespec
	return time.Unix(t.Sec, t.Nsec)
}
