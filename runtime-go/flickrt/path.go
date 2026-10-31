package flickrt

// Effect's `Path`: its POSIX paths, adapted from Node.js as Effect adapts
// them. Separators and dots are ASCII, so slicing WTF-8 bytes at them cuts
// the same text as slicing UTF-16 code units in JS.

import (
	"os"
	"strings"
)

// pathNormalizeString resolves `.` and `..` segments, as Node's
// normalizeStringPosix.
func pathNormalizeString(path string, allowAboveRoot bool) string {
	res, lastSegment, lastSlash, dots := "", 0, -1, 0
	var code byte
	for i := 0; i <= len(path); i++ {
		if i < len(path) {
			code = path[i]
		} else if code == '/' {
			break
		} else {
			code = '/'
		}
		if code != '/' {
			if code == '.' && dots != -1 {
				dots++
			} else {
				dots = -1
			}
			continue
		}
		switch {
		case lastSlash == i-1 || dots == 1:
		case dots == 2:
			if len(res) < 2 || lastSegment != 2 || !strings.HasSuffix(res, "..") {
				if len(res) > 2 {
					if last := strings.LastIndexByte(res, '/'); last != len(res)-1 {
						if last == -1 {
							res, lastSegment = "", 0
						} else {
							res = res[:last]
							lastSegment = len(res) - 1 - strings.LastIndexByte(res, '/')
						}
						lastSlash, dots = i, 0
						continue
					}
				} else if len(res) == 2 || len(res) == 1 {
					res, lastSegment, lastSlash, dots = "", 0, i, 0
					continue
				}
			}
			if allowAboveRoot {
				if len(res) > 0 {
					res += "/.."
				} else {
					res = ".."
				}
				lastSegment = 2
			}
		default:
			if len(res) > 0 {
				res += "/" + path[lastSlash+1:i]
			} else {
				res = path[lastSlash+1 : i]
			}
			lastSegment = i - lastSlash - 1
		}
		lastSlash, dots = i, 0
	}
	return res
}

func pathResolve(paths ...string) string {
	resolved, absolute := "", false
	for i := len(paths) - 1; i >= -1 && !absolute; i-- {
		var p string
		if i >= 0 {
			p = paths[i]
		} else {
			p, _ = os.Getwd()
		}
		if p == "" {
			continue
		}
		resolved, absolute = p+"/"+resolved, p[0] == '/'
	}
	resolved = pathNormalizeString(resolved, !absolute)
	switch {
	case absolute:
		return "/" + resolved
	case resolved != "":
		return resolved
	}
	return "."
}

func pathNormalize(path string) string {
	if path == "" {
		return "."
	}
	absolute, trailing := path[0] == '/', path[len(path)-1] == '/'
	path = pathNormalizeString(path, !absolute)
	if path == "" && !absolute {
		path = "."
	}
	if path != "" && trailing {
		path += "/"
	}
	if absolute {
		return "/" + path
	}
	return path
}

func pathJoin(paths []string) string {
	joined, any := "", false
	for _, p := range paths {
		if p == "" {
			continue
		}
		if any {
			joined += "/" + p
		} else {
			joined, any = p, true
		}
	}
	if !any {
		return "."
	}
	return pathNormalize(joined)
}

// pathRootEnd is where the segments of an absolute path start.
func pathRootEnd(p string) int {
	i := 1
	for i < len(p) && p[i] == '/' {
		i++
	}
	return i
}

func pathRelative(from, to string) string {
	if from == to {
		return ""
	}
	from, to = pathResolve(from), pathResolve(to)
	if from == to {
		return ""
	}
	fromStart, toStart := pathRootEnd(from), pathRootEnd(to)
	fromLen, toLen := len(from)-fromStart, len(to)-toStart
	length := min(fromLen, toLen)
	lastCommonSep, i := -1, 0
	for ; i <= length; i++ {
		if i == length {
			if toLen > length {
				if to[toStart+i] == '/' {
					return to[toStart+i+1:]
				} else if i == 0 {
					return to[toStart+i:]
				}
			} else if fromLen > length {
				if from[fromStart+i] == '/' {
					lastCommonSep = i
				} else if i == 0 {
					lastCommonSep = 0
				}
			}
			break
		}
		if from[fromStart+i] != to[toStart+i] {
			break
		} else if from[fromStart+i] == '/' {
			lastCommonSep = i
		}
	}
	out := ""
	for i = fromStart + lastCommonSep + 1; i <= len(from); i++ {
		if i == len(from) || from[i] == '/' {
			if out == "" {
				out = ".."
			} else {
				out += "/.."
			}
		}
	}
	if out != "" {
		return out + to[toStart+lastCommonSep:]
	}
	toStart += lastCommonSep
	if toStart < len(to) && to[toStart] == '/' {
		toStart++
	}
	return to[toStart:]
}

