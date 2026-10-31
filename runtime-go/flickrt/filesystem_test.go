package flickrt

import (
	"errors"
	"io"
	"math"
	"os"
	"path/filepath"
	"runtime"
	"testing"
)

func fsTestCall(fb *Fiber, name string, args ...Value) Value {
	op := Operation("test.FileSystem", name, len(args))
	for _, arg := range args {
		op = Apply(fb, op, arg)
	}
	return Run(fb, op)
}

func fsTestNode(body func(*Fiber)) {
	Provide(rootFiber(), FileSystemNode("test.FileSystem"), Eff(func(fb *Fiber) Value {
		body(fb)
		return Unit
	}))
}

func fsTestFailure(t *testing.T, call func() Value, want string) *Record {
	t.Helper()
	_, cause := catch(call)
	if cause == nil || cause.kind != causeFail {
		t.Fatalf("expected typed PlatformError, got %v", cause)
	}
	errorValue, ok := cause.err.(*Data)
	if !ok || errorValue.Tag != platformErrorTag {
		t.Fatalf("expected PlatformError, got %v", cause.err)
	}
	reason := errorValue.Fields[0].(*Data)
	fields := record(reason.Fields[0])
	if want == "Bad" {
		if reason.Tag != Tag("PlatformError.Reason.Bad") {
			t.Fatalf("expected BadArgument, got %v", reason)
		}
	} else if reason.Tag != systemReasonTag || fields.Get("tag").(*Data).Tag != Tag("PlatformError.SystemErrorTag."+want) {
		t.Fatalf("expected System.%s, got %v", want, reason)
	}
	return fields
}

func fsTestWrite(t *testing.T, path, text string) {
	t.Helper()
	if err := os.WriteFile(path, []byte(text), 0o600); err != nil {
		t.Fatal(err)
	}
}

func fsTestText(t *testing.T, path, want string) {
	t.Helper()
	got, err := os.ReadFile(path)
	if err != nil || string(got) != want {
		t.Fatalf("%s: got %q, %v, want %q", path, got, err, want)
	}
}

func fsTestSymlink(t *testing.T, target, path string) {
	t.Helper()
	if err := os.Symlink(target, path); err != nil {
		if runtime.GOOS == "windows" {
			t.Skipf("host does not permit symlinks: %v", err)
		}
		t.Fatal(err)
	}
}

func TestFileSystemNodePathOperations(t *testing.T) {
	dir := t.TempDir()
	a, b, c := filepath.Join(dir, "a"), filepath.Join(dir, "b"), filepath.Join(dir, "c")
	fsTestWrite(t, a, "hello")
	fsTestNode(func(fb *Fiber) {
		fsTestCall(fb, "access", Rec("ok", true, "readable", true, "writable", true), a)
		fsTestFailure(t, func() Value {
			return fsTestCall(fb, "access", Rec("ok", false, "readable", false, "writable", false), a+"-missing")
		}, "NotFound")
		fsTestCall(fb, "copy_file", a, b)
		fsTestText(t, b, "hello")
		fsTestWrite(t, b, "old")
		fsTestCall(fb, "copy_file", a, b)
		fsTestText(t, b, "hello")
		fsTestCall(fb, "copy", Rec("overwrite", false, "preserveTimestamps", false), a, c)
		fsTestWrite(t, c, "retained")
		fsTestCall(fb, "copy", Rec("overwrite", false, "preserveTimestamps", false), a, c)
		fsTestText(t, c, "retained")
		fsTestCall(fb, "copy", Rec("overwrite", true, "preserveTimestamps", false), a, c)
		fsTestText(t, c, "hello")
		fsTestCall(fb, "chmod", int32(0o600), c)
		info, err := os.Stat(c)
		if err != nil || runtime.GOOS != "windows" && info.Mode().Perm() != 0o600 {
			t.Fatalf("chmod: %v, %v", info, err)
		}
		if runtime.GOOS == "windows" {
			fsTestFailure(t, func() Value { return fsTestCall(fb, "chown", float64(0), float64(0), c) }, "UnknownError")
		} else {
			fsTestCall(fb, "chown", float64(os.Getuid()), float64(os.Getgid()), c)
		}
		hard := filepath.Join(dir, "hard")
		fsTestCall(fb, "link", a, hard)
		fsTestWrite(t, a, "changed")
		fsTestText(t, hard, "changed")
		fsTestFailure(t, func() Value { return fsTestCall(fb, "link", a, hard) }, "AlreadyExists")
		canonical, err := filepath.EvalSymlinks(a)
		if err != nil {
			t.Fatal(err)
		}
		if got := fsTestCall(fb, "real_path", a); got != canonical {
			t.Fatalf("real_path: got %v, want %s", got, canonical)
		}
		fsTestCall(fb, "truncate", Some(float64(2)), a)
		fsTestText(t, a, "ch")
		fsTestCall(fb, "truncate", Some(float64(4)), a)
		fsTestText(t, a, "ch\x00\x00")
		fsTestCall(fb, "truncate", Some(float64(-1)), a)
		fsTestText(t, a, "")
		fsTestWrite(t, a, "again")
		fsTestCall(fb, "truncate", None, a)
		fsTestText(t, a, "")
		fsTestCall(fb, "utimes", float64(1000.9), float64(2000.9), a)
		stat := record(fsTestCall(fb, "stat", a))
		atime, _ := some(stat.Get("atime"))
		mtime, _ := some(stat.Get("mtime"))
		if atime != float64(1000) || mtime != float64(2000) {
			t.Fatalf("utimes milliseconds: atime=%v mtime=%v", atime, mtime)
		}
		fsTestCall(fb, "utimes", float64(-1000.9), float64(-2000.9), a)
		stat = record(fsTestCall(fb, "stat", a))
		mtime, _ = some(stat.Get("mtime"))
		if mtime != float64(-2000) {
			t.Fatalf("negative Date milliseconds: %v", mtime)
		}
	})
}

