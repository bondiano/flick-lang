package flickrt

import (
	"errors"
	"io"
	"io/fs"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"syscall"
)

func fsWithin(parent, path string) bool {
	rel, err := filepath.Rel(parent, path)
	return err == nil && rel != ".." && !strings.HasPrefix(rel, ".."+string(filepath.Separator))
}

// Check existing ancestors too: a destination's parent can be a symlink into src.
func fsCopyDestination(src, dst string, info fs.FileInfo) error {
	source, err := filepath.Abs(src)
	if err != nil {
		return err
	}
	dest, err := filepath.Abs(dst)
	if err != nil {
		return err
	}
	if fsWithin(source, dest) {
		return &os.PathError{Op: "cp", Path: dst, Err: syscall.EINVAL}
	}
	for parent := filepath.Dir(dest); ; parent = filepath.Dir(parent) {
		ancestor, err := os.Stat(parent)
		if err == nil && os.SameFile(info, ancestor) {
			return &os.PathError{Op: "cp", Path: dst, Err: syscall.EINVAL}
		}
		if err != nil && !errors.Is(err, fs.ErrNotExist) {
			return err
		}
		if parent == filepath.Dir(parent) {
			break
		}
	}
	// Lexical parents are not enough: an existing ancestor may be a symlink
	// into the source, so resolve the nearest one and append what does not
	// exist yet before any directory is made.
	physicalSource, err := filepath.EvalSymlinks(source)
	if err != nil {
		return err
	}
	physicalDest, err := fsPhysicalPath(dest)
	if err != nil {
		return err
	}
	if fsWithin(physicalSource, physicalDest) {
		return &os.PathError{Op: "cp", Path: dst, Err: syscall.EINVAL}
	}
	if existing, err := os.Stat(physicalDest); err == nil && os.SameFile(info, existing) {
		return &os.PathError{Op: "cp", Path: dst, Err: syscall.EINVAL}
	}
	return nil
}

// fsPhysicalPath is `path` with its existing prefix freed of symlinks and the
// part that does not exist yet joined back as written.
func fsPhysicalPath(path string) (string, error) {
	suffix := []string{}
	for {
		resolved, err := filepath.EvalSymlinks(path)
		if err == nil {
			suffix = append(suffix, resolved)
			slices.Reverse(suffix)
			return filepath.Join(suffix...), nil
		}
		if !errors.Is(err, fs.ErrNotExist) {
			return "", err
		}
		parent := filepath.Dir(path)
		if parent == path {
			return "", err
		}
		suffix = append(suffix, filepath.Base(path))
		path = parent
	}
}

// Open without truncation, compare the actual descriptors, then truncate. A
// destination symlink or hardlink must never truncate the source itself.
func fsCopyFile(src, dst string, exclusive bool) (info fs.FileInfo, err error) {
	in, err := os.Open(src)
	if err != nil {
		return nil, err
	}
	defer func() {
		if closeErr := in.Close(); err == nil {
			err = closeErr
		}
	}()
	info, err = in.Stat()
	if err != nil {
		return nil, err
	}
	if info.IsDir() {
		return nil, &os.PathError{Op: "copyfile", Path: src, Err: syscall.EISDIR}
	}
	flags := os.O_WRONLY | os.O_CREATE
	if exclusive {
		flags |= os.O_EXCL
	}
	out, err := os.OpenFile(dst, flags, info.Mode().Perm())
	if err != nil {
		return nil, err
	}
	defer func() {
		if closeErr := out.Close(); err == nil {
			err = closeErr
		}
	}()
	dest, err := out.Stat()
	if err != nil {
		return nil, err
	}
	if os.SameFile(info, dest) {
		// Node's copyFile leaves a source copied onto itself (or a hardlink)
		// untouched instead of truncating it.
		return info, nil
	}
	if err = out.Truncate(0); err != nil {
		return nil, err
	}
	// native copyFile moves exactly the captured st_size, so a device with
	// size 0 yields an empty file instead of an unbounded stream.
	if _, err = io.CopyN(out, in, info.Size()); err != nil && !errors.Is(err, io.EOF) {
		return nil, err
	}
	// Node re-stats after reading, since the read may have changed atime.
	info, err = in.Stat()
	if err == nil {
		err = out.Chmod(info.Mode())
	}
	return info, err
}

