package flickrt

import (
	"crypto/rand"
	"encoding/hex"
	"errors"
	"io/fs"
	"math"
	"math/big"
	"os"
	"path/filepath"
	"strings"
	"syscall"
	"time"
)

// FileSystem.node: each operation of the Flick service
// `FileSystem.FileSystem`, failing with the Flick PlatformError of a system
// error, tagged by the errno Node reports.

var (
	platformErrorTag = Tag("PlatformError.PlatformError.PlatformError")
	systemReasonTag  = Tag("PlatformError.Reason.System")
	fileTypeTags     = map[fs.FileMode]string{
		0:                                 "RegularFile",
		fs.ModeDir:                        "Directory",
		fs.ModeSymlink:                    "SymbolicLink",
		fs.ModeDevice:                     "BlockDevice",
		fs.ModeDevice | fs.ModeCharDevice: "CharacterDevice",
		fs.ModeNamedPipe:                  "Fifo",
		fs.ModeSocket:                     "Socket",
	}
)

// systemTagOf is the tag of the errno of a failed call.
func systemTagOf(err error) string {
	if errors.Is(err, syscall.ELOOP) {
		return "BadResource"
	}
	if errors.Is(err, syscall.EBUSY) {
		return "Busy"
	}
	if errors.Is(err, syscall.EPERM) {
		return "UnknownError"
	}
	if e, ok := nodeError(err); ok {
		switch e[0] {
		case "ENOENT":
			return "NotFound"
		case "EACCES":
			return "PermissionDenied"
		case "EEXIST":
			return "AlreadyExists"
		case "EISDIR", "ENOTDIR":
			return "BadResource"
		}
	}
	return "UnknownError"
}

// systemError is the PlatformError of a failed call of `method`, as Node's
// `syscall` on `path` failed.
func systemError(err error, method, syscall, path string) Value {
	return NewData(platformErrorTag, NewData(systemReasonTag, Rec(
		"tag", Nullary(Tag("PlatformError.SystemErrorTag."+systemTagOf(err))),
		"module_", "FileSystem", "method", method, "description", None,
		"syscall", Some(syscall), "pathOrDescriptor", Some(path))))
}

func fsBadArgument(method, description string) Value {
	return NewData(platformErrorTag, NewData(Tag("PlatformError.Reason.Bad"), Rec(
		"module_", "FileSystem", "method", method, "description", Some(description))))
}

func fsCheckPaths(method string, paths ...string) {
	for _, path := range paths {
		if strings.IndexByte(path, 0) >= 0 {
			Fail(fsBadArgument(method, "path must not contain null bytes"))
		}
	}
}

func fsDone(err error, method, call, path string) Value {
	if err != nil {
		return Fail(systemError(err, method, call, path))
	}
	return Unit
}

func fsMode(mode int32, method string) fs.FileMode {
	if mode < 0 {
		Fail(fsBadArgument(method, "mode must be non-negative"))
	}
	out := fs.FileMode(mode & 0o777)
	if mode&0o4000 != 0 {
		out |= fs.ModeSetuid
	}
	if mode&0o2000 != 0 {
		out |= fs.ModeSetgid
	}
	if mode&0o1000 != 0 {
		out |= fs.ModeSticky
	}
	return out
}

func fsOwnerID(id float64) int {
	if id != math.Trunc(id) || id < -1 || id > math.MaxUint32 {
		Fail(fsBadArgument("chown", "uid and gid must be integers from -1 to 4294967295"))
	}
	return int(id)
}

func fsTruncate(length Value, path string) Value {
	fsCheckPaths("truncate", path)
	var size float64
	if n, ok := some(length); ok {
		size = n.(float64)
		if size != math.Trunc(size) || math.Abs(size) > maxSafe {
			return Fail(fsBadArgument("truncate", "length must be a safe integer"))
		}
	}
	return fsDone(os.Truncate(path, int64(max(size, 0))), "truncate", "ftruncate", path)
}

func fsUtimes(atime, mtime float64, path string) Value {
	fsCheckPaths("utime", path)
	if atime != atime || mtime != mtime || math.Abs(atime) > 8.64e15 || math.Abs(mtime) > 8.64e15 {
		return Fail(systemError(syscall.EINVAL, "utime", "utime", path))
	}
	// Flick's JS adapter constructs Dates, which clip fractional milliseconds.
	return fsDone(os.Chtimes(path, time.UnixMilli(int64(atime)), time.UnixMilli(int64(mtime))), "utime", "utime", path)
}