func TestFileSystemNodeSymlinks(t *testing.T) {
	dir := t.TempDir()
	fsTestWrite(t, filepath.Join(dir, "source"), "content")
	fsTestSymlink(t, "source", filepath.Join(dir, "probe"))
	fsTestNode(func(fb *Fiber) {
		link := filepath.Join(dir, "link")
		fsTestCall(fb, "symlink", "source", link)
		if got := fsTestCall(fb, "read_link", link); got != "source" {
			t.Fatalf("read_link: got %v", got)
		}
		if got := fsTestCall(fb, "real_path", link); got != fsTestCall(fb, "real_path", filepath.Join(dir, "source")) {
			t.Fatalf("symlink real_path: %v", got)
		}
		fsTestFailure(t, func() Value { return fsTestCall(fb, "symlink", "source", link) }, "AlreadyExists")
	})
}

func TestFileSystemOpenIsScopedAndHonorsFlags(t *testing.T) {
	path := filepath.Join(t.TempDir(), "file")
	var handle *os.File
	fsTestNode(func(fb *Fiber) {
		owned(fb, func(inner *Fiber, _ *Owner) Value {
			handle = fsTestCall(inner, "open", Rec("flag", Some(Nullary(Tag("FileSystem.OpenFlag.WriteRead"))), "mode", Some(int32(0o600)), "noFollow", false), path).(*fsFile).f
			if _, err := handle.WriteString("scoped"); err != nil {
				t.Fatal(err)
			}
			if _, err := handle.Seek(0, io.SeekStart); err != nil {
				t.Fatal(err)
			}
			text, err := io.ReadAll(handle)
			if err != nil || string(text) != "scoped" {
				t.Fatalf("opened read/write handle: %q, %v", text, err)
			}
			return Unit
		})
		if _, err := handle.Read(make([]byte, 1)); !errors.Is(err, os.ErrClosed) {
			t.Fatalf("handle remains open: %v", err)
		}
		_, cause := catch(func() Value {
			return owned(fb, func(inner *Fiber, _ *Owner) Value {
				handle = fsTestCall(inner, "open", Rec("flag", None, "mode", None, "noFollow", false), path).(*fsFile).f
				return Fail("body failed")
			})
		})
		if cause == nil || !cause.has(causeFail) {
			t.Fatalf("body failure lost: %v", cause)
		}
		if _, err := handle.Read(make([]byte, 1)); !errors.Is(err, os.ErrClosed) {
			t.Fatalf("failed scope leaked handle: %v", err)
		}
		fsTestFailure(t, func() Value {
			return fsTestCall(fb, "open", Rec("flag", Some(Nullary(Tag("FileSystem.OpenFlag.WriteExclusive"))), "mode", None, "noFollow", false), path)
		}, "AlreadyExists")
	})
}

