package flickrt

import (
	"errors"
	"io"
	"math/big"
	"os"
	"strconv"
	"sync"
)

// An open file of `FileSystem.open`, as Effect's file of Node.js: it
// keeps its own position, which reads and writes start at and move; a file
// opened to append writes at its end and keeps its position.
type fsFile struct {
	f        *os.File
	append   bool
	mu       sync.Mutex
	position int64
}

// fileSystemKey is the service key of `FileSystem.FileSystem`.
const fileSystemKey = "FileSystem.FileSystem"

func fileOf(v Value) *fsFile { return v.(*fsFile) }

// fdError is the PlatformError of a failed call on the file's descriptor,
// which it names, as Effect names it.
func (h *fsFile) fdError(err error, method, syscall string) Value {
	return systemErrorOf(err, "FileSystem", method, syscall, strconv.FormatUint(uint64(h.f.Fd()), 10))
}

func badArgument(module, method, description string) Value {
	return NewData(platformErrorTag, NewData(Tag("PlatformError.Reason.Bad"),
		Rec("module_", module, "method", method, "description", Some(description))))
}

// openFile is Node's `open` of the path with the flags of the options,
// closed when the owner of the call closes.
func openFile(fb *Fiber, options Value, path string) Value {
	o := record(options)
	flag := Tag("FileSystem.OpenFlag.Read")
	if f, ok := some(o.Get("flag")); ok {
		flag = f.(*Data).Tag
	}
	flags := openFlags[flag]
	if o.Get("noFollow").(bool) {
		bit, ok := noFollow()
		if !ok {
			return Fail(badArgument("FileSystem", "open", "noFollow is not supported on this platform"))
		}
		flags |= bit
	}
	mode := uint32(0o666)
	if m, ok := some(o.Get("mode")); ok {
		mode = uint32(m.(int32))
	}
	acquire := Eff(func(*Fiber) Value {
		f, err := os.OpenFile(path, flags, fileMode(mode))
		if err != nil {
			return Fail(systemError(err, "open", "open", path))
		}
		return &fsFile{f: f, append: flags&os.O_APPEND != 0}
	})
	release := Fn(func(h Value) Value {
		return Eff(func(*Fiber) Value {
			if err := fileOf(h).f.Close(); err != nil {
				panic(Defect{err.Error()})
			}
			return Unit
		})
	})
	return AcquireRelease(fb, acquire, release)
}

func FileSystemFileStat(_ *Fiber, file Value) Value {
	h := fileOf(file)
	fi, err := h.f.Stat()
	if err != nil {
		return Fail(h.fdError(err, "stat", "fstat"))
	}
	return infoWith(fi, h.f, "")
}

// FileSystemSeek moves the position: from the start, or from where it is.
func FileSystemSeek(_ *Fiber, offset, from, file Value) Value {
	h := fileOf(file)
	h.mu.Lock()
	defer h.mu.Unlock()
	position := new(big.Int).Set(offset.(*big.Int))
	if ConstructorName(from.(*Data).Tag) == "Current" {
		position.Add(position, big.NewInt(h.position))
	}
	if position.Sign() < 0 {
		return Fail(badArgument("FileSystem", "seek", "Cannot seek before the start of the file"))
	}
	h.position = position.Int64()
	return position
}

func FileSystemSync(_ *Fiber, file Value) Value {
	h := fileOf(file)
	if err := h.f.Sync(); err != nil {
		return Fail(h.fdError(err, "sync", "fsync"))
	}
	return Unit
}

// FileSystemReadAlloc reads at most `size` bytes at the position given, or
// at the file's, which it moves; none at the end of the file.
func FileSystemReadAlloc(_ *Fiber, size, position, file Value) Value {
	h := fileOf(file)
	n := int(size.(int32))
	if n < 0 {
		return Fail(badArgument("FileSystem", "readAlloc", "size must be a non-negative integer"))
	}
	h.mu.Lock()
	defer h.mu.Unlock()
	at, explicit := some(position)
	pos := h.position
	if explicit {
		if at.(*big.Int).Sign() < 0 {
			return Fail(badArgument("FileSystem", "readAlloc", "Cannot read before the start of the file"))
		}
		pos = at.(*big.Int).Int64()
	}
	buf := make([]byte, n)
	read, err := h.f.ReadAt(buf, pos)
	if err != nil && !errors.Is(err, io.EOF) {
		return Fail(h.fdError(err, "readAlloc", "read"))
	}
	if read == 0 {
		return None
	}
	if !explicit {
		h.position = pos + int64(read)
	}
	return Some(byteList(buf[:read]))
}

func FileSystemFileTruncate(_ *Fiber, length, file Value) Value {
	h := fileOf(file)
	size := int64(0)
	if l, ok := some(length); ok {
		size = int64(l.(float64))
	}
	if err := h.f.Truncate(size); err != nil {
		return Fail(h.fdError(err, "truncate", "ftruncate"))
	}
	h.mu.Lock()
	if !h.append && h.position > size {
		h.position = size
	}
	h.mu.Unlock()
	return Unit
}

// write writes the bytes at the file's position, or at its end when it
// appends, and moves the position past them.
func (h *fsFile) write(b []byte, method string) (int, Value) {
	h.mu.Lock()
	defer h.mu.Unlock()
	var n int
	var err error
	if h.append {
		n, err = h.f.Write(b)
	} else {
		n, err = h.f.WriteAt(b, h.position)
		h.position += int64(n)
	}
	if err != nil {
		return n, h.fdError(err, method, "write")
	}
	return n, nil
}

func FileSystemWrite(_ *Fiber, data, file Value) Value {
	n, failure := fileOf(file).write([]byte(byteString(data)), "write")
	if failure != nil {
		return Fail(failure)
	}
	return int32(n)
}

func FileSystemWriteAll(_ *Fiber, data, file Value) Value {
	return fileOf(file).writeAll([]byte(byteString(data)))
}

// writeAll writes all the bytes, failing as Effect does when a write
// takes none.
func (h *fsFile) writeAll(b []byte) Value {
	for len(b) > 0 {
		n, failure := h.write(b, "writeAll")
		if failure != nil {
			return Fail(failure)
		}
		if n == 0 {
			return Fail(NewData(platformErrorTag, NewData(systemReasonTag, Rec(
				"tag", Nullary(Tag("PlatformError.SystemErrorTag.WriteZero")), "module_", "FileSystem",
				"method", "writeAll", "description", Some("write returned 0 bytes written"), "syscall", None,
				"pathOrDescriptor", Some(strconv.FormatUint(uint64(h.f.Fd()), 10))))))
		}
		b = b[n:]
	}
	return Unit
}
