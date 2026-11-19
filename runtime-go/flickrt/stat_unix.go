//go:build linux || darwin

package flickrt

import (
	"io/fs"
	"math/big"
	"os"
	"syscall"
)

// statFields are the fields of Node's `stat` that Go's FileInfo does not
// give, from the system's `stat` structure.
func statFields(fi fs.FileInfo, _ *os.File, _ string) (sf statInfo, ok bool) {
	st, isStat := fi.Sys().(*syscall.Stat_t)
	if !isStat {
		return sf, false
	}
	atime, mtime, birth := statTimes(st)
	return statInfo{
		mode: uint32(st.Mode), dev: new(big.Int).SetUint64(uint64(st.Dev)), ino: new(big.Int).SetUint64(st.Ino),
		nlink: new(big.Int).SetUint64(uint64(st.Nlink)), rdev: new(big.Int).SetUint64(uint64(st.Rdev)),
		uid: Some(float64(st.Uid)), gid: Some(float64(st.Gid)),
		blksize: Some(big.NewInt(int64(st.Blksize))), blocks: Some(big.NewInt(st.Blocks)),
		atime: atime, mtime: mtime, birthtime: birth,
	}, true
}

func timespecMillis(t syscall.Timespec) float64 {
	return float64(t.Sec*1000 + t.Nsec/1e6)
}

// accessPath is Node's `access`: the checks of the mode on the path.
func accessPath(path string, readable, writable bool) error {
	mode := uint32(0) // F_OK
	if readable {
		mode |= 4
	}
	if writable {
		mode |= 2
	}
	return syscall.Access(path, mode)
}

// chownPath is Node's `chown`.
func chownPath(path string, uid, gid int) error { return os.Chown(path, uid, gid) }

// noFollow is the open flag that refuses a symbolic link.
func noFollow() (int, bool) { return syscall.O_NOFOLLOW, true }
