package flickrt

import (
	"crypto/rand"
	"encoding/hex"
	"errors"
	"io/fs"
	"math/big"
	"os"
	"path/filepath"
)

// FileSystem.node as Effect's NodeFileSystem: each operation of the Flick
// service `FileSystem.FileSystem`, failing with the Flick PlatformError of
// a system error, tagged by the errno Node reports.

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

// systemTagOf is Effect's tag of the errno of a failed call.
func systemTagOf(err error) string {
	if e, ok := nodeError(err); ok {
		switch e[0] {
		case "ENOENT":
			return "NotFound"
		case "EACCES":
			return "PermissionDenied"
		case "EEXIST":
			return "AlreadyExists"
		case "EISDIR", "ENOTDIR", "ELOOP":
			return "BadResource"
		case "EBUSY":
			return "Busy"
		}
	}
	return "UnknownError"
}

// systemError is the PlatformError of a failed call of `method`, as Node's
// `syscall` on `path` failed.
func systemError(err error, method, syscall, path string) Value {
	return systemErrorOf(err, "FileSystem", method, syscall, path)
}

// systemErrorOf is the PlatformError of a failed system call of a method
// of a module, about a path or a descriptor.
func systemErrorOf(err error, module, method, syscall, pathOrDescriptor string) Value {
	return NewData(platformErrorTag, NewData(systemReasonTag, Rec(
		"tag", Nullary(Tag("PlatformError.SystemErrorTag."+systemTagOf(err))),
		"module_", module, "method", method, "description", None,
		"syscall", Some(syscall), "pathOrDescriptor", Some(pathOrDescriptor))))
}

func isNotFound(platformError Value) bool {
	reason := platformError.(*Data).Fields[0].(*Data)
	return reason.Tag == systemReasonTag && reason.Fields[0].(*Record).Get("tag").(*Data).Tag == Tag("PlatformError.SystemErrorTag.NotFound")
}

// statInfo is what Node's `stat` tells beside Go's FileInfo: its mode
// with the type bits of `st_mode`, ids, sizes and times in milliseconds.
type statInfo struct {
	mode                    uint32
	dev, ino, nlink, rdev   *big.Int
	uid, gid                Value // Options of a Float
	blksize, blocks         Value // Options of a BigInt
	atime, mtime, birthtime float64
}

// infoOf is `FileSystem.Info` of a stat.
func infoOf(fi fs.FileInfo) Value { return infoWith(fi, nil, "") }

// infoWith is `FileSystem.Info` of a stat of the open file or the path,
// with the fields the system's stat structure gives.
func infoWith(fi fs.FileInfo, f *os.File, path string) Value {
	m := fi.Mode()
	typ, ok := fileTypeTags[m&(fs.ModeType|fs.ModeCharDevice)]
	if !ok {
		typ = "UnknownType"
	}
	sf, known := statFields(fi, f, path)
	if !known {
		// Node's mode carries the type bits of `st_mode` beside the permissions
		bits := uint32(m.Perm())
		switch typ {
		case "RegularFile":
			bits |= 0o100000
		case "Directory":
			bits |= 0o040000
		case "SymbolicLink":
			bits |= 0o120000
		}
		return Rec(
			"type_", Nullary(Tag("FileSystem.FileType."+typ)),
			"mtime", Some(float64(fi.ModTime().UnixMilli())), "atime", None, "birthtime", None,
			"dev", big.NewInt(0), "ino", None, "mode", int32(bits), "nlink", None, "uid", None, "gid", None,
			"rdev", None, "size", big.NewInt(fi.Size()), "blksize", None, "blocks", None)
	}
	return Rec(
		"type_", Nullary(Tag("FileSystem.FileType."+typ)),
		"mtime", Some(sf.mtime), "atime", Some(sf.atime), "birthtime", Some(sf.birthtime),
		"dev", sf.dev, "ino", Some(sf.ino), "mode", int32(sf.mode), "nlink", Some(sf.nlink),
		"uid", sf.uid, "gid", sf.gid, "rdev", Some(sf.rdev), "size", big.NewInt(fi.Size()),
		"blksize", sf.blksize, "blocks", sf.blocks)
}

