package flickrt

// `Stream.watch`: the changes under a path, as Effect's watch on Node
// reports them: the path of what changed, relative to a watched
// directory, created, updated or removed.
//
// shortcut: the path is polled every 50 ms, as Go's standard library has no file system notifications; watch with inotify, FSEvents and ReadDirectoryChangesW if a program needs changes sooner or under many files

import (
	"io/fs"
	"os"
	"path/filepath"
	"sort"
	"time"
)

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
			out[filepath.ToSlash(rel)] = watchEntry{i.ModTime(), i.Size()}
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
	return &Stream{open: func(fb *Fiber) pull {
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
	}}
}

// StreamWatch is the changes under the path, until the stream ends.
func StreamWatch(options, path Value) Value {
	return pollStream(path.(string), record(options).Get("recursive").(bool))
}

// StreamWatchWith watches through the backend where it gives a stream for
// the path and what stat tells of it.
func StreamWatchWith(backend, options, path Value) Value {
	p := path.(string)
	recursive := record(options).Get("recursive").(bool)
	return &Stream{open: func(fb *Fiber) pull {
		info, err := os.Stat(p)
		if err != nil {
			Fail(systemError(err, "watch", "stat", p))
		}
		if s, ok := some(Apply(fb, Apply(fb, backend, p), infoOf(info))); ok {
			return streamOf(s).open(fb)
		}
		return pollStream(p, recursive).open(fb)
	}}
}