func fsOpen(fb *Fiber, options Value, path string) Value {
	fsCheckPaths("open", path)
	o := record(options)
	flags := openFlags[Tag("FileSystem.OpenFlag.Read")]
	if flag, ok := some(o.Get("flag")); ok {
		flags = openFlags[flag.(*Data).Tag]
	}
	mode := fs.FileMode(0o666)
	if m, ok := some(o.Get("mode")); ok {
		mode = fsMode(m.(int32), "open")
	}
	if o.Get("noFollow").(bool) {
		if fsNoFollowFlag == 0 {
			return Fail(fsBadArgument("open", "noFollow is not supported on this platform"))
		}
		flags |= fsNoFollowFlag
	}
	acquire := Eff(func(*Fiber) Value {
		file, err := os.OpenFile(path, flags, mode)
		if err != nil {
			return Fail(systemError(err, "open", "open", path))
		}
		return &fsFile{f: file, append: flags&os.O_APPEND != 0}
	})
	release := Fn(func(_ *Fiber, file Value) Value {
		return Eff(func(*Fiber) Value {
			if err := fsFileOf(file).f.Close(); err != nil {
				panic(Defect{nodeMessage(err, "close", path)})
			}
			return Unit
		})
	})
	return AcquireRelease(fb, acquire, release)
}

func isNotFound(platformError Value) bool {
	reason := platformError.(*Data).Fields()[0].(*Data)
	return reason.Tag == systemReasonTag && reason.Fields()[0].(*Record).Get("tag").(*Data).Tag == Tag("PlatformError.SystemErrorTag.NotFound")
}

// infoOf is `FileSystem.Info` of what Go's stat gives.
func infoOf(fi fs.FileInfo) Value {
	m := fi.Mode()
	typ, ok := fileTypeTags[m&(fs.ModeType|fs.ModeCharDevice)]
	if !ok {
		typ = "UnknownType"
	}
	// Node's mode carries the type bits of `st_mode` beside the permissions
	bits := uint32(m.Perm())
	for flag, b := range map[fs.FileMode]uint32{fs.ModeSetuid: 0o4000, fs.ModeSetgid: 0o2000, fs.ModeSticky: 0o1000} {
		if m&flag != 0 {
			bits |= b
		}
	}
	switch typ {
	case "RegularFile":
		bits |= 0o100000
	case "Directory":
		bits |= 0o040000
	case "SymbolicLink":
		bits |= 0o120000
	}
	dev, ino, nlink, uid, gid, rdev, blksize, blocks, birth := fsStatSys(fi)
	return Rec(
		"type_", Nullary(Tag("FileSystem.FileType."+typ)),
		"mtime", Some(float64(fi.ModTime().UnixMilli())), "atime", Some(float64(fsAccessTime(fi).UnixMilli())), "birthtime", birth,
		"dev", dev, "ino", ino, "mode", int32(bits), "nlink", nlink, "uid", uid, "gid", gid,
		"rdev", rdev, "size", big.NewInt(fi.Size()), "blksize", blksize, "blocks", blocks)
}

// statPath is the info of the path, or the PlatformError of `method`.
func statPath(path, method string) (Value, Value) {
	fi, err := os.Stat(path)
	if err != nil {
		return nil, systemError(err, method, "stat", path)
	}
	return infoOf(fi), nil
}

