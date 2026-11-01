package flickrt

// ReadDirectoryChangesW on an I/O completion port, as libuv watches: a
// file is watched through its directory, its own changes picked by name.

import (
	"errors"
	"path/filepath"
	"strings"
	"sync"
	"syscall"
	"unsafe"
)

const watchNotify = syscall.FILE_NOTIFY_CHANGE_FILE_NAME | syscall.FILE_NOTIFY_CHANGE_DIR_NAME |
	syscall.FILE_NOTIFY_CHANGE_ATTRIBUTES | syscall.FILE_NOTIFY_CHANGE_SIZE |
	syscall.FILE_NOTIFY_CHANGE_LAST_WRITE | syscall.FILE_NOTIFY_CHANGE_LAST_ACCESS |
	syscall.FILE_NOTIFY_CHANGE_CREATION | 0x100 // FILE_NOTIFY_CHANGE_SECURITY

func watchNative(q *watchQueue, path string, dir, recursive bool) (func(), error) {
	name := ""
	if !dir {
		name = filepath.Base(path)
	}
	p, err := syscall.UTF16PtrFromString(q.base)
	if err != nil {
		return nil, err
	}
	h, err := syscall.CreateFile(p, syscall.FILE_LIST_DIRECTORY,
		syscall.FILE_SHARE_READ|syscall.FILE_SHARE_WRITE|syscall.FILE_SHARE_DELETE, nil,
		syscall.OPEN_EXISTING, syscall.FILE_FLAG_BACKUP_SEMANTICS|syscall.FILE_FLAG_OVERLAPPED, 0)
	if err != nil {
		return nil, err
	}
	port, err := syscall.CreateIoCompletionPort(h, 0, 0, 1)
	if err != nil {
		syscall.CloseHandle(h)
		return nil, err
	}
	buf := make([]uint32, 16<<10) // DWORD-aligned, as the records are
	ov := new(syscall.Overlapped)
	var mu sync.Mutex
	stopping := false
	// read asks for the next changes, unless the watch stops: under `mu`,
	// so a stop cancels any read asked for
	read := func() (bool, error) {
		mu.Lock()
		defer mu.Unlock()
		if stopping {
			return false, nil
		}
		return true, syscall.ReadDirectoryChanges(h, (*byte)(unsafe.Pointer(&buf[0])), uint32(len(buf)*4), recursive && dir, watchNotify, nil, ov, 0)
	}
	if _, err := read(); err != nil {
		syscall.CloseHandle(port)
		syscall.CloseHandle(h)
		return nil, err
	}
	done := make(chan struct{})
	go func() {
		defer close(done)
		for {
			var n, key uint32
			var o *syscall.Overlapped
			err := syscall.GetQueuedCompletionStatus(port, &n, &key, &o, syscall.INFINITE)
			if errors.Is(err, syscall.ERROR_OPERATION_ABORTED) {
				return
			}
			if err != nil {
				q.fail() // ERROR_ACCESS_DENIED: the directory was removed
				return
			}
			// none: more changed than the buffer holds, which Node drops too
			if n > 0 {
				watchRecords(q, unsafe.Slice((*byte)(unsafe.Pointer(&buf[0])), n), name)
			}
			if ok, err := read(); !ok {
				return
			} else if err != nil {
				q.fail()
				return
			}
		}
	}()
	return func() {
		mu.Lock()
		stopping = true
		syscall.CancelIoEx(h, ov)
		mu.Unlock()
		<-done // the cancelled read completed: the buffer is free
		syscall.CloseHandle(port)
		syscall.CloseHandle(h)
	}, nil
}

// watchRecords reports the changes of FILE_NOTIFY_INFORMATION records,
// only those of `name` when it is given.
func watchRecords(q *watchQueue, records []byte, name string) {
	for off := 0; off+12 <= len(records); {
		info := (*syscall.FileNotifyInformation)(unsafe.Pointer(&records[off]))
		rel := syscall.UTF16ToString(unsafe.Slice(&info.FileName, info.FileNameLength/2))
		if name == "" || strings.EqualFold(rel, name) {
			if info.Action == syscall.FILE_ACTION_MODIFIED {
				q.change(rel)
			} else {
				q.rename(rel)
			}
		}
		if info.NextEntryOffset == 0 {
			return
		}
		off += int(info.NextEntryOffset)
	}
}
