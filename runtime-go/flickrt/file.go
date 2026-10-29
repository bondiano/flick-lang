package flickrt

import (
	"errors"
	"io"
	"io/fs"
	"os"
	"strings"
	"syscall"
	"unicode/utf8"
)

// The file system behind `File.node`: a failure is a typed error with the
// message Node gives it, and an opened file is closed by the owner of its
// scope.

var (
	openErrorTag  = Tag("File.OpenError.OpenError")
	readErrorTag  = Tag("File.ReadError.ReadError")
	writeErrorTag = Tag("File.WriteError.WriteError")
)

// nodeErrors are the code and text Node gives an errno.
//
// shortcut: only the common errnos read as Node's; others keep Go's text, extend the table when a test shows one
var nodeErrors = map[syscall.Errno][2]string{
	syscall.ENOENT:  {"ENOENT", "no such file or directory"},
	syscall.EACCES:  {"EACCES", "permission denied"},
	syscall.EISDIR:  {"EISDIR", "illegal operation on a directory"},
	syscall.ENOTDIR: {"ENOTDIR", "not a directory"},
	syscall.EEXIST:  {"EEXIST", "file already exists"},
	syscall.EBADF:   {"EBADF", "bad file descriptor"},
}

// nodeMessage is the message of a file system error as Node writes it:
// `ENOENT: no such file or directory, open '/x'`.
func nodeMessage(err error, call, path string) string {
	var errno syscall.Errno
	if errors.As(err, &errno) {
		if e, ok := nodeErrors[errno]; ok {
			if path == "" {
				return e[0] + ": " + e[1] + ", " + call
			}
			return e[0] + ": " + e[1] + ", " + call + " '" + path + "'"
		}
	}
	var pe *fs.PathError
	if errors.As(err, &pe) {
		return pe.Err.Error()
	}
	return err.Error()
}

// decodeUTF8 reads bytes as Node's `utf8` decoding does: each byte of an
// invalid sequence is U+FFFD.
func decodeUTF8(b []byte) string {
	if utf8.Valid(b) {
		return string(b)
	}
	var out strings.Builder
	for len(b) > 0 {
		r, size := utf8.DecodeRune(b)
		out.WriteRune(r)
		b = b[size:]
	}
	return out.String()
}

func FileOpen(fb *Fiber, path Value) Value {
	acquire := Eff(func(*Fiber) Value {
		f, err := os.Open(path.(string))
		if err != nil {
			return Fail(NewData(openErrorTag, nodeMessage(err, "open", path.(string))))
		}
		return f
	})
	release := Fn(func(f Value) Value {
		return Eff(func(*Fiber) Value {
			f.(*os.File).Close()
			return Unit
		})
	})
	return AcquireRelease(fb, acquire, release)
}

func FileReadHandle(_ *Fiber, f Value) Value {
	b, err := io.ReadAll(f.(*os.File))
	if err != nil {
		return Fail(NewData(readErrorTag, nodeMessage(err, "read", "")))
	}
	return decodeUTF8(b)
}

func FileRead(_ *Fiber, path Value) Value {
	b, err := os.ReadFile(path.(string))
	if err != nil {
		return Fail(NewData(readErrorTag, nodeMessage(err, "open", path.(string))))
	}
	return decodeUTF8(b)
}

func FileWrite(_ *Fiber, path, text Value) Value {
	if err := os.WriteFile(path.(string), []byte(wellFormed(text.(string))), 0o666); err != nil {
		return Fail(NewData(writeErrorTag, nodeMessage(err, "open", path.(string))))
	}
	return Unit
}