// openFlags are the flags of Node's `OpenFlag` names, by Flick constructor.
var openFlags = map[string]int{
	Tag("FileSystem.OpenFlag.Read"):                os.O_RDONLY,
	Tag("FileSystem.OpenFlag.ReadWrite"):           os.O_RDWR,
	Tag("FileSystem.OpenFlag.Write"):               os.O_TRUNC | os.O_CREATE | os.O_WRONLY,
	Tag("FileSystem.OpenFlag.WriteExclusive"):      os.O_TRUNC | os.O_CREATE | os.O_WRONLY | os.O_EXCL,
	Tag("FileSystem.OpenFlag.WriteRead"):           os.O_TRUNC | os.O_CREATE | os.O_RDWR,
	Tag("FileSystem.OpenFlag.WriteReadExclusive"):  os.O_TRUNC | os.O_CREATE | os.O_RDWR | os.O_EXCL,
	Tag("FileSystem.OpenFlag.Append"):              os.O_APPEND | os.O_CREATE | os.O_WRONLY,
	Tag("FileSystem.OpenFlag.AppendExclusive"):     os.O_APPEND | os.O_CREATE | os.O_WRONLY | os.O_EXCL,
	Tag("FileSystem.OpenFlag.AppendRead"):          os.O_APPEND | os.O_CREATE | os.O_RDWR,
	Tag("FileSystem.OpenFlag.AppendReadExclusive"): os.O_APPEND | os.O_CREATE | os.O_RDWR | os.O_EXCL,
}

// writeFile writes the data with the options of `WriteOptions`.
func writeFile(options Value, path string, data []byte) Value {
	o := record(options)
	flags := openFlags[Tag("FileSystem.OpenFlag.Write")]
	if f, ok := some(o.Get("flag")); ok {
		flags = openFlags[f.(*Data).Tag]
	}
	mode := fs.FileMode(0o666)
	if m, ok := some(o.Get("mode")); ok {
		mode = fs.FileMode(m.(int32))
	}
	f, err := os.OpenFile(path, flags, mode)
	if err == nil {
		_, err = f.Write(data)
		if cerr := f.Close(); err == nil {
			err = cerr
		}
	}
	if err != nil {
		return Fail(systemError(err, "writeFile", "open", path))
	}
	return Unit
}

// makeTempDirectory is Node's `mkdtemp` of the prefix in the directory,
// the system's temporary one unless given.
func makeTempDirectory(options Value, method string) string {
	o := record(options)
	dir := os.TempDir()
	if d, ok := some(o.Get("directory")); ok {
		dir = d.(string)
	}
	prefix := ""
	if p, ok := some(o.Get("prefix")); ok {
		prefix = p.(string)
	}
	// a prefix may name directories below the directory, as Node joins them
	full := filepath.Join(dir, prefix)
	parent, base := filepath.Dir(full), filepath.Base(full)
	if prefix == "" {
		parent, base = dir, ""
	}
	path, err := os.MkdirTemp(parent, base)
	if err != nil {
		Fail(systemError(err, method, "mkdtemp", full))
	}
	return path
}

// makeTempFile is an empty file of a random name in a new temporary
// directory.
func makeTempFile(options Value, method string) string {
	dir := makeTempDirectory(options, method)
	random := make([]byte, 6)
	rand.Read(random)
	name := hex.EncodeToString(random)
	if s, ok := some(record(options).Get("suffix")); ok {
		name += s.(string)
	}
	path := filepath.Join(dir, name)
	if err := os.WriteFile(path, nil, 0o666); err != nil {
		Fail(systemError(err, method, "open", path))
	}
	return path
}

// removeWhenClosed removes the path and what is below it when the owner
// of the call closes; a failure to is a defect.
func removeWhenClosed(fb *Fiber, path string) {
	fb.owner.add(fb, func(*Fiber, *Cause) Value {
		if err := os.RemoveAll(path); err != nil {
			panic(Defect{err.Error()})
		}
		return Unit
	})
}

// remove is Node's `rm`: a directory only when recursive, a missing path
// an error unless forced.
func remove(options Value, path string) Value {
	o := record(options)
	fi, err := os.Lstat(path)
	switch {
	case err != nil && o.Get("force").(bool) && errors.Is(err, fs.ErrNotExist):
		return Unit
	case err != nil:
		return Fail(systemError(err, "remove", "rm", path))
	case fi.IsDir() && !o.Get("recursive").(bool):
		return Fail(systemError(errors.New("is a directory"), "remove", "rm", path))
	}
	if err := os.RemoveAll(path); err != nil {
		return Fail(systemError(err, "remove", "rm", path))
	}
	return Unit
}