func pathDirname(path string) string {
	if path == "" {
		return "."
	}
	hasRoot, end, matchedSlash := path[0] == '/', -1, true
	for i := len(path) - 1; i >= 1; i-- {
		if path[i] == '/' {
			if !matchedSlash {
				end = i
				break
			}
		} else {
			matchedSlash = false
		}
	}
	switch {
	case end == -1 && hasRoot:
		return "/"
	case end == -1:
		return "."
	case hasRoot && end == 1:
		return "//"
	}
	return path[:end]
}

func pathBasename(path, ext string) string {
	start, end, matchedSlash := 0, -1, true
	if ext != "" && len(ext) <= len(path) {
		if ext == path {
			return ""
		}
		extIdx, firstNonSlashEnd := len(ext)-1, -1
		for i := len(path) - 1; i >= 0; i-- {
			if path[i] == '/' {
				if !matchedSlash {
					start = i + 1
					break
				}
				continue
			}
			if firstNonSlashEnd == -1 {
				matchedSlash, firstNonSlashEnd = false, i+1
			}
			if extIdx >= 0 {
				if path[i] == ext[extIdx] {
					if extIdx--; extIdx == -1 {
						end = i
					}
				} else {
					extIdx, end = -1, firstNonSlashEnd
				}
			}
		}
		if start == end {
			end = firstNonSlashEnd
		} else if end == -1 {
			end = len(path)
		}
		return path[start:end]
	}
	for i := len(path) - 1; i >= 0; i-- {
		if path[i] == '/' {
			if !matchedSlash {
				start = i + 1
				break
			}
		} else if end == -1 {
			matchedSlash, end = false, i+1
		}
	}
	if end == -1 {
		return ""
	}
	return path[start:end]
}

// pathScan finds the last segment of `path` from `start` on: where it
// starts, ends and where its extension's dot is (-1 for none).
func pathScan(path string, start int) (startPart, startDot, end int) {
	startDot, end = -1, -1
	matchedSlash, preDotState := true, 0
	for i := len(path) - 1; i >= start; i-- {
		code := path[i]
		if code == '/' {
			if !matchedSlash {
				startPart = i + 1
				break
			}
			continue
		}
		if end == -1 {
			matchedSlash, end = false, i+1
		}
		if code == '.' {
			if startDot == -1 {
				startDot = i
			} else if preDotState != 1 {
				preDotState = 1
			}
		} else if startDot != -1 {
			preDotState = -1
		}
	}
	if startDot == -1 || end == -1 || preDotState == 0 ||
		preDotState == 1 && startDot == end-1 && startDot == startPart+1 {
		startDot = -1
	}
	return
}

func pathExtname(path string) string {
	_, dot, end := pathScan(path, 0)
	if dot == -1 {
		return ""
	}
	return path[dot:end]
}

func pathParse(path string) Value {
	root, dir, base, ext, name := "", "", "", "", ""
	if path != "" {
		absolute := path[0] == '/'
		start := 0
		if absolute {
			root, start = "/", 1
		}
		startPart, dot, end := pathScan(path, start)
		from := startPart
		if startPart == 0 && absolute {
			from = 1
		}
		if dot == -1 {
			if end != -1 {
				base, name = path[from:end], path[from:end]
			}
		} else {
			name, base, ext = path[from:dot], path[from:end], path[dot:end]
		}
		if startPart > 0 {
			dir = path[:startPart-1]
		} else if absolute {
			dir = "/"
		}
	}
	return Rec("root", root, "dir", dir, "base", base, "ext", ext, "name", name)
}

func pathFormat(parsed Value) string {
	p := record(parsed)
	root, dir := p.Get("root").(string), p.Get("dir").(string)
	if dir == "" {
		dir = root
	}
	base := p.Get("base").(string)
	if base == "" {
		base = p.Get("name").(string) + p.Get("ext").(string)
	}
	switch {
	case dir == "":
		return base
	case dir == root:
		return dir + base
	}
	return dir + "/" + base
}

