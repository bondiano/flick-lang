package flickrt

import (
	"errors"
	"os"
	"slices"
	"strings"
	"syscall"
)

// FileSystem.glob walks the node file system: results are `/`-separated
// paths relative to the root (absolute for an absolute pattern),
// depth-first with each directory's entries in UTF-16 order. Wildcards are
// case-sensitive; leading literal segments are probed with `stat`, so they
// follow the file system's case rules.

func fsGlob(fb *Fiber, options Value, pattern string) Value {
	o := record(options)
	root := ""
	if value, ok := some(o.Get("root")); ok {
		root = value.(string)
	}
	excludes := []string{}
	for _, excluded := range List(o.Get("exclude")) {
		excludes = append(excludes, excluded.(string))
	}
	paths, failure := fsGlobWalk(fb, pattern, root, excludes, false)
	if failure != nil {
		return Fail(systemError(failure.err, failure.method, failure.syscall, failure.path))
	}
	out := make([]Value, len(paths))
	for i, path := range paths {
		out[i] = path
	}
	return out
}

// fsGlobFailure is the PlatformError the walk ends with.
type fsGlobFailure struct {
	err                   error
	method, syscall, path string
}

// fsGlobEntry is a DirectoryEntry; kind is 'd' a directory, 'l' a symbolic
// link, 'f' anything else.
type fsGlobEntry struct {
	name string
	kind byte
}

type fsGlobChild struct {
	relative string
	accepted bool
	excluded []int
	// next is the states to walk the child with, when descend is set
	next    []int
	descend bool
}

type fsGlobWalker struct {
	fb                    *Fiber
	compiled, excludes    *fsGlobCompiled
	excludeNeedsDirectory bool
	root                  string
}

func fsGlobMissing(err error) bool {
	tag := systemTagOf(err)
	return tag == "NotFound" || tag == "BadResource"
}

// fsGlobIgnorable counts EPERM too, as a denied permission.
func fsGlobIgnorable(err error) bool {
	return fsGlobMissing(err) || systemTagOf(err) == "PermissionDenied" || errors.Is(err, syscall.EPERM)
}

// fsGlobNotLink: Node reports `readlink` on a path that is not a link as an
// unknown error.
func fsGlobNotLink(err error) bool { return fsGlobIgnorable(err) || systemTagOf(err) == "UnknownError" }

func fsGlobJoin(parent, name string) string {
	switch {
	case parent == "" && name == "":
		return "/"
	case parent == "":
		return name
	case strings.HasSuffix(parent, "/"):
		return parent + name
	}
	return parent + "/" + name
}

// fsGlobWalk is the glob of `pattern` under `root`; dot lets wildcards match
// names that start with `.`.
func fsGlobWalk(fb *Fiber, pattern, root string, excludes []string, dot bool) ([]string, *fsGlobFailure) {
	w := &fsGlobWalker{fb: fb, compiled: fsGlobCompile([]string{pattern}, dot), excludes: fsGlobCompile(excludes, true), root: root}
	if len(w.compiled.initial) == 0 {
		return []string{}, nil
	}
	for _, node := range w.excludes.nodes {
		w.excludeNeedsDirectory = w.excludeNeedsDirectory || node.kind == fsGlobEnd && node.directoryOnly
	}
	return w.directory("", w.compiled.initial, w.excludes.initial, true)
}

func (w *fsGlobWalker) fsPath(relative string) string {
	switch {
	case fsGlobAbsolute(relative):
		return relative
	case relative == "" && w.root == "":
		return "."
	case relative == "":
		return w.root
	}
	return fsGlobJoin(w.root, relative)
}

// probe stats a literal name, keeping broken links so literal probes agree
// with directory listings.
func (w *fsGlobWalker) probe(parent, name string, named bool) (*fsGlobEntry, *fsGlobFailure) {
	path := w.fsPath(fsGlobJoin(parent, name))
	info, err := os.Stat(path)
	switch {
	case err == nil && info.IsDir():
		return &fsGlobEntry{name, 'd'}, nil
	case err == nil:
		return &fsGlobEntry{name, 'f'}, nil
	case systemTagOf(err) == "NotFound":
		if _, err := os.Readlink(path); err == nil {
			return &fsGlobEntry{name, 'l'}, nil
		} else if !fsGlobNotLink(err) {
			return nil, &fsGlobFailure{err, "readLink", "readlink", path}
		}
		return nil, nil
	case named && fsGlobMissing(err) || !named && fsGlobIgnorable(err):
		return nil, nil
	}
	return nil, &fsGlobFailure{err, "stat", "stat", path}
}

