package flickrt

import (
	"io/fs"
	"os"
	"syscall"
	"time"
)

// Node rejects noFollow on Windows, where libuv ignores O_NOFOLLOW.
const fsNoFollowFlag = 0

func fsAccess(path string, _, writable bool) error {
	info, err := os.Stat(path)
	if err == nil && writable && !info.IsDir() && info.Mode().Perm()&0o222 == 0 {
		return &os.PathError{Op: "access", Path: path, Err: syscall.EPERM}
	}
	return err
}

func fsAccessTime(info fs.FileInfo) time.Time {
	t := info.Sys().(*syscall.Win32FileAttributeData).LastAccessTime
	ticks := uint64(t.HighDateTime)<<32 | uint64(t.LowDateTime)
	return time.Unix(int64(ticks/10000000)-11644473600, int64(ticks%10000000)*100)
}
