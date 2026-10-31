package flickrt

import (
	"io/fs"
	"math/big"
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

// fsStatSys is what Node's stat gives beside the type, mode, size and
// times: on Windows the creation time.
// shortcut: no device, inode, links or blocks, which Go's FileInfo lacks on Windows; read them by GetFileInformationByHandle when a program needs them
func fsStatSys(info fs.FileInfo) (dev *big.Int, ino, nlink, uid, gid, rdev, blksize, blocks, birth Value) {
	birth = None
	if d, ok := info.Sys().(*syscall.Win32FileAttributeData); ok {
		ticks := uint64(d.CreationTime.HighDateTime)<<32 | uint64(d.CreationTime.LowDateTime)
		birth = Some(float64(time.Unix(int64(ticks/10000000)-11644473600, int64(ticks%10000000)*100).UnixMilli()))
	}
	return big.NewInt(0), None, None, None, None, None, None, None, birth
}