// statPath is the info of the path, or the PlatformError of `method`.
func statPath(path, method string) (Value, Value) {
	fi, err := os.Stat(path)
	if err != nil {
		return nil, systemError(err, method, "stat", path)
	}
	return infoWith(fi, nil, path), nil
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
// of the call closes; a failure to is a defect, as Effect's `orDie`.
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
		"access", Curry2(fsAccess), "copy", Curry3(fsCopy), "copy_file", Curry2(fsCopyFile),
		"chmod", Curry2(fsChmod), "chown", Curry3(fsChown), "glob", Curry2(fsGlob),
		"exists", Fn(func(path Value) Value {
			return Eff(func(*Fiber) Value {
				_, err := os.Stat(path.(string))
				if err != nil && !errors.Is(err, fs.ErrNotExist) {
					return Fail(systemError(err, "access", "access", path.(string)))
				}
				return err == nil
			})
		}),
		"link", Curry2(fsLink),
		"make_directory", Fn(func(options Value) Value {
			return Fn(func(path Value) Value {
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
					if err != nil {
						return Fail(systemError(err, "makeDirectory", "mkdir", path.(string)))
					}
					return Unit
				})
			})
		}),
		"make_temp_directory", Fn(func(options Value) Value {
			return Eff(func(*Fiber) Value { return makeTempDirectory(options, "makeTempDirectory") })
		}),
		"make_temp_directory_scoped", Fn(func(options Value) Value {
			return Eff(func(fb *Fiber) Value {
				dir := makeTempDirectory(options, "makeTempDirectoryScoped")
				removeWhenClosed(fb, dir)
				return dir
			})
		}),
		"make_temp_file", Fn(func(options Value) Value {
			return Eff(func(*Fiber) Value { return makeTempFile(options, "makeTempFile") })
		}),
		"make_temp_file_scoped", Fn(func(options Value) Value {
			return Eff(func(fb *Fiber) Value {
				file := makeTempFile(options, "makeTempFileScoped")
				removeWhenClosed(fb, filepath.Dir(file))
				return file
			})
		}),
		"open", Curry2(fsOpen),
		"read_directory", Fn(func(recursive Value) Value {
			return Fn(func(path Value) Value {
				return Eff(func(*Fiber) Value { return readDirectory(recursive.(bool), path.(string)) })
			})
		}),
		"read_file", Fn(func(path Value) Value {
			return Eff(func(*Fiber) Value {
				b, err := os.ReadFile(path.(string))
				if err != nil {
					return Fail(systemError(err, "readFile", "open", path.(string)))
				}
				return byteList(b)
			})
		}),
		"read_file_string", Fn(func(path Value) Value {
			return Eff(func(*Fiber) Value {
				b, err := os.ReadFile(path.(string))
				if err != nil {
					return Fail(systemError(err, "readFile", "open", path.(string)))
				}
				return decodeText(b)
			})
		}),
		"read_link", Fn(fsReadLink), "real_path", Fn(fsRealPath),
		"remove", Fn(func(options Value) Value {
			return Fn(func(path Value) Value {
				return Eff(func(*Fiber) Value { return remove(options, path.(string)) })
			})
		}),
		"rename", Fn(func(from Value) Value {
			return Fn(func(to Value) Value {
				return Eff(func(*Fiber) Value {
					if err := os.Rename(from.(string), to.(string)); err != nil {
						return Fail(systemError(err, "rename", "rename", from.(string)))
					}
					return Unit
				})
			})
		}),
		"stat", Fn(func(path Value) Value {
			return Eff(func(*Fiber) Value {
				info, failure := statPath(path.(string), "stat")
				if failure != nil {
					return Fail(failure)
				}
				return info
			})
		}),
		"symlink", Curry2(fsSymlink), "truncate", Curry2(fsTruncate), "utimes", Curry3(fsUtimes),
		"write_file", Fn(func(options Value) Value {
			return Fn(func(path Value) Value {
				return Fn(func(data Value) Value {
					return Eff(func(*Fiber) Value { return writeFile(options, path.(string), []byte(byteString(data))) })
				})
			})
		}),
		"write_file_string", Fn(func(options Value) Value {
			return Fn(func(path Value) Value {
				return Fn(func(text Value) Value {
					return Eff(func(*Fiber) Value {
						return writeFile(options, path.(string), []byte(wellFormed(text.(string))))
					})
				})
			})
		}),
	)
	return LayerSucceed(key, impl)
}

// decodeText is the text of UTF-8 bytes as a `TextDecoder` reads it: a
// leading byte order mark dropped, an invalid byte a U+FFFD.
func decodeText(b []byte) string {
	if len(b) >= 3 && b[0] == 0xef && b[1] == 0xbb && b[2] == 0xbf {
		b = b[3:]
	}
	return decodeUTF8(b)
}
