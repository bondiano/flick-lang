package flickrt

import (
	"os"
	"path/filepath"
	"reflect"
	"runtime"
	"slices"
	"strings"
	"testing"
)

func fsTestGlob(t *testing.T, fb *Fiber, root, pattern string, excludes []Value, want ...string) {
	t.Helper()
	got := []string{}
	for _, value := range List(fsTestCall(fb, "glob", Rec("root", Some(root), "exclude", excludes), pattern)) {
		got = append(got, filepath.ToSlash(value.(string)))
	}
	slices.Sort(got)
	slices.Sort(want)
	if len(got) != len(want) || !slices.Equal(got, want) {
		t.Errorf("glob %q excluding %v: got %q, want %q", pattern, excludes, got, want)
	}
}

func TestFileSystemGlobGrammarAndRoots(t *testing.T) {
	root := t.TempDir()
	files := []string{
		"root.txt", ".hidden.txt", "file[.txt", "unclosed{.txt", "#notes", "!literal",
		"letters/a.txt", "letters/b.txt", "letters/c.txt", "letters/é.txt",
		"nums/1.txt", "nums/2.txt", "nums/3.txt", "nums/01.txt", "nums/02.txt", "nums/03.txt",
		"words/one.txt", "words/two.txt", "words/three.txt", "words/a.txt", "words/aa.txt", "words/ab.txt", "words/ba.txt", "words/b.txt",
		"sub/nested.txt", "sub/.inside.txt", "sub/deep/z.txt", ".secret/x.txt", "case/UPPER.TXT",
		"a.txt", "astral/😀.txt", "astral/𐐀😀.txt", "{x}1", "{x}{1..2}", "ς.txt", "ſ.txt", "aẞ.txt", "z.txt", "ıa.txt", "x.σ", "x.ς", "xx.σ", "xx.ς",
	}
	for _, file := range files {
		path := filepath.Join(root, filepath.FromSlash(file))
		if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
			t.Fatal(err)
		}
		fsTestWrite(t, path, file)
	}
	fsTestNode(func(fb *Fiber) {
		cases := []struct {
			pattern string
			want    []string
		}{
			{"letters/?.txt", []string{"letters/a.txt", "letters/b.txt", "letters/c.txt", "letters/é.txt"}},
			{"letters/[ab].txt", []string{"letters/a.txt", "letters/b.txt"}},
			{"letters/[!ab].txt", []string{"letters/c.txt", "letters/é.txt"}},
			{"letters/[[:alpha:]].txt", []string{"letters/a.txt", "letters/b.txt", "letters/c.txt", "letters/é.txt"}},
			{"letters/{a,b}.txt", []string{"letters/a.txt", "letters/b.txt"}},
			{"letters/{a..c}.txt", []string{"letters/a.txt", "letters/b.txt", "letters/c.txt"}},
			{"letters/{c..a..2}.txt", []string{"letters/a.txt", "letters/c.txt"}},
			{"letters/{a,{b,c}}.txt", []string{"letters/a.txt", "letters/b.txt", "letters/c.txt"}},
			{"nums/{1..3..2}.txt", []string{"nums/1.txt", "nums/3.txt"}},
			{"nums/{03..01}.txt", []string{"nums/01.txt", "nums/02.txt", "nums/03.txt"}},
			{"words/@(one|two).txt", []string{"words/one.txt", "words/two.txt"}},
			{"words/?(one|two).txt", []string{"words/one.txt", "words/two.txt"}},
			{"words/+(a|b).txt", []string{"words/a.txt", "words/aa.txt", "words/ab.txt", "words/ba.txt", "words/b.txt"}},
			{"words/*(a|b).txt", []string{"words/a.txt", "words/aa.txt", "words/ab.txt", "words/ba.txt", "words/b.txt"}},
			{"words/!(one|two).txt", []string{"words/three.txt", "words/a.txt", "words/aa.txt", "words/ab.txt", "words/ba.txt", "words/b.txt"}},
			{"words/@(+(a)|b).txt", []string{"words/a.txt", "words/aa.txt", "words/b.txt"}},
			{"words/!(a)*.txt", []string{"words/b.txt", "words/ba.txt", "words/one.txt", "words/two.txt", "words/three.txt"}},
			{"sub/**/*.txt", []string{"sub/nested.txt", "sub/deep/z.txt"}},
			{"**/.inside.txt", []string{"sub/.inside.txt"}},
			{"**/.secret/*.txt", []string{".secret/x.txt"}},
			{"@(a|).txt", []string{"a.txt"}},
			{"{x}{1..2}", []string{"{x}{1..2}"}},
			{".secret/*.txt", []string{".secret/x.txt"}},
			{"file[.txt", []string{"file[.txt"}},
			{"unclosed{.txt", []string{"unclosed{.txt"}},
			{"#notes", []string{"#notes"}},
			{"!literal", []string{"!literal"}},
			{"letters\\[ab].txt", []string{"letters/a.txt", "letters/b.txt"}},
			{"astral/?.txt", nil},
			{"astral/??.txt", []string{"astral/😀.txt"}},
			{"astral/[[:alpha:]]?.txt", []string{"astral/𐐀😀.txt"}},
			{"letters/", []string{"letters"}},
			{"root.txt/", nil},
			{"not-present/*", nil},
			{"", nil},
		}
		for _, test := range cases {
			fsTestGlob(t, fb, root, test.pattern, nil, test.want...)
		}
		fsTestGlob(t, fb, root, "sub/**", nil, "sub", "sub/deep", "sub/deep/z.txt", "sub/nested.txt")
		fsTestGlob(t, fb, root, "{letters,nums}/{a,1}.txt", nil, "letters/a.txt", "nums/1.txt")
		fsTestGlob(t, fb, root, "sub/**/*.txt", []Value{"sub/deep/**"}, "sub/nested.txt")
		fsTestGlob(t, fb, root, "letters/*.txt", []Value{"**/[ab].txt"}, "letters/c.txt", "letters/é.txt")
		absolute := filepath.ToSlash(filepath.Join(root, "letters", "[ab].txt"))
		fsTestGlob(t, fb, root, absolute, nil, filepath.ToSlash(filepath.Join(root, "letters", "a.txt")), filepath.ToSlash(filepath.Join(root, "letters", "b.txt")))
		fsTestGlob(t, fb, root, "sub", []Value{"sub/**"}, "sub")
		if runtime.GOOS == "linux" {
			fsTestGlob(t, fb, root, "case/*.txt", nil)
			fsTestGlob(t, fb, root, "σ*.txt", nil)
			fsTestGlob(t, fb, root, "s*.txt", nil)
			fsTestGlob(t, fb, root, "[[:alpha:]]ß*.txt", nil)
			fsTestGlob(t, fb, root, "[Z-a].txt", nil, "a.txt")
		} else {
			fsTestGlob(t, fb, root, "case/*.txt", nil, "case/UPPER.TXT")
			fsTestGlob(t, fb, root, "σ*.txt", nil, "ς.txt")
			fsTestGlob(t, fb, root, "s*.txt", nil)
			fsTestGlob(t, fb, root, "[[:alpha:]]ß*.txt", nil, "aẞ.txt")
			fsTestGlob(t, fb, root, "[Z-a].txt", nil, "a.txt", "z.txt")
		}
		// dotless i matches no fold in [a-z]; aẞ.txt still matches via alpha.
		fsTestGlob(t, fb, root, "[a-z][[:alpha:]].txt", nil, "aẞ.txt")
		// native fast suffix tests lowercase, they do not canonicalize σ/ς.
		fsTestGlob(t, fb, root, "*.σ", nil, "x.σ", "xx.σ")
		fsTestGlob(t, fb, root, "??.σ", nil, "xx.σ")
		fsTestFailure(t, func() Value {
			return fsTestCall(fb, "glob", Rec("root", Some(root), "exclude", []Value{}), strings.Repeat("x", 65537))
		}, "Bad")
	})
}

