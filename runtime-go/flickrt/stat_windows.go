package flickrt

import (
	"io/fs"
	"math/big"
	"os"
	"syscall"
)

// statFields are the fields of Node's `stat` that Go's FileInfo does not
// give, as libuv reads them on windows: the volume and file index of the
// file's handle, permissions 0o666 or 0o444 by the read-only attribute.
func statFields(fi fs.FileInfo, f *os.File, path string) (sf statInfo, ok bool) {
	attrs, isAttrs := fi.Sys().(*syscall.Win32FileAttributeData)
	if !isAttrs {
		return sf, false
	}
	var h syscall.Handle
	if f != nil {
		h = syscall.Handle(f.Fd())
	} else if p, err := syscall.UTF16PtrFromString(path); err == nil {
		opened, err := syscall.CreateFile(p, 0, syscall.FILE_SHARE_READ|syscall.FILE_SHARE_WRITE|syscall.FILE_SHARE_DELETE,
			nil, syscall.OPEN_EXISTING, syscall.FILE_FLAG_BACKUP_SEMANTICS, 0)
		if err == nil {
			defer syscall.CloseHandle(opened)
			h = opened
		}
	}
	var info syscall.ByHandleFileInformation
	known := h != 0 && syscall.GetFileInformationByHandle(h, &info) == nil
	mode := uint32(0o666)
	if attrs.FileAttributes&syscall.FILE_ATTRIBUTE_READONLY != 0 {
		mode = 0o444
	}
	switch {
	case fi.Mode()&fs.ModeSymlink != 0:
		mode |= 0o120000
	case fi.IsDir():
		mode |= 0o040000
	default:
		mode |= 0o100000
	}
	size := int64(attrs.FileSizeHigh)<<32 | int64(attrs.FileSizeLow)
	sf = statInfo{
		mode: mode, dev: big.NewInt(0), ino: big.NewInt(0), nlink: big.NewInt(1), rdev: big.NewInt(0),
		uid: Some(0.0), gid: Some(0.0), blksize: Some(big.NewInt(4096)), blocks: Some(big.NewInt((size + 4095) / 4096 * 8)),
		atime:     float64(attrs.LastAccessTime.Nanoseconds() / 1e6),
		mtime:     float64(attrs.LastWriteTime.Nanoseconds() / 1e6),
		birthtime: float64(attrs.CreationTime.Nanoseconds() / 1e6),
	}
	if known {
		sf.dev = big.NewInt(int64(info.VolumeSerialNumber))
		sf.ino = new(big.Int).SetUint64(uint64(info.FileIndexHigh)<<32 | uint64(info.FileIndexLow))
		sf.nlink = big.NewInt(int64(info.NumberOfLinks))
	}
	return sf, true
}

// accessPath is Node's `access` on windows: the path exists, and is not
// read-only when it should be writable.
func accessPath(path string, _, writable bool) error {
	fi, err := os.Stat(path)
	if err == nil && writable && fi.Mode().Perm()&0o200 == 0 {
		return &fs.PathError{Op: "access", Path: path, Err: fs.ErrPermission}
	}
	return err
}

// chownPath does nothing, as libuv's `chown` on windows.
func chownPath(string, int, int) error { return nil }

// noFollow is unavailable: libuv ignores the flag on windows.
func noFollow() (int, bool) { return 0, false }

