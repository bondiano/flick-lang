package flickrt

// An open file of `FileSystem.open`, as on Node: it keeps its own
// position, which reads and writes move unless the file appends.

import (
	"io"
	"math/big"
	"os"
	"sync"
)

type fsFile struct {
	f      *os.File
	append bool
	mu     sync.Mutex
	pos    int64
}

func fsFileOf(v Value) *fsFile { return v.(*fsFile) }

func (file *fsFile) fail(err error, method, syscall string) Value {
	return Fail(systemError(err, method, syscall, file.f.Name()))
}

func FileSystemFileStat(_ *Fiber, f Value) Value {
	file := fsFileOf(f)
	fi, err := file.f.Stat()
	if err != nil {
		return file.fail(err, "stat", "fstat")
	}
	return infoOf(fi)
}

// FileSystemSeek moves the position, from the start or from where it is.
func FileSystemSeek(_ *Fiber, offset, from, f Value) Value {
	file := fsFileOf(f)
	file.mu.Lock()
	defer file.mu.Unlock()
	pos := offset.(*big.Int).Int64()
	if from.(*Data).Tag != Tag("FileSystem.SeekMode.Start") {
		pos += file.pos
	}
	if pos < 0 {
		return Fail(fsBadArgument("seek", "Cannot seek before the start of the file"))
	}
	file.pos = pos
	return big.NewInt(pos)
}

func FileSystemSync(_ *Fiber, f Value) Value {
	file := fsFileOf(f)
	if err := file.f.Sync(); err != nil {
		return file.fail(err, "sync", "fsync")
	}
	return Unit
}

// FileSystemReadAlloc reads at most `size` bytes at the position given, or
// at the file's own, which it moves; None at the end.
func FileSystemReadAlloc(_ *Fiber, size, position, f Value) Value {
	file := fsFileOf(f)
	n := size.(int32)
	if n < 0 {
		return Fail(fsBadArgument("readAlloc", "size must be a non-negative integer"))
	}
	file.mu.Lock()
	defer file.mu.Unlock()
	pos, explicit := file.pos, false
	if p, ok := some(position); ok {
		pos, explicit = p.(*big.Int).Int64(), true
		if pos < 0 {
			return Fail(fsBadArgument("readAlloc", "position must be non-negative"))
		}
	}
	buf := make([]byte, n)
	read, err := file.f.ReadAt(buf, pos)
	if err != nil && err != io.EOF {
		return file.fail(err, "readAlloc", "read")
	}
	if read == 0 {
		return None
	}
	if !explicit {
		file.pos = pos + int64(read)
	}
	return Some(byteList(buf[:read]))
}

func FileSystemFileTruncate(_ *Fiber, length, f Value) Value {
	file := fsFileOf(f)
	var n int64
	if l, ok := some(length); ok {
		n = int64(l.(float64))
	}
	file.mu.Lock()
	defer file.mu.Unlock()
	if err := file.f.Truncate(n); err != nil {
		return file.fail(err, "truncate", "ftruncate")
	}
	if !file.append && file.pos > n {
		file.pos = n
	}
	return Unit
}

// write writes the bytes at the position, or at the end of a file that
// appends; how many it wrote.
func (file *fsFile) write(b []byte, method string) int {
	var n int
	var err error
	if file.append {
		n, err = file.f.Write(b)
	} else {
		n, err = file.f.WriteAt(b, file.pos)
		file.pos += int64(n)
	}
	if err != nil {
		file.fail(err, method, "write")
	}
	return n
}

func FileSystemWrite(_ *Fiber, data, f Value) Value {
	file := fsFileOf(f)
	file.mu.Lock()
	defer file.mu.Unlock()
	return int32(file.write([]byte(byteString(data)), "write"))
}

func FileSystemWriteAll(_ *Fiber, data, f Value) Value {
	file := fsFileOf(f)
	file.mu.Lock()
	defer file.mu.Unlock()
	file.write([]byte(byteString(data)), "writeAll")
	return Unit
}
