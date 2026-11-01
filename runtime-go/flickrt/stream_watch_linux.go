package flickrt

// inotify, with libuv's mask; a recursive watch has a watch per
// directory, as Node's recursive watch on Linux (recursive_watch.js).

import (
	"encoding/binary"
	"errors"
	"io/fs"
	"os"
	"path/filepath"
	"strings"
	"syscall"
)

const inotifyMask = syscall.IN_ATTRIB | syscall.IN_CREATE | syscall.IN_MODIFY | syscall.IN_DELETE |
	syscall.IN_DELETE_SELF | syscall.IN_MOVE_SELF | syscall.IN_MOVED_FROM | syscall.IN_MOVED_TO

type inotify struct {
	q         *watchQueue
	f         *os.File         // nonblocking, so reads wait in the netpoller and Close wakes them
	dirs      map[int32]string // the directory of each watch, relative to the watched one
	path      string
	dir       bool
	recursive bool
}

func watchNative(q *watchQueue, path string, dir, recursive bool) (func(), error) {
	fd, err := syscall.InotifyInit1(syscall.IN_CLOEXEC | syscall.IN_NONBLOCK)
	if err != nil {
		return nil, err
	}
	w := &inotify{q: q, f: os.NewFile(uintptr(fd), "inotify"), dirs: map[int32]string{}, path: path, dir: dir, recursive: recursive}
	if err := w.add(""); err != nil {
		w.f.Close()
		return nil, err
	}
	if dir && recursive {
		if err := w.addTree("", false); err != nil {
			w.f.Close()
			return nil, err
		}
	}
	done := make(chan struct{})
	go func() {
		defer close(done)
		w.read()
	}()
	return func() { w.f.Close(); <-done }, nil
}

// add watches the directory `rel`, or the watched file.
func (w *inotify) add(rel string) error {
	conn, err := w.f.SyscallConn()
	if err != nil {
		return err
	}
	var wd int
	var added error
	if err := conn.Control(func(fd uintptr) {
		wd, added = syscall.InotifyAddWatch(int(fd), filepath.Join(w.path, rel), inotifyMask)
	}); err != nil {
		return err
	}
	if added == nil {
		w.dirs[int32(wd)] = rel
	}
	return added
}

// addTree watches the directories under `rel`, reporting what is in them
// when they are new.
func (w *inotify) addTree(rel string, report bool) error {
	top := filepath.Join(w.path, rel)
	return filepath.WalkDir(top, func(p string, d fs.DirEntry, err error) error {
		if err != nil || p == top {
			return nil
		}
		sub, _ := filepath.Rel(w.path, p)
		if report {
			w.q.rename(sub)
		}
		if !d.IsDir() {
			return nil
		}
		if err := w.add(sub); err != nil && !errors.Is(err, syscall.ENOENT) && !errors.Is(err, syscall.ENOTDIR) {
			return err
		}
		return nil
	})
}

// forget stops watching the directories at and under `rel`, moved or
// removed.
func (w *inotify) forget(rel string) {
	conn, err := w.f.SyscallConn()
	if err != nil {
		return
	}
	for wd, dir := range w.dirs {
		if dir == rel || strings.HasPrefix(dir, rel+string(filepath.Separator)) {
			delete(w.dirs, wd)
			conn.Control(func(fd uintptr) { syscall.InotifyRmWatch(int(fd), uint32(wd)) })
		}
	}
}

func (w *inotify) read() {
	buf := make([]byte, 64<<10)
	for {
		n, err := w.f.Read(buf)
		if errors.Is(err, os.ErrClosed) {
			return
		}
		if err != nil {
			w.q.fail()
			return
		}
		for off := 0; off+syscall.SizeofInotifyEvent <= n; {
			wd := int32(binary.NativeEndian.Uint32(buf[off:]))
			mask := binary.NativeEndian.Uint32(buf[off+4:])
			size := int(binary.NativeEndian.Uint32(buf[off+12:]))
			name := strings.TrimRight(string(buf[off+syscall.SizeofInotifyEvent:off+syscall.SizeofInotifyEvent+size]), "\x00")
			off += syscall.SizeofInotifyEvent + size
			if err := w.event(wd, mask, name); err != nil {
				w.q.fail()
				return
			}
		}
	}
}

func (w *inotify) event(wd int32, mask uint32, name string) error {
	dir, ok := w.dirs[wd]
	if !ok {
		return nil
	}
	if mask&syscall.IN_IGNORED != 0 {
		delete(w.dirs, wd)
		return nil
	}
	changed := mask&(syscall.IN_ATTRIB|syscall.IN_MODIFY) != 0
	if name == "" {
		// the watched path itself, by its name as libuv reports it; Node's
		// recursive watch reports nothing of its directories, and of a
		// file only the changes
		switch {
		case w.dir && (w.recursive || dir != ""):
		case w.recursive && !w.dir:
			if _, err := os.Stat(w.path); err == nil {
				w.q.change(filepath.Base(w.path))
			}
		case changed:
			w.q.change(filepath.Base(w.path))
		default:
			w.q.rename(filepath.Base(w.path))
		}
		return nil
	}
	rel := filepath.Join(dir, name)
	isDir := mask&syscall.IN_ISDIR != 0
	switch {
	case changed && !(w.recursive && isDir):
		w.q.change(rel)
	case changed:
	case !w.recursive:
		w.q.rename(rel)
	case mask&(syscall.IN_CREATE|syscall.IN_MOVED_TO) != 0:
		w.q.rename(rel)
		if isDir {
			if err := w.add(rel); err != nil && !errors.Is(err, syscall.ENOENT) && !errors.Is(err, syscall.ENOTDIR) {
				return err
			}
			return w.addTree(rel, true)
		}
	default:
		w.q.rename(rel)
		if isDir {
			w.forget(rel)
		}
	}
	return nil
}
