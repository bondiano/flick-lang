package flickrt

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"slices"
	"strings"
	"testing"
)

// fsTestGlobTree builds the tree of testdata/glob.txt: a path is a file,
// ["d", path, mode?] a directory and ["l", path, target] a link.
func fsTestGlobTree(t *testing.T, root string, tree []json.RawMessage) {
	t.Helper()
	for _, raw := range tree {
		var entry []any
		if json.Unmarshal(raw, &entry) != nil {
			var path string
			if err := json.Unmarshal(raw, &path); err != nil {
				t.Fatal(err)
			}
			entry = []any{"f", path}
		}
		at := filepath.Join(root, filepath.FromSlash(entry[1].(string)))
		switch {
		case entry[0] == "d" && len(entry) > 2:
			if err := os.Chmod(at, os.FileMode(entry[2].(float64))); err != nil {
				t.Fatal(err)
			}
			t.Cleanup(func() { _ = os.Chmod(at, 0o700) })
		case entry[0] == "d":
			if err := os.MkdirAll(at, 0o700); err != nil {
				t.Fatal(err)
			}
		default:
			if err := os.MkdirAll(filepath.Dir(at), 0o700); err != nil {
				t.Fatal(err)
			}
			if entry[0] == "l" {
				fsTestSymlink(t, entry[2].(string), at)
			} else {
				fsTestWrite(t, at, entry[1].(string))
			}
		}
	}
}

// testdata/glob.txt is Effect's FileSystem.glob of each case in a fixture
// tree, written by tools/go-glob/gen.mjs.
func TestFileSystemGlobAsEffect(t *testing.T) {
	if runtime.GOOS == "windows" || os.Geteuid() == 0 {
		t.Skip("the fixture needs symlinks and permission bits that bind")
	}
	data, err := os.ReadFile("testdata/glob.txt")
	if err != nil {
		t.Fatal(err)
	}
	lines := strings.Split(strings.TrimSuffix(string(data), "\n"), "\n")
	header, ok := strings.CutPrefix(lines[0], "tree\t")
	var tree []json.RawMessage
	if !ok || json.Unmarshal([]byte(header), &tree) != nil {
		t.Fatalf("bad tree line %q", lines[0])
	}
	dir := t.TempDir()
	fsTestGlobTree(t, dir, tree)
	cwd, err := os.Getwd()
	if err != nil {
		t.Fatal(err)
	}
	if err := os.Chdir(dir); err != nil {
		t.Fatal(err)
	}
	defer func() { _ = os.Chdir(cwd) }()
	abs := func(text string) string { return strings.ReplaceAll(text, "<abs>", filepath.ToSlash(dir)) }
	for _, line := range lines[1:] {
		input, want, _ := strings.Cut(line, "\t")
		var c struct {
			Pattern string
			Root    string
			Exclude []string
			Dot     bool
		}
		if err := json.Unmarshal([]byte(input), &c); err != nil {
			t.Fatalf("bad case %q: %v", input, err)
		}
		excludes := make([]string, len(c.Exclude))
		for i, excluded := range c.Exclude {
			excludes[i] = abs(excluded)
		}
		paths, failure := fsGlobWalk(nil, abs(c.Pattern), abs(c.Root), excludes, c.Dot)
		got := ""
		if failure != nil {
			got = fmt.Sprintf("error: %s %s %s %s", systemTagOf(failure.err), failure.method, failure.syscall, failure.path)
		} else {
			encoded, _ := json.Marshal(paths)
			got = string(encoded)
		}
		var wantPaths []string
		if json.Unmarshal([]byte(abs(want)), &wantPaths) == nil && failure == nil {
			if !slices.Equal(paths, wantPaths) {
				t.Errorf("%s:\n got %q\nwant %q", input, paths, wantPaths)
			}
		} else if got != abs(want) {
			t.Errorf("%s:\n got %s\nwant %s", input, got, abs(want))
		}
	}
}

func TestFileSystemGlobThroughBinding(t *testing.T) {
	root := t.TempDir()
	for _, name := range []string{"a.txt", "b.txt", "c.md"} {
		fsTestWrite(t, filepath.Join(root, name), name)
	}
	fsTestNode(func(fb *Fiber) {
		got := List(fsTestCall(fb, "glob", Rec("root", Some(root), "exclude", []Value{"b.txt"}), "*.txt"))
		if len(got) != 1 || got[0] != "a.txt" {
			t.Errorf("glob: got %v", got)
		}
	})
}
