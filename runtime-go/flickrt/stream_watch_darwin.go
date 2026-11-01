package flickrt

// kqueue: a descriptor per directory and file, as FSEvents, which Node
// uses on macOS, has no API in package syscall. A directory that changed
// is listed again and compared with its last listing; a file reports its
// writes. Unlike FSEvents nothing is coalesced, and a file created as its
// new directory is listed may be reported twice.

import (
	"errors"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"syscall"
)

const kqueueNotes = syscall.NOTE_DELETE | syscall.NOTE_WRITE | syscall.NOTE_EXTEND | syscall.NOTE_ATTRIB |
	syscall.NOTE_RENAME | syscall.NOTE_REVOKE

type kqueueNode struct {
	rel   string
	names map[string]bool // a directory's last listing; nil for a file
}

type kqueue struct {
	q         *watchQueue
	kq        int
	nodes     map[int]*kqueueNode // by descriptor
	fds       map[string]int      // by relative path
	file      bool                // the watched path is a file
	recursive bool
}

func watchNative(q *watchQueue, path string, dir, recursive bool) (func(), error) {
	kq, err := syscall.Kqueue()
	if err != nil {
		return nil, err
	}
	syscall.CloseOnExec(kq)
	w := &kqueue{q: q, kq: kq, nodes: map[int]*kqueueNode{}, fds: map[string]int{}, file: !dir, recursive: recursive}
	wake := []syscall.Kevent_t{{Filter: syscall.EVFILT_USER, Flags: syscall.EV_ADD | syscall.EV_CLEAR}}
	if _, err = syscall.Kevent(kq, wake, nil, nil); err == nil {
		if dir {
			err = w.addDir("", false)
		} else {
			_, err = w.open(filepath.Base(path), nil)
		}
	}
	if err != nil {
		w.close()
		return nil, err
	}
	// the descriptors are closed by the watch's goroutine alone, and the
	// kqueue woken only while it is open
	var mu sync.Mutex
	ended := false
	done := make(chan struct{})
	go func() {
		defer close(done)
		w.run()
		mu.Lock()
		ended = true
		w.close()
		mu.Unlock()
	}()
	return func() {
		mu.Lock()
		if !ended {
			wake[0].Flags, wake[0].Fflags = 0, syscall.NOTE_TRIGGER
			syscall.Kevent(kq, wake, nil, nil)
		}
		mu.Unlock()
		<-done
	}, nil
}

// open watches `rel` under a descriptor of its own.
func (w *kqueue) open(rel string, names map[string]bool) (int, error) {
	fd, err := syscall.Open(filepath.Join(w.q.base, rel), syscall.O_EVTONLY|syscall.O_CLOEXEC, 0)
	if err != nil {
		return -1, err
	}
	ev := []syscall.Kevent_t{{Ident: uint64(fd), Filter: syscall.EVFILT_VNODE, Flags: syscall.EV_ADD | syscall.EV_CLEAR, Fflags: kqueueNotes}}
	if _, err := syscall.Kevent(w.kq, ev, nil, nil); err != nil {
		syscall.Close(fd)
		return -1, err
	}
	w.nodes[fd] = &kqueueNode{rel, names}
	w.fds[rel] = fd
	return fd, nil
}

// addDir watches the directory `rel` and what is in it, reporting that
// when it is new.
func (w *kqueue) addDir(rel string, report bool) error {
	if _, err := w.open(rel, map[string]bool{}); err != nil {
		return err
	}
	return w.list(rel, report)
}

// list compares the directory `rel` with its last listing: an entry that
// came or went is renamed, a new one watched. Only as the watch starts
// does it fail, out of descriptors.
func (w *kqueue) list(rel string, report bool) error {
	node := w.nodes[w.fds[rel]]
	entries, err := os.ReadDir(filepath.Join(w.q.base, rel))
	if err != nil {
		return nil // removed: its parent's listing tells
	}
	now := make(map[string]bool, len(entries))
	for _, e := range entries {
		now[e.Name()] = true
		if node.names[e.Name()] {
			continue
		}
		// a file is watched before it is reported, a directory listed after
		child := filepath.Join(rel, e.Name())
		var err error
		if !e.IsDir() {
			_, err = w.open(child, nil)
		}
		if report {
			w.q.rename(child)
		}
		if e.IsDir() && w.recursive {
			err = w.addDir(child, report)
		}
		if err != nil && !report && watchPolled(err) {
			return err
		}
	}
	for name := range node.names {
		if !now[name] {
			child := filepath.Join(rel, name)
			w.forget(child)
			w.q.rename(child)
		}
	}
	node.names = now
	return nil
}

// forget stops watching `rel` and what is under it.
func (w *kqueue) forget(rel string) {
	for path, fd := range w.fds {
		if rel == "" || path == rel || strings.HasPrefix(path, rel+string(filepath.Separator)) {
			syscall.Close(fd)
			delete(w.fds, path)
			delete(w.nodes, fd)
		}
	}
}

func (w *kqueue) close() {
	for fd := range w.nodes {
		syscall.Close(fd)
	}
	syscall.Close(w.kq)
}

// run takes one event at a time: a descriptor closed as one is handled
// may be reused by the next, so none is left pending.
func (w *kqueue) run() {
	events := make([]syscall.Kevent_t, 1)
	for {
		n, err := syscall.Kevent(w.kq, nil, events, nil)
		if errors.Is(err, syscall.EINTR) || err == nil && n == 0 {
			continue
		}
		if err != nil {
			w.q.fail()
			return
		}
		e := events[0]
		if e.Filter == syscall.EVFILT_USER {
			return
		}
		node, ok := w.nodes[int(e.Ident)]
		if !ok {
			continue
		}
		gone := e.Fflags&(syscall.NOTE_DELETE|syscall.NOTE_RENAME|syscall.NOTE_REVOKE) != 0
		switch {
		case node.names != nil && gone:
			// its parent's listing tells; the watched directory itself
			// is not reported, as with FSEvents
			w.forget(node.rel)
		case node.names != nil:
			w.list(node.rel, true)
		case gone && w.file:
			w.forget(node.rel)
			w.q.rename(node.rel)
		case gone:
			// a file replaced keeps its name in the listing: watch the new one
			w.forget(node.rel)
			if _, err := w.open(node.rel, nil); err == nil {
				w.q.rename(node.rel)
			}
		default:
			w.q.change(node.rel)
		}
	}
}