func TestFileSystemGlobDoesNotRecurseThroughImplicitSymlinks(t *testing.T) {
	root := t.TempDir()
	if err := os.Mkdir(filepath.Join(root, "real"), 0o700); err != nil {
		t.Fatal(err)
	}
	fsTestWrite(t, filepath.Join(root, "real", "a.txt"), "a")
	fsTestWrite(t, filepath.Join(root, "target"), "")
	fsTestSymlink(t, "real", filepath.Join(root, "alias"))
	fsTestSymlink(t, "missing", filepath.Join(root, "broken"))
	fsTestSymlink(t, "..", filepath.Join(root, "real", "cycle"))
	fsTestNode(func(fb *Fiber) {
		fsTestGlob(t, fb, root, "**/*.txt", nil, "real/a.txt")
		fsTestGlob(t, fb, root, "*/*.txt", nil, "real/a.txt")
		fsTestGlob(t, fb, root, "alias/*.txt", nil, "alias/a.txt")
		fsTestGlob(t, fb, root, "alias/**/*.txt", nil, "alias/a.txt")
		fsTestGlob(t, fb, root, "**/alias/*.txt", nil)
		fsTestGlob(t, fb, root, "./**/*.txt", nil, "alias/a.txt", "real/a.txt")
		fsTestGlob(t, fb, root, "foo/../**/*.txt", nil, "alias/a.txt", "real/a.txt")
		fsTestGlob(t, fb, root, "*/../target", nil, "target")
		fsTestGlob(t, fb, root, "broken", nil, "broken")
		fsTestGlob(t, fb, root, "**", nil, ".", "alias", "broken", "real", "real/a.txt", "real/cycle", "target")
		fsTestGlob(t, fb, root, "**", []Value{"**"}, ".")
		fsTestGlob(t, fb, root, "real", []Value{"real/**"}, "real")
	})
}

func TestFileSystemGlobAbsoluteExcludeDropsTrailingSlash(t *testing.T) {
	root := t.TempDir()
	fsTestWrite(t, filepath.Join(root, "a.txt"), "a")
	fsTestWrite(t, filepath.Join(root, "b.txt"), "b")
	excluded := filepath.ToSlash(filepath.Join(root, "a.txt")) + "/"
	fsTestNode(func(fb *Fiber) {
		fsTestGlob(t, fb, root, "*.txt", []Value{excluded}, "b.txt")
	})
}

