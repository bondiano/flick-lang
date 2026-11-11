package flickrt

// `Stream.watch`: the changes under a path, as a watch on Node reports
// them: the path of what changed, relative to the watched directory, or
// the name of a watched file. The system tells that a path was renamed or
// changed: a rename is a Create when the path exists afterwards and a
// Remove when it does not, a change an Update. Each system's notifications
// are in a stream_watch_<os>.go; without them, or out of watches or
// descriptors, the path is polled, where Node would fail.

import (
	"errors"
	"io/fs"
	"os"
	"path/filepath"
	"sort"
	"sync"
	"syscall"
	"time"
)

// watchQueue is what a native watch saw and no pull took yet: the
// watch's goroutine never waits for the stream's reader.
type watchQueue struct {
	base   string // the directory the relative paths start from
	path   string // the watched path, of the failure
	mu     sync.Mutex
	events []Value
	failed bool
	ready  chan struct{} // holds a token while there is something to take
}

func (q *watchQueue) add(event Value) {
	q.mu.Lock()
	q.events = append(q.events, event)
	q.mu.Unlock()
	q.notify()
}

func (q *watchQueue) notify() {
	select {
	case q.ready <- struct{}{}:
	default:
	}
}

// rename reports that `rel` was created or removed, as stat tells.
func (q *watchQueue) rename(rel string) {
	kind := "Remove"
	if _, err := os.Stat(filepath.Join(q.base, rel)); err == nil {
		kind = "Create"
	}
	q.add(watchEvent(kind, rel))
}

func (q *watchQueue) change(rel string) { q.add(watchEvent("Update", rel)) }

// fail ends the stream, after the events queued before, as Node's
// watcher fails with an error.
func (q *watchQueue) fail() {
	q.mu.Lock()
	q.failed = true
	q.mu.Unlock()
	q.notify()
}

func (q *watchQueue) pull(fb *Fiber) ([]Value, bool) {
	for {
		q.mu.Lock()
		events, failed := q.events, q.failed
		q.events = nil
		q.mu.Unlock()
		if len(events) > 0 {
			return events, true
		}
		if failed {
			Fail(NewData(platformErrorTag, NewData(systemReasonTag, Rec(
				"tag", Nullary(Tag("PlatformError.SystemErrorTag.UnknownError")), "module_", "FileSystem",
				"method", "watch", "description", None, "syscall", None, "pathOrDescriptor", Some(q.path)))))
		}
		signal := signalOf(fb)
		done := park(fb, nil)
		select {
		case <-q.ready:
			done()
		case <-signal:
			done()
			panic(fb.state.stopped())
		}
	}
}

// watchPolled is whether a native watch could not start for want of
// watches or descriptors, or the system has none.
func watchPolled(err error) bool {
	return errors.Is(err, errors.ErrUnsupported) || errors.Is(err, syscall.ENOSPC) ||
		errors.Is(err, syscall.EMFILE) || errors.Is(err, syscall.ENFILE)
}

// watching is the pull of the changes under the path stat told of, its
// watch stopped as the stream's scope closes.
func watching(fb *Fiber, path string, info fs.FileInfo, recursive bool) pull {
	q := &watchQueue{base: path, path: path, ready: make(chan struct{}, 1)}
	if !info.IsDir() {
		q.base = filepath.Dir(path)
	}
	stop, err := watchNative(q, path, info.IsDir(), recursive)
	if watchPolled(err) {
		return pollStream(path, recursive).open(fb)
	}
	if err != nil {
		q.fail()
		return q.pull
	}
	fb.owner.add(fb, func(*Fiber, *Cause) Value { stop(); return Unit })
	return q.pull
}

const watchInterval = 50 * time.Millisecond

type watchEntry struct {
	mod  time.Time
	size int64
}

// watchScan is what is under the path: a directory's entries by relative
// path, or a file by its name.
func watchScan(path string, recursive bool) (map[string]watchEntry, error) {
	info, err := os.Stat(path)
	if err != nil {
		return nil, err
	}
	out := map[string]watchEntry{}
	if !info.IsDir() {
		out[filepath.Base(path)] = watchEntry{info.ModTime(), info.Size()}
		return out, nil
	}
	err = filepath.WalkDir(path, func(p string, d fs.DirEntry, err error) error {
		if err != nil || p == path {
			return nil
		}
		if i, err := d.Info(); err == nil {
			rel, _ := filepath.Rel(path, p)
			out[rel] = watchEntry{i.ModTime(), i.Size()}
		}
		if d.IsDir() && !recursive {
			return filepath.SkipDir
		}
		return nil
	})
	return out, err
}

func watchEvent(kind, path string) Value {
	return NewData(Tag("Stream.Platform.WatchEvent."+kind), path)
}

// watchChanges is the events from one scan to the next, in path order.
func watchChanges(before, after map[string]watchEntry) []Value {
	var paths []string
	for p := range after {
		paths = append(paths, p)
	}
	for p := range before {
		if _, ok := after[p]; !ok {
			paths = append(paths, p)
		}
	}
	sort.Strings(paths)
	var out []Value
	for _, p := range paths {
		old, was := before[p]
		now, is := after[p]
		switch {
		case !was:
			out = append(out, watchEvent("Create", p))
		case !is:
			out = append(out, watchEvent("Remove", p))
		case !old.mod.Equal(now.mod) || old.size != now.size:
			out = append(out, watchEvent("Update", p))
		}
	}
	return out
}

func pollStream(path string, recursive bool) *Stream {
	return streamOpen(func(fb *Fiber) pull {
		seen, err := watchScan(path, recursive)
		if err != nil {
			Fail(systemError(err, "watch", "watch", path))
		}
		return func(fb *Fiber) ([]Value, bool) {
			for {
				tick := make(chan struct{})
				time.AfterFunc(watchInterval, func() { close(tick) })
				wait(fb, tick)
				now, err := watchScan(path, recursive)
				if err != nil {
					now = map[string]watchEntry{}
				}
				events := watchChanges(seen, now)
				seen = now
				if len(events) > 0 {
					return events, true
				}
			}
		}
	})
}

// StreamWatch is the changes under the path, until the stream ends.
func StreamWatch(options, path Value) Value {
	return StreamWatchWith(nil, options, path)
}

// StreamWatchWith watches through the backend where it gives a stream for
// the path and what stat tells of it; it needs the file system of
// `FileSystem.node`, and fails with NotFound under another.
func StreamWatchWith(backend, options, path Value) Value {
	p := path.(string)
	recursive := record(options).Get("recursive").(bool)
	return streamOpen(func(fb *Fiber) pull {
		requireNode(fb, "FileSystem.FileSystem", "FileSystem", "watch", p)
		info, err := os.Stat(p)
		if err != nil {
			Fail(systemError(err, "stat", "stat", p))
		}
		if backend != nil {
			if s, ok := some(Apply(fb, Apply(fb, backend, p), infoOf(info))); ok {
				return streamOf(s).open(fb)
			}
		}
		return watching(fb, p, info, recursive)
	})
}