func pathBadArgument(method, description string) Value {
	return NewData(platformErrorTag, NewData(Tag("PlatformError.Reason.Bad"), Rec(
		"module_", "Path", "method", method, "description", Some(description))))
}

func pathFromFileUrl(url string) Value {
	u, ok := parseURL(url)
	switch {
	case !ok:
		return Fail(pathBadArgument("fromFileUrl", "invalid URL "+url))
	case u.scheme != "file":
		return Fail(pathBadArgument("fromFileUrl", "URL must be of scheme file"))
	case u.host != "":
		return Fail(pathBadArgument("fromFileUrl", "Invalid file URL host"))
	}
	lower := strings.ToLower(u.path)
	if strings.Contains(lower, "%2f") {
		return Fail(pathBadArgument("fromFileUrl", "must not include encoded / characters"))
	}
	path, ok := decodeURIComponent(u.path)
	if !ok {
		return Fail(pathBadArgument("fromFileUrl", "invalid URL "+url))
	}
	return path
}

var pathURLChars = strings.NewReplacer("%", "%25", "\\", "%5C", "\n", "%0A", "\r", "%0D", "\t", "%09", "?", "%3F", "#", "%23")

func pathToFileUrl(path string) Value {
	resolved := pathResolve(path)
	if strings.HasSuffix(path, "/") && !strings.HasSuffix(resolved, "/") {
		resolved += "/"
	}
	// `?` and `#` are encoded too: the pathname setter keeps them in the path
	u, _ := parseURL("file://" + pathURLChars.Replace(resolved))
	return u.href()
}

func pathResolveWithin(root, input string) Value {
	if record(pathParse(input)).Get("root").(string) != "" {
		return None
	}
	base := pathResolve(root)
	candidate := pathResolve(base, input)
	rel := pathRelative(base, candidate)
	if rel == ".." || strings.HasPrefix(rel, "../") || strings.HasPrefix(rel, "/") {
		return None
	}
	return Some(candidate)
}

// PathLayer is `Path.layer`, Effect's POSIX paths.
func PathLayer(key Value) Value {
	return LayerSucceed(key, Rec(
		"sep", Fn(func(*Fiber, Value) Value { return Succeed("/") }),
		"basename", Fn(func(_ *Fiber, p Value) Value { return Succeed(pathBasename(p.(string), "")) }),
		"basename_without", Curry2(func(_ *Fiber, ext, p Value) Value { return Succeed(pathBasename(p.(string), ext.(string))) }),
		"dirname", Fn(func(_ *Fiber, p Value) Value { return Succeed(pathDirname(p.(string))) }),
		"extname", Fn(func(_ *Fiber, p Value) Value { return Succeed(pathExtname(p.(string))) }),
		"format", Fn(func(_ *Fiber, p Value) Value { return Succeed(pathFormat(p)) }),
		"from_file_url", Fn(func(_ *Fiber, url Value) Value {
			return Eff(func(*Fiber) Value { return pathFromFileUrl(url.(string)) })
		}),
		"is_absolute", Fn(func(_ *Fiber, p Value) Value { return Succeed(strings.HasPrefix(p.(string), "/")) }),
		"join", Fn(func(_ *Fiber, ps Value) Value { return Succeed(pathJoin(stringsOf(ps))) }),
		"normalize", Fn(func(_ *Fiber, p Value) Value { return Succeed(pathNormalize(p.(string))) }),
		"parse", Fn(func(_ *Fiber, p Value) Value { return Succeed(pathParse(p.(string))) }),
		"relative", Curry2(func(_ *Fiber, from, to Value) Value { return Succeed(pathRelative(from.(string), to.(string))) }),
		"resolve", Fn(func(_ *Fiber, ps Value) Value {
			return Eff(func(*Fiber) Value { return pathResolve(stringsOf(ps)...) })
		}),
		"resolve_within", Curry2(func(_ *Fiber, root, input Value) Value {
			return Eff(func(*Fiber) Value { return pathResolveWithin(root.(string), input.(string)) })
		}),
		"to_file_url", Fn(func(_ *Fiber, p Value) Value {
			return Eff(func(*Fiber) Value { return pathToFileUrl(p.(string)) })
		}),
		"to_namespaced_path", Fn(func(_ *Fiber, p Value) Value { return Succeed(p) }),
	))
}

// PathNode is `Path.node`.
// shortcut: POSIX paths on every OS, as Node's on Linux and macOS; Node's
// win32 paths on Windows when G12 runs the platform tests there.
func PathNode(key Value) Value { return PathLayer(key) }