func TestFileSystemOpenNoFollow(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "file")
	fsTestWrite(t, path, "intact")
	options := Rec("flag", None, "mode", None, "noFollow", true)
	if runtime.GOOS == "windows" {
		fsTestNode(func(fb *Fiber) {
			err := fsTestFailure(t, func() Value { return fsTestCall(fb, "open", options, path) }, "Bad")
			description, _ := some(err.Get("description"))
			if description != "noFollow is not supported on this platform" {
				t.Fatalf("Windows noFollow contract: %v", description)
			}
		})
		return
	}
	fsTestSymlink(t, "file", filepath.Join(dir, "link"))
	fsTestSymlink(t, dir, filepath.Join(dir, "parent-link"))
	fsTestNode(func(fb *Fiber) {
		fsTestFailure(t, func() Value { return fsTestCall(fb, "open", options, filepath.Join(dir, "link")) }, "BadResource")
		owned(fb, func(inner *Fiber, _ *Owner) Value {
			file := fsTestCall(inner, "open", options, filepath.Join(dir, "parent-link", "file")).(*fsFile).f
			contents, err := io.ReadAll(file)
			if err != nil || string(contents) != "intact" {
				t.Fatalf("noFollow must allow symlinked parents: %q, %v", contents, err)
			}
			return Unit
		})
	})
}

func TestFileSystemCopyDoesNotDestroyItsSource(t *testing.T) {
	dir := t.TempDir()
	src, hard := filepath.Join(dir, "source"), filepath.Join(dir, "hard")
	fsTestWrite(t, src, "precious")
	if err := os.Link(src, hard); err != nil {
		t.Fatal(err)
	}
	fsTestNode(func(fb *Fiber) {
		for _, dest := range []string{src, hard} {
			fsTestCall(fb, "copy_file", src, dest)
			fsTestText(t, src, "precious")
			fsTestFailure(t, func() Value {
				return fsTestCall(fb, "copy", Rec("overwrite", true, "preserveTimestamps", false), src, dest)
			}, "UnknownError")
			fsTestText(t, src, "precious")
		}
		tree := filepath.Join(dir, "tree")
		if err := os.Mkdir(tree, 0o700); err != nil {
			t.Fatal(err)
		}
		fsTestWrite(t, filepath.Join(tree, "child"), "kept")
		fsTestFailure(t, func() Value {
			return fsTestCall(fb, "copy", Rec("overwrite", true, "preserveTimestamps", false), tree, filepath.Join(tree, "nested", "copy"))
		}, "UnknownError")
		if _, err := os.Stat(filepath.Join(tree, "nested")); !errors.Is(err, os.ErrNotExist) {
			t.Fatalf("descendant rejection mutated source: %v", err)
		}
		fsTestFailure(t, func() Value {
			return fsTestCall(fb, "copy", Rec("overwrite", true, "preserveTimestamps", false), tree, src)
		}, "BadResource")
		fsTestText(t, src, "precious")
	})
}

