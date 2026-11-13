//go:build !linux && !darwin && !windows

package flickrt

import (
	"io/fs"
	"os"
)

// shortcut: other systems give only what Go's FileInfo has; read their stat structures when Flick programs run there
func statFields(fs.FileInfo, *os.File, string) (statInfo, bool) { return statInfo{}, false }

func accessPath(path string, _, _ bool) error {
	_, err := os.Stat(path)
	return err
}

func chownPath(path string, uid, gid int) error { return os.Chown(path, uid, gid) }

func noFollow() (int, bool) { return 0, false }