// readDirectory is the names in the directory, or with `recursive` the
// paths below it relative to it, sorted.
func readDirectory(recursive bool, path string) Value {
	out := []Value{}
	err := filepath.WalkDir(path, func(p string, d fs.DirEntry, err error) error {
		if err != nil || p == path {
			return err
		}
		rel, _ := filepath.Rel(path, p)
		out = append(out, rel)
		if d.IsDir() && !recursive {
			return filepath.SkipDir
		}
		return nil
	})
	if err != nil {
		return Fail(systemError(err, "readDirectory", "scandir", path))
	}
	return out
}

// FileSystemNode is the layer of the service of key `key`.
func FileSystemNode(key Value) Value {
	impl := Rec(
		"access", Curry2(func(_ *Fiber, options, path Value) Value {
			return Eff(func(*Fiber) Value {
				p, o := path.(string), record(options)
				fsCheckPaths("access", p)
				return fsDone(fsAccess(p, o.Get("readable").(bool), o.Get("writable").(bool)), "access", "access", p)
			})
		}),
		"copy", Curry3(func(_ *Fiber, options, from, to Value) Value {
			return Eff(func(*Fiber) Value {
				src, dst, o := from.(string), to.(string), record(options)
				fsCheckPaths("copy", src, dst)
				return fsDone(fsCopy(src, dst, o.Get("overwrite").(bool), o.Get("preserveTimestamps").(bool)), "copy", "cp", src)
			})
		}),
		"copy_file", Curry2(func(_ *Fiber, from, to Value) Value {
			return Eff(func(*Fiber) Value {
				src, dst := from.(string), to.(string)
				fsCheckPaths("copyFile", src, dst)
				_, err := fsCopyFile(src, dst, false)
				return fsDone(err, "copyFile", "copyfile", src)
			})
		}),
		"chmod", Curry2(func(_ *Fiber, mode, path Value) Value {
			return Eff(func(*Fiber) Value {
				p := path.(string)
				fsCheckPaths("chmod", p)
				return fsDone(os.Chmod(p, fsMode(mode.(int32), "chmod")), "chmod", "chmod", p)
			})
		}),
		"chown", Curry3(func(_ *Fiber, uid, gid, path Value) Value {
			return Eff(func(*Fiber) Value {
				p := path.(string)
				fsCheckPaths("chown", p)
				return fsDone(os.Chown(p, fsOwnerID(uid.(float64)), fsOwnerID(gid.(float64))), "chown", "chown", p)
			})
		}),
		"glob", Curry2(func(_ *Fiber, options, pattern Value) Value {
			return Eff(func(fb *Fiber) Value { return fsGlob(fb, options, pattern.(string)) })
		}),
		"exists", Fn(func(_ *Fiber, path Value) Value {
			return Eff(func(*Fiber) Value {
				_, err := os.Stat(path.(string))
				if err != nil && !errors.Is(err, fs.ErrNotExist) {
					return Fail(systemError(err, "access", "access", path.(string)))
				}
				return err == nil
			})
		}),
		"link", Curry2(func(_ *Fiber, from, to Value) Value {
			return Eff(func(*Fiber) Value {
				src, dst := from.(string), to.(string)
				fsCheckPaths("link", src, dst)
				return fsDone(os.Link(src, dst), "link", "link", src)
			})
		}),
		"make_directory", Curry2(func(_ *Fiber, options, path Value) Value {
			return Eff(func(*Fiber) Value {
				o := record(options)
				mode := fs.FileMode(0o777)
				if m, ok := some(o.Get("mode")); ok {
					mode = fs.FileMode(m.(int32))
				}
				var err error
				if o.Get("recursive").(bool) {
					err = os.MkdirAll(path.(string), mode)
				} else {
					err = os.Mkdir(path.(string), mode)
				}
				return fsDone(err, "makeDirectory", "mkdir", path.(string))
			})
		}),
		"make_temp_directory", Fn(func(_ *Fiber, options Value) Value {
			return Eff(func(*Fiber) Value { return makeTempDirectory(options, "makeTempDirectory") })
		}),
		"make_temp_directory_scoped", Fn(func(_ *Fiber, options Value) Value {
			return Eff(func(fb *Fiber) Value {
				dir := makeTempDirectory(options, "makeTempDirectoryScoped")
				removeWhenClosed(fb, dir)
				return dir
			})
		}),
		"make_temp_file", Fn(func(_ *Fiber, options Value) Value {
			return Eff(func(*Fiber) Value { return makeTempFile(options, "makeTempFile") })
		}),
		"make_temp_file_scoped", Fn(func(_ *Fiber, options Value) Value {
			return Eff(func(fb *Fiber) Value {
				file := makeTempFile(options, "makeTempFileScoped")
				removeWhenClosed(fb, filepath.Dir(file))
				return file
			})
		}),
		"open", Curry2(func(_ *Fiber, options, path Value) Value {
			return Eff(func(fb *Fiber) Value { return fsOpen(fb, options, path.(string)) })
		}),
		"read_directory", Curry2(func(_ *Fiber, recursive, path Value) Value {
			return Eff(func(*Fiber) Value { return readDirectory(recursive.(bool), path.(string)) })
		}),
		"read_file", Fn(func(_ *Fiber, path Value) Value {
			return Eff(func(*Fiber) Value {
				b, err := os.ReadFile(path.(string))
				if err != nil {
					return Fail(systemError(err, "readFile", "open", path.(string)))
				}
				return byteList(b)
			})
		}),
		"read_file_string", Fn(func(_ *Fiber, path Value) Value {
			return Eff(func(*Fiber) Value {
				b, err := os.ReadFile(path.(string))
				if err != nil {
					return Fail(systemError(err, "readFile", "open", path.(string)))
				}
				return decodeText(b)
			})
		}),
		"read_link", Fn(func(_ *Fiber, path Value) Value {
			return Eff(func(*Fiber) Value {
				p := path.(string)
				fsCheckPaths("readLink", p)
				target, err := os.Readlink(p)
				if err != nil {
					return Fail(systemError(err, "readLink", "readlink", p))
				}
				return target
			})
		}),
		"real_path", Fn(func(_ *Fiber, path Value) Value {
			return Eff(func(*Fiber) Value {
				p := path.(string)
				fsCheckPaths("realPath", p)
				canonical, err := filepath.EvalSymlinks(p)
				if err == nil {
					canonical, err = filepath.Abs(canonical)
				}
				if err != nil {
					return Fail(systemError(err, "realPath", "realpath", p))
				}
				return canonical
			})
		}),
		"remove", Curry2(func(_ *Fiber, options, path Value) Value {
			return Eff(func(*Fiber) Value { return remove(options, path.(string)) })
		}),
		"rename", Curry2(func(_ *Fiber, from, to Value) Value {
			return Eff(func(*Fiber) Value {
				return fsDone(os.Rename(from.(string), to.(string)), "rename", "rename", from.(string))
			})
		}),
		"stat", Fn(func(_ *Fiber, path Value) Value {
			return Eff(func(*Fiber) Value {
				info, failure := statPath(path.(string), "stat")
				if failure != nil {
					return Fail(failure)
				}
				return info
			})
		}),
		"symlink", Curry2(func(_ *Fiber, from, to Value) Value {
			return Eff(func(*Fiber) Value {
				target, path := from.(string), to.(string)
				fsCheckPaths("symlink", target, path)
				return fsDone(os.Symlink(target, path), "symlink", "symlink", target)
			})
		}),
		"truncate", Curry2(func(_ *Fiber, length, path Value) Value {
			return Eff(func(*Fiber) Value { return fsTruncate(length, path.(string)) })
		}),
		"utimes", Curry3(func(_ *Fiber, atime, mtime, path Value) Value {
			return Eff(func(*Fiber) Value { return fsUtimes(atime.(float64), mtime.(float64), path.(string)) })
		}),
		"write_file", Curry3(func(_ *Fiber, options, path, data Value) Value {
			return Eff(func(*Fiber) Value { return writeFile(options, path.(string), []byte(byteString(data))) })
		}),
		"write_file_string", Curry3(func(_ *Fiber, options, path, text Value) Value {
			return Eff(func(*Fiber) Value { return writeFile(options, path.(string), []byte(wellFormed(text.(string)))) })
		}),
	)
	return LayerSucceed(key, markNode(impl))
}

// decodeText is the text of UTF-8 bytes as a `TextDecoder` reads it: a
// leading byte order mark dropped, an invalid byte a U+FFFD.
func decodeText(b []byte) string {
	if len(b) >= 3 && b[0] == 0xef && b[1] == 0xbb && b[2] == 0xbf {
		b = b[3:]
	}
	return decodeUTF8(b)
}