// entries lists a directory, probing leading literals without requiring
// permission to list their parent; named reports whether the children are
// still reached only through literals.
func (w *fsGlobWalker) entries(relative string, states []int, named bool) ([]fsGlobEntry, bool, *fsGlobFailure) {
	literals := []string{}
	listed := !named
	for _, state := range states {
		node := w.compiled.nodes[state]
		if node.kind == fsGlobLiteral && !slices.Contains(literals, node.value) {
			literals = append(literals, node.value)
		} else if node.kind != fsGlobLiteral && node.kind != fsGlobEnd {
			listed = true
		}
	}
	out := []fsGlobEntry{}
	if listed {
		path := w.fsPath(relative)
		listing, err := os.ReadDir(path)
		if err != nil && !(named && fsGlobMissing(err) || !named && fsGlobIgnorable(err)) {
			return nil, false, &fsGlobFailure{err, "readDirectoryEntries", "scandir", path}
		}
		if err == nil {
			for _, entry := range listing {
				kind := byte('f')
				if entry.Type()&os.ModeSymlink != 0 {
					kind = 'l'
				} else if entry.IsDir() {
					kind = 'd'
				}
				out = append(out, fsGlobEntry{entry.Name(), kind})
			}
		}
	}
	for _, name := range literals {
		// listings never contain `.`, `..`, or the root of an absolute pattern
		if listed && name != "." && name != ".." && !fsGlobRoot(name) {
			continue
		}
		entry, failure := w.probe(relative, name, named)
		if failure != nil {
			return nil, false, failure
		}
		if entry != nil {
			out = append(out, *entry)
		}
	}
	slices.SortStableFunc(out, func(a, b fsGlobEntry) int { return CompareStrings(a.name, b.name) })
	return out, named && !listed, nil
}

// visit matches an entry, resolving a link's target only when the match
// depends on it. Node's listing types are never unknown here: Go resolves
// those itself.
func (w *fsGlobWalker) visit(entry fsGlobEntry, parent string, states, excluded []int) (*fsGlobChild, *fsGlobFailure) {
	explicit, globstar := w.compiled.step(states, entry.name)
	next := fsGlobUnion(explicit, globstar)
	if len(next) == 0 {
		return nil, nil
	}
	relative := fsGlobJoin(parent, entry.name)
	nextExcluded := fsGlobUnion(w.excludes.step(excluded, entry.name))
	descend := w.compiled.canDescend(next)
	child := func(directory, link bool) *fsGlobChild {
		accepted := w.compiled.accepts(next, directory)
		if !accepted && (!directory || !descend) || w.excludes.accepts(nextExcluded, directory) {
			return nil
		}
		out := &fsGlobChild{relative: relative, accepted: accepted, excluded: nextExcluded}
		switch {
		case !directory || !descend:
		case !link || len(globstar) == 0:
			out.next, out.descend = next, true
		// follow symlinks only through explicit segments, not `**`
		case w.compiled.canDescend(explicit):
			out.next, out.descend = explicit, true
		}
		return out
	}
	if !descend && !w.compiled.needsDirectory(next) && !w.excludeNeedsDirectory {
		return child(false, false), nil
	}
	if entry.kind != 'l' {
		return child(entry.kind == 'd', false), nil
	}
	path := w.fsPath(relative)
	info, err := os.Stat(path)
	if err != nil && !fsGlobIgnorable(err) {
		return nil, &fsGlobFailure{err, "stat", "stat", path}
	}
	return child(err == nil && info.IsDir(), true), nil
}

func (w *fsGlobWalker) directory(relative string, states, excluded []int, named bool) ([]string, *fsGlobFailure) {
	if w.fb != nil {
		Check(w.fb)
	}
	listing, named, failure := w.entries(relative, states, named)
	if failure != nil {
		return nil, failure
	}
	children := make([]*fsGlobChild, 0, len(listing))
	for _, entry := range listing {
		child, failure := w.visit(entry, relative, states, excluded)
		if failure != nil {
			return nil, failure
		}
		children = append(children, child)
	}
	out := []string{}
	for _, child := range children {
		if child == nil {
			continue
		}
		if child.accepted {
			out = append(out, child.relative)
		}
		if child.descend {
			paths, failure := w.directory(child.relative, child.next, child.excluded, named)
			if failure != nil {
				return nil, failure
			}
			out = append(out, paths...)
		}
	}
	return out, nil
}