func fsCopy(src, dst string, overwrite, preserveTimes bool) error {
	info, err := os.Lstat(src)
	if err != nil {
		return err
	}
	if info.IsDir() {
		if err := fsCopyDestination(src, dst, info); err != nil {
			return err
		}
	}
	return fsCopyTree(src, dst, info, overwrite, preserveTimes)
}

func fsCopyTree(src, dst string, info fs.FileInfo, overwrite, preserveTimes bool) error {
	dest, err := os.Lstat(dst)
	if err != nil && !errors.Is(err, fs.ErrNotExist) {
		return err
	}
	if err == nil {
		if os.SameFile(info, dest) {
			return &os.PathError{Op: "cp", Path: dst, Err: syscall.EINVAL}
		}
		// Node's ERR_FS_CP_DIR_TO_NON_DIR and ERR_FS_CP_NON_DIR_TO_DIR carry no errno, so they are read as UnknownError
		if info.IsDir() != dest.IsDir() {
			return &os.PathError{Op: "cp", Path: dst, Err: syscall.EINVAL}
		}
	} else {
		dest = nil
	}
	if err := os.MkdirAll(filepath.Dir(dst), 0o777); err != nil {
		return err
	}
	switch {
	case info.IsDir():
		if dest == nil {
			if err := os.Mkdir(dst, info.Mode().Perm()|0o700); err != nil {
				return err
			}
		}
		entries, err := os.ReadDir(src)
		if err != nil {
			return err
		}
		for _, entry := range entries {
			child, err := entry.Info()
			if err != nil {
				return err
			}
			if err := fsCopyTree(filepath.Join(src, entry.Name()), filepath.Join(dst, entry.Name()), child, overwrite, preserveTimes); err != nil {
				return err
			}
		}
		if dest == nil {
			return os.Chmod(dst, info.Mode())
		}
		return nil
	case info.Mode()&fs.ModeSymlink != 0:
		return fsCopyLink(src, dst, dest)
	case info.Mode().IsRegular() || info.Mode()&fs.ModeDevice != 0:
		if dest != nil {
			if !overwrite {
				return nil
			}
			if err := os.Remove(dst); err != nil {
				return err
			}
		}
		copied, err := fsCopyFile(src, dst, true)
		if err != nil {
			return err
		}
		if preserveTimes {
			if copied.Mode().Perm()&0o200 == 0 {
				if err := os.Chmod(dst, copied.Mode()|0o200); err != nil {
					return err
				}
			}
			err := os.Chtimes(dst, fsAccessTime(copied), copied.ModTime())
			if modeErr := os.Chmod(dst, copied.Mode()); err == nil {
				err = modeErr
			}
			return err
		}
		return nil
	default:
		return &os.PathError{Op: "cp", Path: src, Err: syscall.EINVAL}
	}
}

func fsCopyLink(src, dst string, dest fs.FileInfo) error {
	target, err := os.Readlink(src)
	if err != nil {
		return err
	}
	if !filepath.IsAbs(target) {
		target, err = filepath.Abs(filepath.Join(filepath.Dir(src), target))
		if err != nil {
			return err
		}
	}
	if dest != nil {
		if dest.Mode()&fs.ModeSymlink == 0 {
			return os.Symlink(target, dst)
		}
		destTarget, err := os.Readlink(dst)
		if err != nil {
			return err
		}
		if !filepath.IsAbs(destTarget) {
			destTarget, err = filepath.Abs(filepath.Join(filepath.Dir(dst), destTarget))
			if err != nil {
				return err
			}
		}
		followed, err := os.Stat(src)
		if err != nil {
			return err
		}
		if followed.IsDir() && (fsWithin(target, destTarget) || fsWithin(destTarget, target)) {
			return &os.PathError{Op: "cp", Path: dst, Err: syscall.EINVAL}
		}
		if err := os.Remove(dst); err != nil {
			return err
		}
	}
	return os.Symlink(target, dst)
}
