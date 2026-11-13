package flickrt

import (
	"io"
	"io/fs"
	"os"
	"path/filepath"
	"time"
)

// The operations of `FileSystem.node` beyond reading and writing whole
// files, as Node's `fs` does them, each failing with the PlatformError of
// the system call Node makes.

// fileMode is a Node mode as Go's: permissions and the setuid, setgid and
// sticky bits.
func fileMode(mode uint32) os.FileMode {
	m := os.FileMode(mode & 0o777)
	if mode&0o4000 != 0 {
		m |= os.ModeSetuid
	}
	if mode&0o2000 != 0 {
		m |= os.ModeSetgid
	}
	if mode&0o1000 != 0 {
		m |= os.ModeSticky
	}
	return m
}

// fsCall is a task of an operation that fails with the PlatformError of
// `method`, as the system call `syscall` on `path` failed.
func fsCall(method, syscall string, path Value, op func(path string) (Value, error)) Value {
	return Eff(func(*Fiber) Value {
		out, err := op(path.(string))
		if err != nil {
			return Fail(systemError(err, method, syscall, path.(string)))
		}
		return out
	})
}

func noValue(err error) (Value, error) { return Unit, err }

func fsAccess(options, path Value) Value {
	o := record(options)
	return fsCall("access", "access", path, func(p string) (Value, error) {
		return noValue(accessPath(p, o.Get("readable").(bool), o.Get("writable").(bool)))
	})
}

// copyFile copies a file's bytes and permissions, over what is there.
func copyFile(from, to string) error {
	src, err := os.Open(from)
	if err != nil {
		return err
	}
	defer src.Close()
	fi, err := src.Stat()
	if err != nil {
		return err
	}
	dst, err := os.OpenFile(to, os.O_WRONLY|os.O_CREATE|os.O_TRUNC, fi.Mode().Perm())
	if err != nil {
		return err
	}
	_, err = io.Copy(dst, src)
	if cerr := dst.Close(); err == nil {
		err = cerr
	}
	if err == nil {
		err = os.Chmod(to, fi.Mode().Perm())
	}
	return err
}

// copyTree is Node's `cp` with `recursive`: directories are made, files
// copied, a file already there kept unless `overwrite`, symbolic links
// copied as links.
func copyTree(from, to string, overwrite, preserve bool) error {
	return filepath.WalkDir(from, func(p string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		rel, _ := filepath.Rel(from, p)
		target := filepath.Join(to, rel)
		fi, err := os.Lstat(p)
		if err != nil {
			return err
		}
		switch {
		case d.IsDir():
			if err := os.MkdirAll(target, fi.Mode().Perm()|0o700); err != nil {
				return err
			}
		case fi.Mode()&fs.ModeSymlink != 0:
			link, err := os.Readlink(p)
			if err != nil {
				return err
			}
			if _, err := os.Lstat(target); err == nil {
				if !overwrite {
					return nil
				}
				os.Remove(target)
			}
			return os.Symlink(link, target)
		default:
			if _, err := os.Lstat(target); err == nil && !overwrite {
				return nil
			}
			if err := copyFile(p, target); err != nil {
				return err
			}
		}
		if preserve {
			return os.Chtimes(target, fileAtime(fi), fi.ModTime())
		}
		return nil
	})
}

// fileAtime is the access time of a stat, its modification time where the
// system does not say.
func fileAtime(fi fs.FileInfo) time.Time {
	if sf, ok := statFields(fi, nil, ""); ok {
		return time.UnixMilli(int64(sf.atime))
	}
	return fi.ModTime()
}

func fsCopy(options, from, to Value) Value {
	o := record(options)
	return fsCall("copy", "cp", from, func(p string) (Value, error) {
		return noValue(copyTree(p, to.(string), o.Get("overwrite").(bool), o.Get("preserveTimestamps").(bool)))
	})
}

func fsCopyFile(from, to Value) Value {
	return fsCall("copyFile", "copyfile", from, func(p string) (Value, error) { return noValue(copyFile(p, to.(string))) })
}

func fsChmod(mode, path Value) Value {
	return fsCall("chmod", "chmod", path, func(p string) (Value, error) {
		return noValue(os.Chmod(p, fileMode(uint32(mode.(int32)))))
	})
}

func fsChown(uid, gid, path Value) Value {
	return fsCall("chown", "chown", path, func(p string) (Value, error) {
		return noValue(chownPath(p, int(uid.(float64)), int(gid.(float64))))
	})
}

func fsGlob(options, pattern Value) Value {
	o := record(options)
	return Eff(func(*Fiber) Value {
		cwd, ok := some(o.Get("root"))
		if !ok {
			cwd, _ = os.Getwd()
		}
		var exclude []string
		for _, e := range List(o.Get("exclude")) {
			exclude = append(exclude, e.(string))
		}
		paths, err := globPaths(pattern.(string), cwd.(string), exclude)
		if err != nil {
			return Fail(systemError(err, "glob", "scandir", cwd.(string)))
		}
		return paths
	})
}

func fsLink(from, to Value) Value {
	return fsCall("link", "link", from, func(p string) (Value, error) { return noValue(os.Link(p, to.(string))) })
}

func fsSymlink(target, path Value) Value {
	return fsCall("symlink", "symlink", target, func(t string) (Value, error) { return noValue(os.Symlink(t, path.(string))) })
}

func fsReadLink(path Value) Value {
	return fsCall("readLink", "readlink", path, func(p string) (Value, error) {
		target, err := os.Readlink(p)
		return target, err
	})
}

// fsRealPath is the absolute path with its symbolic links resolved.
func fsRealPath(path Value) Value {
	return fsCall("realPath", "lstat", path, func(p string) (Value, error) {
		abs, err := filepath.Abs(p)
		if err != nil {
			return nil, err
		}
		real, err := filepath.EvalSymlinks(abs)
		return real, err
	})
}

func fsTruncate(length, path Value) Value {
	size := int64(0)
	if l, ok := some(length); ok {
		size = int64(l.(float64))
	}
	return fsCall("truncate", "open", path, func(p string) (Value, error) { return noValue(os.Truncate(p, size)) })
}

// fsUtimes sets the access and modification times, in milliseconds.
func fsUtimes(atime, mtime, path Value) Value {
	at := time.UnixMilli(0).Add(time.Duration(atime.(float64) * 1e6))
	mt := time.UnixMilli(0).Add(time.Duration(mtime.(float64) * 1e6))
	return fsCall("utime", "utime", path, func(p string) (Value, error) { return noValue(os.Chtimes(p, at, mt)) })
}

func fsOpen(options, path Value) Value {
	return Eff(func(fb *Fiber) Value { return openFile(fb, options, path.(string)) })
}