func TestFileSystemGlobSeparatorRootedPatternOnWindows(t *testing.T) {
	if runtime.GOOS != "windows" {
		t.Skip("separator-rooted patterns are Windows win32.resolve semantics")
	}
	patterns := fsGlobCompile("/target/*.txt", `C:\work`)
	separator := string(filepath.Separator)
	if len(patterns) != 1 || patterns[0].base != "C:"+separator || patterns[0].output != separator {
		t.Fatalf("rooted pattern compiled as %+v", patterns)
	}
}

func TestFileSystemGlobSymlinkAfterGlobstarNeedsRealDirectory(t *testing.T) {
	root := t.TempDir()
	if err := os.MkdirAll(filepath.Join(root, "sub", "real"), 0o700); err != nil {
		t.Fatal(err)
	}
	fsTestWrite(t, filepath.Join(root, "sub", "real", "a.txt"), "a")
	fsTestSymlink(t, "real", filepath.Join(root, "sub", "alias"))
	fsTestNode(func(fb *Fiber) {
		fsTestGlob(t, fb, root, "sub/**/alias/*.txt", nil)
		fsTestGlob(t, fb, root, "sub/**/real/*.txt", nil, "sub/real/a.txt")
		fsTestGlob(t, fb, root, "sub/alias/*.txt", nil, "sub/alias/a.txt")
	})
}

func TestFileSystemGlobExtglobDotAndLiteralRules(t *testing.T) {
	root := t.TempDir()
	literals := []string{"@()", "+()"}
	if runtime.GOOS != "windows" {
		literals = append(literals, "*()", "?()")
	}
	visible := append([]string{"x", "xx", "x.y", "a", "ab", "b", "c"}, literals...)
	for _, name := range append([]string{".x", ".xy", ".yx"}, visible...) {
		fsTestWrite(t, filepath.Join(root, name), name)
	}
	fsTestNode(func(fb *Fiber) {
		fsTestGlob(t, fb, root, "+(*|.x*)", nil, append([]string{".x", ".xy"}, visible...)...)
		fsTestGlob(t, fb, root, "!(@(a))", nil, visible...)
		fsTestGlob(t, fb, root, "!(a|@(b))", nil, visible...)
		fsTestGlob(t, fb, root, "!(!(a))", nil, "a")
		fsTestGlob(t, fb, root, "!(@(!(a)))", nil, "a")
		fsTestGlob(t, fb, root, "!(!(@(a)))", nil, "a")
		fsTestGlob(t, fb, root, "!(@(@(a)))", nil, visible...)
		fsTestGlob(t, fb, root, "@(@(@(@(a))))", nil, "a")
		deep := "!(" + strings.Repeat("@(", 40) + "a" + strings.Repeat(")", 40) + ")"
		fsTestGlob(t, fb, root, deep, nil, visible...)
		for _, pattern := range literals {
			fsTestGlob(t, fb, root, pattern, nil, pattern)
		}
		fsTestGlob(t, fb, root, "[.]x", nil, ".x")
		fsTestGlob(t, fb, root, "[z-a]", nil)
		fsTestGlob(t, fb, root, "@(?(a))b", nil, "ab", "b")
		fsTestGlob(t, fb, root, "[!z-a]", nil)
		fsTestGlob(t, fb, root, ".*", []Value{"**"}, ".x", ".xy", ".yx")
		fsTestGlob(t, fb, root, "*", []Value{"**"})
	})
}

func TestFileSystemGlobBraceCapPrecedesDeduplication(t *testing.T) {
	root := t.TempDir()
	fsTestWrite(t, filepath.Join(root, "b1"), "")
	fsTestNode(func(fb *Fiber) {
		// The raw expansion cap fills before `b` is reached; native returns [].
		fsTestGlob(t, fb, root, "{a,a,b}{1..50000}", nil)
	})
}

func TestFileSystemBraceExpansionUsesNativeRanges(t *testing.T) {
	for pattern, want := range map[string][]string{
		"{a,b}{1,2}":  {"a1", "a2", "b1", "b2"},
		"{-02..2..2}": {"-02", "000", "002"},
		"{-1..1..01}": {"-1", "00", "01"},
		"${a,b}":      {"${a,b}"},
		"{a,b}":       {"a", "b"},
		"{a,a}":       {"a", "a"},
		"{**,**}":     {"**", "**"},
		"{}{1..2}":    {"{}1", "{}2"},
		"${x}{1..2}":  {"${x}1", "${x}2"},
		"{a{1..2}}":   {"{a{1..2}}"},
		"{x}{a,b}":    {"{x}a", "{x}b"},
		"{{a,b}}":     {"{a}", "{b}"},
	} {
		if got := fsGlobBraces(pattern, true); !reflect.DeepEqual(got, want) {
			t.Errorf("brace %q: got %q, want %q", pattern, got, want)
		}
	}
}
