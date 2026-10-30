//go:build linux || darwin

package flickrt

import "syscall"

const fsNoFollowFlag = syscall.O_NOFOLLOW

func fsAccess(path string, readable, writable bool) error {
	var mode uint32
	if readable {
		mode |= 4
	}
	if writable {
		mode |= 2
	}
	return syscall.Access(path, mode)
}