func TestFileSystemCopySymlinkSafetyAndTimestamps(t *testing.T) {
	dir := t.TempDir()
	source, dest := filepath.Join(dir, "source"), filepath.Join(dir, "dest")
	if err := os.Mkdir(source, 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.Mkdir(filepath.Join(source, "sub"), 0o700); err != nil {
		t.Fatal(err)
	}
	file := filepath.Join(source, "file")
	fsTestWrite(t, file, "bytes")
	fsTestSymlink(t, "file", filepath.Join(source, "link"))
	fsTestSymlink(t, source, filepath.Join(dir, "alias"))
	fsTestSymlink(t, filepath.Join(source, "sub"), filepath.Join(dir, "sub-alias"))
	fsTestSymlink(t, file, filepath.Join(dir, "file-link"))
	fsTestNode(func(fb *Fiber) {
		fsTestCall(fb, "copy_file", file, filepath.Join(dir, "file-link"))
		fsTestText(t, file, "bytes")
		fsTestFailure(t, func() Value {
			return fsTestCall(fb, "copy", Rec("overwrite", true, "preserveTimestamps", false), source, filepath.Join(dir, "alias", "copy"))
		}, "UnknownError")
		fsTestFailure(t, func() Value {
			return fsTestCall(fb, "copy", Rec("overwrite", true, "preserveTimestamps", false), source, filepath.Join(dir, "sub-alias", "new"))
		}, "UnknownError")
		if _, err := os.Stat(filepath.Join(source, "sub", "new")); !errors.Is(err, os.ErrNotExist) {
			t.Fatalf("copy through a symlink to source/sub created an entry in the source: %v", err)
		}
		fsTestCall(fb, "utimes", float64(1000), float64(2000), file)
		fsTestCall(fb, "copy", Rec("overwrite", true, "preserveTimestamps", true), source, dest)
		fsTestText(t, filepath.Join(dest, "file"), "bytes")
		copied, err := os.Stat(filepath.Join(dest, "file"))
		if err != nil || copied.ModTime().UnixMilli() != 2000 {
			t.Fatalf("copy did not preserve mtime: %v, %v", copied, err)
		}
		target, err := os.Readlink(filepath.Join(dest, "link"))
		if err != nil || target != file {
			t.Fatalf("copy must resolve relative symlink targets: %q, %v", target, err)
		}
	})
}

func TestFileSystemInvalidNumbersAndPathsCannotMutateFiles(t *testing.T) {
	path := filepath.Join(t.TempDir(), "file")
	fsTestWrite(t, path, "keep")
	fsTestNode(func(fb *Fiber) {
		for _, n := range []float64{math.NaN(), math.Inf(1), math.Inf(-1), 1.5, 1 << 53} {
			fsTestFailure(t, func() Value { return fsTestCall(fb, "truncate", Some(n), path) }, "Bad")
		}
		for _, n := range []float64{math.NaN(), math.Inf(1), -2, 0.5, 1 << 32} {
			fsTestFailure(t, func() Value { return fsTestCall(fb, "chown", n, float64(0), path) }, "Bad")
		}
		for _, n := range []float64{math.NaN(), math.Inf(1), 8.64e15 + 1} {
			fsTestFailure(t, func() Value { return fsTestCall(fb, "utimes", n, float64(0), path) }, "UnknownError")
		}
		fsTestFailure(t, func() Value { return fsTestCall(fb, "chmod", int32(-1), path) }, "Bad")
		fsTestFailure(t, func() Value {
			return fsTestCall(fb, "access", Rec("ok", true, "readable", true, "writable", false), path+"\x00")
		}, "Bad")
		for _, name := range []string{"read_link", "real_path"} {
			fsTestFailure(t, func() Value { return fsTestCall(fb, name, path+"-missing") }, "NotFound")
		}
		fsTestText(t, path, "keep")
	})
}

func TestFileSystemCopyOfSizeZeroDeviceIsBounded(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("/dev/zero is a POSIX device")
	}
	zero := "/dev/zero"
	info, err := os.Stat(zero)
	if err != nil || info.Size() != 0 {
		t.Skip("the host has no size-zero /dev/zero")
	}
	dest := filepath.Join(t.TempDir(), "out")
	fsTestNode(func(fb *Fiber) {
		fsTestCall(fb, "copy_file", zero, dest)
		copied, err := os.Stat(dest)
		if err != nil || copied.Size() != 0 {
			t.Fatalf("a size-zero device copy must stay bounded: %v, %v", copied, err)
		}
	})
}

func TestFileSystemCopyPreservesAccessAndModificationTimes(t *testing.T) {
	dir := t.TempDir()
	source, dest := filepath.Join(dir, "source"), filepath.Join(dir, "dest")
	fsTestWrite(t, source, "read-only source")
	if err := os.Chmod(source, 0o400); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { os.Chmod(source, 0o600); os.Chmod(dest, 0o600) })
	fsTestNode(func(fb *Fiber) {
		fsTestCall(fb, "utimes", float64(1000), float64(2000), source)
		fsTestCall(fb, "copy", Rec("overwrite", false, "preserveTimestamps", true), source, dest)
		original, err := os.Stat(source)
		if err != nil {
			t.Fatal(err)
		}
		copied, err := os.Stat(dest)
		if err != nil {
			t.Fatal(err)
		}
		if fsAccessTime(original).UnixMilli() != fsAccessTime(copied).UnixMilli() || !original.ModTime().Equal(copied.ModTime()) {
			t.Fatalf("copy timestamps differ: source %v/%v, dest %v/%v", fsAccessTime(original), original.ModTime(), fsAccessTime(copied), copied.ModTime())
		}
		if copied.Mode().Perm()&0o200 != 0 {
			t.Fatalf("copy left its temporary writable mode: %v", copied.Mode())
		}
	})
}

func TestFileSystemAccessChecksPermissions(t *testing.T) {
	path := filepath.Join(t.TempDir(), "file")
	fsTestWrite(t, path, "read-only")
	if runtime.GOOS != "windows" && os.Geteuid() == 0 {
		t.Skip("root may access files regardless of their mode")
	}
	if err := os.Chmod(path, 0o400); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { os.Chmod(path, 0o600) })
	fsTestNode(func(fb *Fiber) {
		fsTestCall(fb, "access", Rec("ok", false, "readable", true, "writable", false), path)
		reason := "PermissionDenied"
		if runtime.GOOS == "windows" {
			reason = "UnknownError"
		}
		fsTestFailure(t, func() Value {
			return fsTestCall(fb, "access", Rec("ok", false, "readable", false, "writable", true), path)
		}, reason)
	})
}
