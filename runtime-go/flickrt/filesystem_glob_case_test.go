package flickrt

import (
	"os"
	"path/filepath"
	"runtime"
	"testing"
)

func TestFileSystemGlobFullUnicodeFastCase(t *testing.T) {
	root := t.TempDir()
	fsTestWrite(t, filepath.Join(root, "x.𐐀"), "")
	fsTestWrite(t, filepath.Join(root, "y.İ"), "")
	nocase := runtime.GOOS == "windows" || runtime.GOOS == "darwin"
	fsTestNode(func(fb *Fiber) {
		want := []string(nil)
		if nocase {
			want = []string{"x.𐐀"}
		}
		// Deseret lowercases across the surrogate pair; dotted I expands to
		// i + U+0307, so `*.i` must not match.
		fsTestGlob(t, fb, root, "*.𐐨", nil, want...)
		fsTestGlob(t, fb, root, "*.i", nil)
	})
}

func TestFileSystemGlobFullUppercaseCanonicalize(t *testing.T) {
	root := t.TempDir()
	fsTestWrite(t, filepath.Join(root, "ᾈ.txt"), "")
	fsTestNode(func(fb *Fiber) {
		// Both full-uppercase to a two-code-point sequence; canonical keeps the
		// originals distinct on every host.
		fsTestGlob(t, fb, root, "ᾀ*.txt", nil)
	})
}

func TestFileSystemGlobMalformedBraceUsesLeftmostPair(t *testing.T) {
	root := t.TempDir()
	fsTestWrite(t, filepath.Join(root, "{abd"), "")
	fsTestNode(func(fb *Fiber) {
		fsTestGlob(t, fb, root, "{a{b,c}{d,e}", nil, "{abd")
	})
}

func TestFileSystemGlobExcludeDirectoryDescent(t *testing.T) {
	root := t.TempDir()
	if err := os.Mkdir(filepath.Join(root, "sub"), 0o700); err != nil {
		t.Fatal(err)
	}
	for _, name := range []string{".x", "visible", "sub/.deep", "sub/file"} {
		fsTestWrite(t, filepath.Join(root, name), "")
	}
	fsTestNode(func(fb *Fiber) {
		fsTestGlob(t, fb, root, ".*", []Value{"**"}, ".x")
		fsTestGlob(t, fb, root, ".*", []Value{filepath.Join(root, "**")}, ".x")
		fsTestGlob(t, fb, root, "sub", []Value{"sub/**"}, "sub")
		fsTestGlob(t, fb, root, "sub/.*", []Value{"sub/**"})
		fsTestGlob(t, fb, root, "**", []Value{"sub/**"}, ".", "visible")
		fsTestGlob(t, fb, root, "**/.*", []Value{"sub/**"}, ".x")
	})
}

func TestFileSystemGlobUnicodePosixClasses(t *testing.T) {
	for _, test := range []struct {
		pattern string
		name    string
		nocase  bool
		want    bool
	}{
		{"[[:print:]]", "\u0378", true, true},
		{"[[:print:]]", "\ue000", true, true},
		{"[[:print:]]", "a", true, false},
		{"[[:graph:]]", "\u0378", true, false},
		{"[[:lower:]]", "\u01c5", true, true},
		{"[[:upper:]]", "\u01c5", true, true},
		{"[[:alpha:]]", "\u0345", true, true},
		{"[[:alpha:]]", "\u0345", false, false},
		{"[[:ascii:]]", "\u212a", true, false},
		{"[[:ascii:][:digit:]]", "\u212a", true, true},
	} {
		part := fsGlobCompilePart(test.pattern)
		got := fsGlobMatchTokens(part.tokens, test.name, 0, len(test.name), test.nocase, part.unicode)
		if got != test.want {
			t.Errorf("%q matching %q (nocase=%v): got %v, want %v", test.pattern, test.name, test.nocase, got, test.want)
		}
	}
}
